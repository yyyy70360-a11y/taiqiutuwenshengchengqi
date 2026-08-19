use crate::{auth, AppState};
use axum::{
    extract::{Form, Path, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Redirect, Response},
    Json,
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

#[derive(Debug, Deserialize)]
pub struct ApplicationReviewForm {
    csrf: String,
    #[serde(default)]
    note: String,
}

#[derive(Default)]
struct ApplicationRows {
    pending: Vec<String>,
    approved: Vec<String>,
    rejected: Vec<String>,
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

pub async fn registration_applications(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let rows = match registration_application_rows(&state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "注册申请",
        &state.config.admin_email,
        "applications",
        &registration_applications_html(&rows, &csrf),
    ))
}

pub async fn registration_application_count(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if require_session(&state, &headers).await.is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::BIGINT FROM registration_applications WHERE status = 'pending'",
    )
    .fetch_one(&state.db)
    .await
    {
        Ok(count) => Json(serde_json::json!({ "pending": count })).into_response(),
        Err(error) => {
            tracing::error!(error = %error, "registration application count failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn approve_registration_application(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(application_id): Path<String>,
    Form(input): Form<ApplicationReviewForm>,
) -> Response {
    review_registration_application(state, headers, application_id, input, true).await
}

pub async fn reject_registration_application(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(application_id): Path<String>,
    Form(input): Form<ApplicationReviewForm>,
) -> Response {
    review_registration_application(state, headers, application_id, input, false).await
}

async fn review_registration_application(
    state: AppState,
    headers: HeaderMap,
    application_id: String,
    input: ApplicationReviewForm,
    approve: bool,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if !verify_csrf(&session, &input.csrf) {
        return application_error_page(&state, "页面已过期，请返回注册申请页重试。");
    }
    let mut transaction = match state.db.begin().await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(error = %error, "registration review transaction failed");
            return application_error_page(&state, "审核操作启动失败。");
        }
    };
    let application = match sqlx::query_as::<_, (String, Option<String>, String)>(
        "SELECT email, password_hash, status FROM registration_applications WHERE id = $1 FOR UPDATE",
    )
    .bind(&application_id)
    .fetch_optional(&mut *transaction)
    .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return application_error_page(&state, "注册申请不存在。"),
        Err(error) => {
            tracing::error!(error = %error, "registration application lock failed");
            return application_error_page(&state, "读取注册申请失败。");
        }
    };
    if application.2 != "pending" {
        return Redirect::to("/admin/registration-applications").into_response();
    }

    let result = if approve {
        let Some(password_hash) = application.1 else {
            return application_error_page(&state, "申请凭据已清除，无法批准。");
        };
        let user_exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
                .bind(&application.0)
                .fetch_one(&mut *transaction)
                .await;
        match user_exists {
            Ok(true) => Err("该邮箱已经是正式账号。".to_string()),
            Ok(false) => {
                let insert =
                    sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)")
                        .bind(Uuid::new_v4().to_string())
                        .bind(&application.0)
                        .bind(password_hash)
                        .execute(&mut *transaction)
                        .await;
                match insert {
                    Ok(_) => sqlx::query(
                        "UPDATE registration_applications SET status = 'approved', password_hash = NULL, reviewed_at = NOW(), reviewed_by = $1, review_note = $2 WHERE id = $3 AND status = 'pending'",
                    )
                    .bind(&state.config.admin_email)
                    .bind(input.note.trim())
                    .bind(&application_id)
                    .execute(&mut *transaction)
                    .await
                    .map(|_| ()),
                    Err(error) => Err(error),
                }
                .map_err(|error| {
                    tracing::error!(error = %error, "registration approval failed");
                    "批准注册申请失败。".to_string()
                })
            }
            Err(error) => {
                tracing::error!(error = %error, "registration approval user lookup failed");
                Err("检查正式账号失败。".to_string())
            }
        }
    } else {
        sqlx::query(
            "UPDATE registration_applications SET status = 'rejected', password_hash = NULL, reviewed_at = NOW(), reviewed_by = $1, review_note = $2 WHERE id = $3 AND status = 'pending'",
        )
        .bind(&state.config.admin_email)
        .bind(input.note.trim())
        .bind(&application_id)
        .execute(&mut *transaction)
        .await
        .map(|_| ())
        .map_err(|error| {
            tracing::error!(error = %error, "registration rejection failed");
            "拒绝注册申请失败。".to_string()
        })
    };
    if let Err(message) = result {
        return application_error_page(&state, &message);
    }
    if let Err(error) = transaction.commit().await {
        tracing::error!(error = %error, "registration review commit failed");
        return application_error_page(&state, "提交审核结果失败。");
    }
    Redirect::to("/admin/registration-applications").into_response()
}

fn application_error_page(state: &AppState, message: &str) -> Response {
    html_response(layout(
        "注册申请",
        &state.config.admin_email,
        "applications",
        &format!("<p class=\"error\">{}</p>", escape_html(message)),
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
    let mut transaction = match state.db.begin().await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(error = %error, "admin user transaction failed");
            return html_response(layout(
                "用户管理",
                &state.config.admin_email,
                "users",
                "<p class=\"error\">更新用户状态失败。</p>",
            ));
        }
    };
    if let Err(error) = sqlx::query("UPDATE users SET disabled = $1 WHERE id = $2")
        .bind(disabled)
        .bind(&user_id)
        .execute(&mut *transaction)
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
    if disabled {
        if let Err(error) = sqlx::query(
            "UPDATE sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(&user_id)
        .execute(&mut *transaction)
        .await
        {
            tracing::error!(error = %error, "admin session revoke failed");
            return html_response(layout(
                "用户管理",
                &state.config.admin_email,
                "users",
                "<p class=\"error\">撤销用户登录状态失败。</p>",
            ));
        }
    }
    if let Err(error) = transaction.commit().await {
        tracing::error!(error = %error, "admin user transaction commit failed");
        return html_response(layout(
            "用户管理",
            &state.config.admin_email,
            "users",
            "<p class=\"error\">提交用户状态失败。</p>",
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

async fn registration_application_rows(state: &AppState) -> Result<ApplicationRows, Response> {
    let rows = sqlx::query(
        "SELECT id, email, status, requested_at, reviewed_at, reviewed_by, review_note FROM registration_applications ORDER BY requested_at DESC LIMIT 500",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取注册申请失败"))?;
    let mut output = ApplicationRows::default();
    for row in rows {
        let id: String = row.get("id");
        let email: String = row.get("email");
        let status: String = row.get("status");
        let requested_at: DateTime<Utc> = row.get("requested_at");
        let reviewed_at: Option<DateTime<Utc>> = row.get("reviewed_at");
        let reviewed_by: Option<String> = row.get("reviewed_by");
        let review_note: String = row.get("review_note");
        if status == "pending" {
            output.pending.push(format!(
                "<tr><td>{}</td><td>{}</td><td><span class=\"badge pending\">待审核</span></td><td class=\"review-actions\"><form method=\"post\" action=\"/admin/registration-applications/{}/approve\"><input type=\"hidden\" name=\"csrf\" value=\"__CSRF__\"><button type=\"submit\">批准</button></form><form class=\"reject-form\" method=\"post\" action=\"/admin/registration-applications/{}/reject\"><input type=\"hidden\" name=\"csrf\" value=\"__CSRF__\"><input name=\"note\" maxlength=\"240\" placeholder=\"拒绝原因（选填）\"><button class=\"danger\" type=\"submit\">拒绝</button></form></td></tr>",
                escape_html(&email),
                escape_html(&format_time(requested_at)),
                escape_html(&id),
                escape_html(&id),
            ));
            continue;
        }
        let status_badge = if status == "approved" {
            "<span class=\"badge ok\">已批准</span>"
        } else {
            "<span class=\"badge bad\">已拒绝</span>"
        };
        let history_row = format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&email),
            status_badge,
            escape_html(&format_time(requested_at)),
            escape_html(&reviewed_at.map(format_time).unwrap_or_else(|| "-".into()),),
            escape_html(reviewed_by.as_deref().unwrap_or("-")),
            escape_html(if review_note.is_empty() {
                "-"
            } else {
                &review_note
            }),
        );
        if status == "approved" {
            output.approved.push(history_row);
        } else {
            output.rejected.push(history_row);
        }
    }
    Ok(output)
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

fn registration_applications_html(rows: &ApplicationRows, csrf: &str) -> String {
    let pending = if rows.pending.is_empty() {
        "<tr><td colspan=\"4\" class=\"muted\">暂无待审核申请</td></tr>".into()
    } else {
        rows.pending
            .join("")
            .replace("__CSRF__", &escape_html(csrf))
    };
    let history = |items: &[String], empty: &str| {
        if items.is_empty() {
            format!("<tr><td colspan=\"6\" class=\"muted\">{}</td></tr>", empty)
        } else {
            items.join("")
        }
    };
    format!(
        "<div class=\"subnav\"><a href=\"#pending\">待审核 <b>{}</b></a><a href=\"#approved\">已批准</a><a href=\"#rejected\">已拒绝</a></div><section id=\"pending\"><h2>待审核申请</h2><table><tr><th>邮箱</th><th>申请时间</th><th>状态</th><th>操作</th></tr>{}</table></section><section id=\"approved\"><h2>已批准</h2><table><tr><th>邮箱</th><th>状态</th><th>申请时间</th><th>审核时间</th><th>审核人</th><th>备注</th></tr>{}</table></section><section id=\"rejected\"><h2>已拒绝</h2><table><tr><th>邮箱</th><th>状态</th><th>申请时间</th><th>审核时间</th><th>审核人</th><th>备注</th></tr>{}</table></section>",
        rows.pending.len(),
        pending,
        history(&rows.approved, "暂无已批准申请"),
        history(&rows.rejected, "暂无已拒绝申请"),
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
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>{}</head><body><header><div><strong>台球图文生成器后台</strong><span>{}</span></div><nav><a class=\"{}\" href=\"/admin\">首页</a><a class=\"{}\" href=\"/admin/registration-applications\">申请 <b id=\"pendingCount\" class=\"nav-count\" hidden></b></a><a class=\"{}\" href=\"/admin/users\">用户</a><a class=\"{}\" href=\"/admin/ai-usage\">AI 记录</a></nav></header><main>{}</main><script>fetch('/admin/registration-applications/count',{{credentials:'same-origin'}}).then(r=>r.ok?r.json():null).then(v=>{{if(v&&v.pending>0){{const e=document.getElementById('pendingCount');e.textContent=v.pending;e.hidden=false}}}}).catch(()=>{{}})</script></body></html>",
        escape_html(title),
        css(),
        escape_html(admin_email),
        active_class(active, "dashboard"),
        active_class(active, "applications"),
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
    "<style>:root{color-scheme:light;background:#f6f7f9;color:#1f2933;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}body{margin:0}header{display:flex;justify-content:space-between;align-items:center;padding:18px 28px;background:#111827;color:white}header span{display:block;margin-top:4px;color:#aab2c0;font-size:13px}nav{display:flex;gap:8px}nav a{color:#cbd5e1;text-decoration:none;padding:8px 12px;border-radius:6px}nav a.active,nav a:hover{background:#263244;color:white}.nav-count{display:inline-grid;place-items:center;min-width:18px;height:18px;margin-left:5px;border-radius:9px;background:#ef4444;color:white;font-size:11px}main{max-width:1180px;margin:28px auto;padding:0 20px}section{background:white;border:1px solid #e5e7eb;border-radius:8px;margin-bottom:18px;padding:18px;box-shadow:0 1px 2px rgba(15,23,42,.04)}h1,h2{margin:0 0 16px}table{width:100%;border-collapse:collapse;font-size:14px}th,td{padding:11px 10px;border-bottom:1px solid #edf0f3;text-align:left;vertical-align:middle}th{background:#f8fafc;color:#475569;font-weight:700}.grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;background:transparent;border:0;box-shadow:none;padding:0}.metric{background:white;border:1px solid #e5e7eb;border-radius:8px;padding:18px}.metric b{display:block;font-size:30px;margin-bottom:6px}.metric span,.muted{color:#64748b}.badge{display:inline-block;border-radius:999px;padding:3px 8px;font-size:12px;font-weight:700}.badge.ok{background:#dcfce7;color:#166534}.badge.bad{background:#fee2e2;color:#991b1b}.badge.pending{background:#fef3c7;color:#92400e}.subnav{display:flex;gap:8px;margin-bottom:18px}.subnav a{padding:8px 12px;border:1px solid #dbe2ea;border-radius:6px;background:white;color:#334155;text-decoration:none}.review-actions{min-width:430px}.review-actions form{display:inline-flex;align-items:center;gap:8px;margin-right:8px}.review-actions input{width:210px;margin:0;padding:8px 10px}button{appearance:none;border:0;border-radius:6px;background:#111827;color:white;padding:8px 12px;font-weight:700;cursor:pointer}.secondary{background:#475569}.danger{background:#b91c1c}.error{color:#b91c1c;background:#fef2f2;border:1px solid #fecaca;border-radius:6px;padding:10px 12px}.login{display:grid;min-height:100vh;place-items:center;background:#eef2f7}.login-card{width:min(420px,calc(100vw - 32px));background:white;border:1px solid #e5e7eb;border-radius:8px;padding:26px;box-shadow:0 10px 30px rgba(15,23,42,.08)}label{display:block;margin:14px 0;color:#475569;font-weight:700}input{box-sizing:border-box;width:100%;margin-top:6px;border:1px solid #cbd5e1;border-radius:6px;padding:10px 12px;font:inherit}@media(max-width:760px){header{display:block}nav{margin-top:14px;flex-wrap:wrap}.grid{grid-template-columns:1fr}table{display:block;overflow-x:auto;white-space:nowrap}.review-actions{min-width:360px}}</style>"
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
