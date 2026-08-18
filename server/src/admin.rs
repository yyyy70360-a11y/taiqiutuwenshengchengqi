use crate::{auth, AppState};
use axum::{
    extract::{Form, Path, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

const SESSION_COOKIE: &str = "billiards_admin_session";
const SESSION_HOURS: i64 = 8;

#[derive(Debug)]
struct AdminSession {
    id: String,
    csrf_token_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct CsrfForm {
    csrf: String,
}

#[derive(Debug)]
struct DashboardStats {
    total_users: i64,
    disabled_users: i64,
    today_items: i64,
    today_failures: i64,
}

pub async fn login_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !admin_configured(&state) {
        return html_response(layout("后台未配置", "", "", &admin_not_configured_html()));
    }
    if load_session(&state, &headers).await.is_some() {
        return Redirect::to("/admin").into_response();
    }
    html_response(login_html("", &state.config.admin_email))
}

pub async fn login(State(state): State<AppState>, Form(input): Form<LoginForm>) -> Response {
    if !admin_configured(&state) {
        return html_response(layout("后台未配置", "", "", &admin_not_configured_html()));
    }
    let email = input.email.trim().to_lowercase();
    let valid = email == state.config.admin_email
        && auth::verify_password(&input.password, &state.config.admin_password_hash);
    if !valid {
        return html_response(login_html("邮箱或密码错误", &state.config.admin_email));
    }
    let token = new_token();
    let session_id = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::hours(SESSION_HOURS);
    if let Err(error) =
        sqlx::query("INSERT INTO admin_sessions (id, token_hash, expires_at) VALUES ($1, $2, $3)")
            .bind(&session_id)
            .bind(auth::hash_token(&token))
            .bind(expires_at)
            .execute(&state.db)
            .await
    {
        tracing::error!(error = %error, "admin login session insert failed");
        return html_response(login_html(
            "后台登录失败，请稍后重试",
            &state.config.admin_email,
        ));
    }
    let mut response = Redirect::to("/admin").into_response();
    set_cookie(
        &mut response,
        &session_cookie(&token, SESSION_HOURS * 60 * 60),
    );
    response
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<CsrfForm>,
) -> Response {
    if let Some(session) = load_session(&state, &headers).await {
        if verify_csrf(&session, &input.csrf) {
            let _ = sqlx::query("UPDATE admin_sessions SET revoked_at = NOW() WHERE id = $1")
                .bind(session.id)
                .execute(&state.db)
                .await;
        } else {
            return html_response(layout(
                "请求已过期",
                &state.config.admin_email,
                "dashboard",
                "<p class=\"error\">页面已过期，请返回首页重试。</p>",
            ));
        }
    }
    let mut response = Redirect::to("/admin/login").into_response();
    set_cookie(&mut response, &expired_session_cookie());
    response
}

pub async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let stats = match dashboard_stats(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let recent_errors = match recent_error_rows(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "管理首页",
        &state.config.admin_email,
        "dashboard",
        &dashboard_html(&state, &stats, &recent_errors, &csrf),
    ))
}

pub async fn users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let rows = match user_rows(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "用户管理",
        &state.config.admin_email,
        "users",
        &users_html(&rows, &csrf),
    ))
}

pub async fn disable_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(input): Form<CsrfForm>,
) -> Response {
    update_user_disabled(state, headers, user_id, input.csrf, true).await
}

pub async fn enable_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(input): Form<CsrfForm>,
) -> Response {
    update_user_disabled(state, headers, user_id, input.csrf, false).await
}

pub async fn ai_usage(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if require_session(&state, &headers).await.is_none() {
        return Redirect::to("/admin/login").into_response();
    }
    let rows = match ai_usage_rows(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "AI 调用记录",
        &state.config.admin_email,
        "ai",
        &ai_usage_html(&rows),
    ))
}

async fn update_user_disabled(
    state: AppState,
    headers: HeaderMap,
    user_id: String,
    csrf: String,
    disabled: bool,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if !verify_csrf(&session, &csrf) {
        return html_response(layout(
            "请求已过期",
            &state.config.admin_email,
            "users",
            "<p class=\"error\">页面已过期，请返回用户管理页重试。</p>",
        ));
    }
    if let Err(error) = sqlx::query("UPDATE users SET disabled = $1 WHERE id = $2")
        .bind(disabled)
        .bind(user_id)
        .execute(&state.db)
        .await
    {
        tracing::error!(error = %error, "admin user status update failed");
        return html_response(layout(
            "用户管理",
            &state.config.admin_email,
            "users",
            "<p class=\"error\">更新用户状态失败。</p>",
        ));
    }
    Redirect::to("/admin/users").into_response()
}

async fn require_session(state: &AppState, headers: &HeaderMap) -> Option<AdminSession> {
    if !admin_configured(state) {
        return None;
    }
    load_session(state, headers).await
}

async fn load_session(state: &AppState, headers: &HeaderMap) -> Option<AdminSession> {
    let token = cookie_value(headers, SESSION_COOKIE)?;
    let row = sqlx::query(
        "SELECT id, csrf_token_hash FROM admin_sessions WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > NOW()",
    )
    .bind(auth::hash_token(&token))
    .fetch_optional(&state.db)
    .await
    .ok()??;
    Some(AdminSession {
        id: row.get("id"),
        csrf_token_hash: row.get("csrf_token_hash"),
    })
}

async fn rotate_csrf(state: &AppState, session: &AdminSession) -> Result<String, Response> {
    let token = new_token();
    sqlx::query("UPDATE admin_sessions SET csrf_token_hash = $1 WHERE id = $2")
        .bind(auth::hash_token(&token))
        .bind(&session.id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "admin csrf update failed");
            html_response(layout(
                "后台错误",
                &state.config.admin_email,
                "",
                "<p class=\"error\">后台页面初始化失败。</p>",
            ))
        })?;
    Ok(token)
}

fn verify_csrf(session: &AdminSession, csrf: &str) -> bool {
    !csrf.trim().is_empty() && auth::hash_token(csrf) == session.csrf_token_hash
}

async fn dashboard_stats(state: &AppState) -> Result<DashboardStats, Response> {
    let users = sqlx::query("SELECT COUNT(*)::BIGINT AS total, COUNT(*) FILTER (WHERE disabled)::BIGINT AS disabled FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(|error| internal_page(state, error, "读取用户统计失败"))?;
    let usage = sqlx::query("SELECT COALESCE(SUM(item_count), 0)::BIGINT AS items, COUNT(*) FILTER (WHERE status <> 'success')::BIGINT AS failures FROM usage_records WHERE created_at >= CURRENT_DATE")
        .fetch_one(&state.db)
        .await
        .map_err(|error| internal_page(state, error, "读取 AI 统计失败"))?;
    Ok(DashboardStats {
        total_users: users.get("total"),
        disabled_users: users.get("disabled"),
        today_items: usage.get("items"),
        today_failures: usage.get("failures"),
    })
}

async fn recent_error_rows(state: &AppState) -> Result<Vec<String>, Response> {
    let rows = sqlx::query(
        "SELECT u.email, r.operation, r.error_message, r.created_at FROM usage_records r JOIN users u ON u.id = r.user_id WHERE r.status <> 'success' ORDER BY r.created_at DESC LIMIT 5",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取最近错误失败"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let email: String = row.get("email");
            let operation: String = row.get("operation");
            let error_message: String = row.get("error_message");
            let created_at: DateTime<Utc> = row.get("created_at");
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&format_time(created_at)),
                escape_html(&email),
                escape_html(&operation),
                escape_html(&error_message)
            )
        })
        .collect())
}

async fn user_rows(state: &AppState) -> Result<Vec<String>, Response> {
    let rows = sqlx::query(
        "SELECT u.id, u.email, u.disabled, u.created_at, u.last_login_at, COALESCE(SUM(r.item_count), 0)::BIGINT AS items, COUNT(r.id)::BIGINT AS calls FROM users u LEFT JOIN usage_records r ON r.user_id = u.id GROUP BY u.id ORDER BY u.created_at DESC LIMIT 200",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取用户列表失败"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let id: String = row.get("id");
            let email: String = row.get("email");
            let disabled: bool = row.get("disabled");
            let created_at: DateTime<Utc> = row.get("created_at");
            let last_login_at: Option<DateTime<Utc>> = row.get("last_login_at");
            let items: i64 = row.get("items");
            let calls: i64 = row.get("calls");
            let status = if disabled { "<span class=\"badge bad\">已封禁</span>" } else { "<span class=\"badge ok\">正常</span>" };
            let action = if disabled { "enable" } else { "disable" };
            let action_label = if disabled { "解封" } else { "封禁" };
            let action_class = if disabled { "secondary" } else { "danger" };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><form method=\"post\" action=\"/admin/users/{}/{}\"><input type=\"hidden\" name=\"csrf\" value=\"__CSRF__\"><button class=\"{}\" type=\"submit\">{}</button></form></td></tr>",
                escape_html(&email),
                status,
                escape_html(&format_time(created_at)),
                escape_html(&last_login_at.map(format_time).unwrap_or_else(|| "未登录".into())),
                calls,
                items,
                escape_html(&id),
                action,
                action_class,
                action_label
            )
        })
        .collect())
}

async fn ai_usage_rows(state: &AppState) -> Result<Vec<String>, Response> {
    let rows = sqlx::query(
        "SELECT u.email, r.operation, r.item_count, r.status, r.duration_ms, r.error_message, r.created_at FROM usage_records r JOIN users u ON u.id = r.user_id ORDER BY r.created_at DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取 AI 调用记录失败"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let email: String = row.get("email");
            let operation: String = row.get("operation");
            let item_count: i32 = row.get("item_count");
            let status_value: String = row.get("status");
            let duration_ms: i64 = row.get("duration_ms");
            let error_message: String = row.get("error_message");
            let created_at: DateTime<Utc> = row.get("created_at");
            let status = if status_value == "success" { "<span class=\"badge ok\">成功</span>" } else { "<span class=\"badge bad\">失败</span>" };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{} ms</td><td>{}</td></tr>",
                escape_html(&format_time(created_at)),
                escape_html(&email),
                escape_html(&operation),
                item_count,
                status,
                duration_ms,
                escape_html(&error_message)
            )
        })
        .collect())
}

fn dashboard_html(
    state: &AppState,
    stats: &DashboardStats,
    recent_errors: &[String],
    csrf: &str,
) -> String {
    let ai_key_status = if state.config.ai_api_key.trim().is_empty() {
        "<span class=\"badge bad\">未配置</span>"
    } else {
        "<span class=\"badge ok\">已配置</span>"
    };
    let errors = if recent_errors.is_empty() {
        "<tr><td colspan=\"4\" class=\"muted\">暂无失败记录</td></tr>".into()
    } else {
        recent_errors.join("")
    };
    format!(
        "<section class=\"grid\"><div class=\"metric\"><b>{}</b><span>用户总数</span></div><div class=\"metric\"><b>{}</b><span>封禁用户</span></div><div class=\"metric\"><b>{}</b><span>今日生成条数</span></div><div class=\"metric\"><b>{}</b><span>今日失败</span></div></section><section><h2>AI 配置</h2><table><tr><th>项目</th><th>状态</th></tr><tr><td>API Key</td><td>{}</td></tr><tr><td>Base URL</td><td>{}</td></tr><tr><td>模型</td><td>{}</td></tr><tr><td>超时</td><td>{} 秒</td></tr><tr><td>并发</td><td>{}</td></tr></table></section><section><h2>最近失败</h2><table><tr><th>时间</th><th>用户</th><th>接口</th><th>错误</th></tr>{}</table></section><form method=\"post\" action=\"/admin/logout\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"secondary\" type=\"submit\">退出登录</button></form>",
        stats.total_users,
        stats.disabled_users,
        stats.today_items,
        stats.today_failures,
        ai_key_status,
        escape_html(&state.config.ai_base_url),
        escape_html(&state.config.ai_model),
        state.config.ai_timeout.as_secs(),
        state.config.ai_max_concurrency,
        errors,
        escape_html(csrf)
    )
}

fn users_html(rows: &[String], csrf: &str) -> String {
    let body = if rows.is_empty() {
        "<tr><td colspan=\"7\" class=\"muted\">暂无用户</td></tr>".into()
    } else {
        rows.join("").replace("__CSRF__", &escape_html(csrf))
    };
    format!(
        "<section><h2>用户管理</h2><table><tr><th>邮箱</th><th>状态</th><th>注册时间</th><th>最后登录</th><th>AI 次数</th><th>生成条数</th><th>操作</th></tr>{}</table></section>",
        body
    )
}

fn ai_usage_html(rows: &[String]) -> String {
    let body = if rows.is_empty() {
        "<tr><td colspan=\"7\" class=\"muted\">暂无调用记录</td></tr>".into()
    } else {
        rows.join("")
    };
    format!(
        "<section><h2>AI 调用记录</h2><table><tr><th>时间</th><th>用户</th><th>接口</th><th>条数</th><th>状态</th><th>耗时</th><th>错误</th></tr>{}</table></section>",
        body
    )
}

fn login_html(error: &str, default_email: &str) -> String {
    let error_html = if error.is_empty() {
        String::new()
    } else {
        format!("<p class=\"error\">{}</p>", escape_html(error))
    };
    format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>后台登录</title>{}</head><body class=\"login\"><main class=\"login-card\"><h1>台球图文生成器后台</h1><form method=\"post\" action=\"/admin/login\"><label>管理员邮箱<input name=\"email\" type=\"email\" value=\"{}\" autocomplete=\"username\" required></label><label>密码<input name=\"password\" type=\"password\" autocomplete=\"current-password\" required></label>{}<button type=\"submit\">登录</button></form></main></body></html>",
        css(),
        escape_html(default_email),
        error_html
    )
}

fn layout(title: &str, admin_email: &str, active: &str, content: &str) -> String {
    format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>{}</head><body><header><div><strong>台球图文生成器后台</strong><span>{}</span></div><nav><a class=\"{}\" href=\"/admin\">首页</a><a class=\"{}\" href=\"/admin/users\">用户</a><a class=\"{}\" href=\"/admin/ai-usage\">AI 记录</a></nav></header><main>{}</main></body></html>",
        escape_html(title),
        css(),
        escape_html(admin_email),
        active_class(active, "dashboard"),
        active_class(active, "users"),
        active_class(active, "ai"),
        content
    )
}

fn admin_not_configured_html() -> String {
    "<section><h2>后台未配置</h2><p class=\"error\">服务器还没有配置 ADMIN_PASSWORD_HASH，暂时不能登录后台。</p></section>".into()
}

fn active_class(active: &str, current: &str) -> &'static str {
    if active == current {
        "active"
    } else {
        ""
    }
}

fn admin_configured(state: &AppState) -> bool {
    !state.config.admin_email.trim().is_empty()
        && !state.config.admin_password_hash.trim().is_empty()
}

fn internal_page(state: &AppState, error: sqlx::Error, message: &'static str) -> Response {
    tracing::error!(error = %error, message, "admin database operation failed");
    html_response(layout(
        "后台错误",
        &state.config.admin_email,
        "",
        &format!("<p class=\"error\">{}</p>", escape_html(message)),
    ))
}

fn html_response(body: String) -> Response {
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "text/html; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

fn set_cookie(response: &mut Response, value: &str) {
    if let Ok(header) = HeaderValue::from_str(value) {
        response.headers_mut().append(SET_COOKIE, header);
    }
}

fn session_cookie(token: &str, max_age_seconds: i64) -> String {
    format!(
        "{}={}; Path=/admin; Max-Age={}; HttpOnly; SameSite=Lax",
        SESSION_COOKIE, token, max_age_seconds
    )
}

fn expired_session_cookie() -> String {
    format!(
        "{}=; Path=/admin; Max-Age=0; HttpOnly; SameSite=Lax",
        SESSION_COOKIE
    )
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let header = headers.get(COOKIE)?.to_str().ok()?;
    header.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

fn new_token() -> String {
    format!("{}.{}", Uuid::new_v4(), Uuid::new_v4())
}

fn format_time(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn css() -> &'static str {
    "<style>:root{color-scheme:light;background:#f6f7f9;color:#1f2933;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}body{margin:0}header{display:flex;justify-content:space-between;align-items:center;padding:18px 28px;background:#111827;color:white}header span{display:block;margin-top:4px;color:#aab2c0;font-size:13px}nav{display:flex;gap:8px}nav a{color:#cbd5e1;text-decoration:none;padding:8px 12px;border-radius:6px}nav a.active,nav a:hover{background:#263244;color:white}main{max-width:1180px;margin:28px auto;padding:0 20px}section{background:white;border:1px solid #e5e7eb;border-radius:8px;margin-bottom:18px;padding:18px;box-shadow:0 1px 2px rgba(15,23,42,.04)}h1,h2{margin:0 0 16px}table{width:100%;border-collapse:collapse;font-size:14px}th,td{padding:11px 10px;border-bottom:1px solid #edf0f3;text-align:left;vertical-align:middle}th{background:#f8fafc;color:#475569;font-weight:700}.grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;background:transparent;border:0;box-shadow:none;padding:0}.metric{background:white;border:1px solid #e5e7eb;border-radius:8px;padding:18px}.metric b{display:block;font-size:30px;margin-bottom:6px}.metric span,.muted{color:#64748b}.badge{display:inline-block;border-radius:999px;padding:3px 8px;font-size:12px;font-weight:700}.badge.ok{background:#dcfce7;color:#166534}.badge.bad{background:#fee2e2;color:#991b1b}button{appearance:none;border:0;border-radius:6px;background:#111827;color:white;padding:8px 12px;font-weight:700;cursor:pointer}.secondary{background:#475569}.danger{background:#b91c1c}.error{color:#b91c1c;background:#fef2f2;border:1px solid #fecaca;border-radius:6px;padding:10px 12px}.login{display:grid;min-height:100vh;place-items:center;background:#eef2f7}.login-card{width:min(420px,calc(100vw - 32px));background:white;border:1px solid #e5e7eb;border-radius:8px;padding:26px;box-shadow:0 10px 30px rgba(15,23,42,.08)}label{display:block;margin:14px 0;color:#475569;font-weight:700}input{box-sizing:border-box;width:100%;margin-top:6px;border:1px solid #cbd5e1;border-radius:6px;padding:10px 12px;font:inherit}@media(max-width:760px){header{display:block}nav{margin-top:14px;flex-wrap:wrap}.grid{grid-template-columns:1fr}table{display:block;overflow-x:auto;white-space:nowrap}}</style>"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn escapes_html_values() {
        assert_eq!(
            escape_html("<a href=\"x\">Tom & Jerry</a>"),
            "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&lt;/a&gt;"
        );
    }

    #[test]
    fn parses_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("one=1; billiards_admin_session=abc; other=2"),
        );
        assert_eq!(
            cookie_value(&headers, SESSION_COOKIE).as_deref(),
            Some("abc")
        );
    }
}
