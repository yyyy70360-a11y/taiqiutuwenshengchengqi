use crate::{
    ai_config::{AiConfigUpdate, AiRuntimeConfig},
    auth, AppState,
};
use axum::{
    extract::{Form, Path, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::Semaphore;
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

#[derive(Debug, Deserialize, Default)]
pub struct UsersQuery {
    q: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserCreateForm {
    csrf: String,
    email: String,
    password: String,
    disabled: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PasswordForm {
    csrf: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct AiConfigForm {
    csrf: String,
    base_url: String,
    model: String,
    api_key: String,
    clear_api_key: Option<String>,
    timeout_seconds: u64,
    max_concurrency: usize,
}

#[derive(Debug, Clone)]
struct UserListFilter {
    q: String,
    status: String,
}

#[derive(Debug)]
struct UserDetailData {
    id: String,
    email: String,
    disabled: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
    active_sessions: i64,
    total_sessions: i64,
    account_count: i64,
    copy_count: i64,
    ai_calls: i64,
    ai_items: i64,
    recent_usage_rows: Vec<String>,
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
    let ai_config = match crate::ai_config::load(&state.db, &state.config).await {
        Ok(value) => value,
        Err(error) => return internal_page(&state, error, "读取 AI 配置失败"),
    };
    html_response(layout(
        "管理首页",
        &state.config.admin_email,
        "dashboard",
        &dashboard_html(&ai_config, &stats, &recent_errors, &csrf),
    ))
}

pub async fn users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UsersQuery>,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let normalized = normalize_users_query(query);
    let rows = match user_rows(&state, &normalized).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "用户管理",
        &state.config.admin_email,
        "users",
        &users_html(&rows, &csrf, &normalized),
    ))
}

pub async fn new_user(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "创建用户",
        &state.config.admin_email,
        "users",
        &new_user_html(&csrf, ""),
    ))
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<UserCreateForm>,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if !verify_csrf(&session, &input.csrf) {
        return html_response(layout(
            "请求已过期",
            &state.config.admin_email,
            "users",
            "<p class=\"error\">页面已过期，请返回创建用户页重试。</p>",
        ));
    }
    let email = match auth::normalize_email(&input.email) {
        Ok(value) => value,
        Err(error) => {
            return html_response(layout(
                "创建用户",
                &state.config.admin_email,
                "users",
                &new_user_html(&input.csrf, &error.message),
            ))
        }
    };
    if let Err(error) = auth::validate_password(&input.password) {
        return html_response(layout(
            "创建用户",
            &state.config.admin_email,
            "users",
            &new_user_html(&input.csrf, &error.message),
        ));
    }
    let password_hash = match auth::hash_password(&input.password) {
        Ok(value) => value,
        Err(error) => {
            return html_response(layout(
                "创建用户",
                &state.config.admin_email,
                "users",
                &new_user_html(&input.csrf, &error.message),
            ))
        }
    };
    let user_id = Uuid::new_v4().to_string();
    let disabled = input.disabled.is_some();
    let result = sqlx::query(
        "INSERT INTO users (id, email, password_hash, disabled) VALUES ($1, $2, $3, $4)",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(password_hash)
    .bind(disabled)
    .execute(&state.db)
    .await;
    match result {
        Ok(_) => Redirect::to(&format!("/admin/users/{user_id}")).into_response(),
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            html_response(layout(
                "创建用户",
                &state.config.admin_email,
                "users",
                &new_user_html(&input.csrf, "该邮箱已注册"),
            ))
        }
        Err(error) => internal_page(&state, error, "创建用户失败"),
    }
}

pub async fn user_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let detail = match user_detail_data(&state, &user_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return html_response(layout(
                "用户不存在",
                &state.config.admin_email,
                "users",
                "<p class=\"error\">用户不存在。</p>",
            ))
        }
        Err(response) => return response,
    };
    html_response(layout(
        "用户详情",
        &state.config.admin_email,
        "users",
        &user_detail_html(&detail, &csrf, "", false),
    ))
}

pub async fn reset_user_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(input): Form<PasswordForm>,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let message = if !verify_csrf(&session, &input.csrf) {
        ("页面已过期，请返回用户详情页重试。".to_string(), false)
    } else if let Err(error) = auth::validate_password(&input.password) {
        (error.message, false)
    } else {
        match auth::hash_password(&input.password) {
            Ok(password_hash) => {
                let updated = sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
                    .bind(password_hash)
                    .bind(&user_id)
                    .execute(&state.db)
                    .await;
                match updated {
                    Ok(_) => {
                        let _ = revoke_sessions(&state, &user_id).await;
                        ("密码已重置，用户需要重新登录。".to_string(), true)
                    }
                    Err(error) => return internal_page(&state, error, "重置密码失败"),
                }
            }
            Err(error) => (error.message, false),
        }
    };
    render_user_detail_message(state, session, user_id, &message.0, message.1).await
}

pub async fn revoke_user_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Form(input): Form<CsrfForm>,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let message = if verify_csrf(&session, &input.csrf) {
        match revoke_sessions(&state, &user_id).await {
            Ok(_) => ("已强制该用户下线。".to_string(), true),
            Err(error) => return internal_page(&state, error, "强制下线失败"),
        }
    } else {
        ("页面已过期，请返回用户详情页重试。".to_string(), false)
    };
    render_user_detail_message(state, session, user_id, &message.0, message.1).await
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

pub async fn ai_config(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let config = match crate::ai_config::load(&state.db, &state.config).await {
        Ok(value) => value,
        Err(error) => return internal_page(&state, error, "读取 AI 配置失败"),
    };
    html_response(layout(
        "AI 配置",
        &state.config.admin_email,
        "ai-config",
        &ai_config_html(&config, &csrf, "", false),
    ))
}

pub async fn save_ai_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<AiConfigForm>,
) -> Response {
    let Some(session) = require_session(&state, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if !verify_csrf(&session, &input.csrf) {
        return html_response(layout(
            "请求已过期",
            &state.config.admin_email,
            "ai-config",
            "<p class=\"error\">页面已过期，请返回 AI 配置页重试。</p>",
        ));
    }
    let update = AiConfigUpdate {
        base_url: input.base_url,
        model: input.model,
        timeout_seconds: input.timeout_seconds,
        max_concurrency: input.max_concurrency,
        api_key: Some(input.api_key),
        clear_database_api_key: input.clear_api_key.is_some(),
    };
    let saved = match crate::ai_config::save(&state.db, &state.config, update).await {
        Ok(value) => value,
        Err(message) => {
            let current = crate::ai_config::load(&state.db, &state.config)
                .await
                .unwrap_or_else(|_| state_ai_config_fallback(&state));
            return html_response(layout(
                "AI 配置",
                &state.config.admin_email,
                "ai-config",
                &ai_config_html(&current, &input.csrf, &message, false),
            ));
        }
    };
    {
        let mut semaphore = state.ai_semaphore.write().await;
        *semaphore = Arc::new(Semaphore::new(saved.max_concurrency));
    }
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    html_response(layout(
        "AI 配置",
        &state.config.admin_email,
        "ai-config",
        &ai_config_html(
            &saved,
            &csrf,
            "AI 配置已保存，新的生成任务会立即使用。",
            true,
        ),
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
        .bind(&user_id)
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
    if disabled {
        if let Err(error) = revoke_sessions(&state, &user_id).await {
            return internal_page(&state, error, "撤销用户会话失败");
        }
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

fn normalize_users_query(query: UsersQuery) -> UserListFilter {
    let status = match query.status.as_deref() {
        Some("active") => "active",
        Some("disabled") => "disabled",
        _ => "all",
    }
    .to_string();
    UserListFilter {
        q: query.q.unwrap_or_default().trim().to_string(),
        status,
    }
}

async fn render_user_detail_message(
    state: AppState,
    session: AdminSession,
    user_id: String,
    message: &str,
    success: bool,
) -> Response {
    let csrf = match rotate_csrf(&state, &session).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let detail = match user_detail_data(&state, &user_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return html_response(layout(
                "用户不存在",
                &state.config.admin_email,
                "users",
                "<p class=\"error\">用户不存在。</p>",
            ))
        }
        Err(response) => return response,
    };
    html_response(layout(
        "用户详情",
        &state.config.admin_email,
        "users",
        &user_detail_html(&detail, &csrf, message, success),
    ))
}

async fn revoke_sessions(state: &AppState, user_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn user_detail_data(
    state: &AppState,
    user_id: &str,
) -> Result<Option<UserDetailData>, Response> {
    let Some(user) = sqlx::query(
        "SELECT id, email, disabled, created_at, last_login_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取用户详情失败"))?
    else {
        return Ok(None);
    };

    let stats = sqlx::query(
        "SELECT
            (SELECT COUNT(*)::BIGINT FROM sessions WHERE user_id = $1 AND revoked_at IS NULL AND refresh_expires_at > NOW()) AS active_sessions,
            (SELECT COUNT(*)::BIGINT FROM sessions WHERE user_id = $1) AS total_sessions,
            (SELECT COUNT(*)::BIGINT FROM accounts WHERE user_id = $1) AS account_count,
            (SELECT COUNT(*)::BIGINT FROM copy_library WHERE user_id = $1) AS copy_count,
            (SELECT COUNT(*)::BIGINT FROM usage_records WHERE user_id = $1) AS ai_calls,
            (SELECT COALESCE(SUM(item_count), 0)::BIGINT FROM usage_records WHERE user_id = $1) AS ai_items",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取用户统计失败"))?;

    let usage_rows = sqlx::query(
        "SELECT operation, item_count, status, duration_ms, error_message, created_at FROM usage_records WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| internal_page(state, error, "读取用户调用记录失败"))?;
    let recent_usage_rows = usage_rows
        .into_iter()
        .map(|row| {
            let operation: String = row.get("operation");
            let item_count: i32 = row.get("item_count");
            let status_value: String = row.get("status");
            let duration_ms: i64 = row.get("duration_ms");
            let error_message: String = row.get("error_message");
            let created_at: DateTime<Utc> = row.get("created_at");
            let status = if status_value == "success" {
                "<span class=\"badge ok\">成功</span>"
            } else {
                "<span class=\"badge bad\">失败</span>"
            };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{} ms</td><td>{}</td></tr>",
                escape_html(&format_time(created_at)),
                escape_html(&operation),
                item_count,
                status,
                duration_ms,
                escape_html(&error_message)
            )
        })
        .collect();

    Ok(Some(UserDetailData {
        id: user.get("id"),
        email: user.get("email"),
        disabled: user.get("disabled"),
        created_at: user.get("created_at"),
        last_login_at: user.get("last_login_at"),
        active_sessions: stats.get("active_sessions"),
        total_sessions: stats.get("total_sessions"),
        account_count: stats.get("account_count"),
        copy_count: stats.get("copy_count"),
        ai_calls: stats.get("ai_calls"),
        ai_items: stats.get("ai_items"),
        recent_usage_rows,
    }))
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

async fn user_rows(state: &AppState, filter: &UserListFilter) -> Result<Vec<String>, Response> {
    let rows = sqlx::query(
        "SELECT u.id, u.email, u.disabled, u.created_at, u.last_login_at, COALESCE(SUM(r.item_count), 0)::BIGINT AS items, COUNT(r.id)::BIGINT AS calls FROM users u LEFT JOIN usage_records r ON r.user_id = u.id WHERE ($1 = '' OR u.email ILIKE '%' || $1 || '%') AND ($2 = 'all' OR ($2 = 'active' AND u.disabled = FALSE) OR ($2 = 'disabled' AND u.disabled = TRUE)) GROUP BY u.id ORDER BY u.created_at DESC LIMIT 200",
    )
    .bind(&filter.q)
    .bind(&filter.status)
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
                "<tr><td><a href=\"/admin/users/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><form method=\"post\" action=\"/admin/users/{}/{}\"><input type=\"hidden\" name=\"csrf\" value=\"__CSRF__\"><button class=\"{}\" type=\"submit\">{}</button></form></td></tr>",
                escape_html(&id),
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
    ai_config: &AiRuntimeConfig,
    stats: &DashboardStats,
    recent_errors: &[String],
    csrf: &str,
) -> String {
    let ai_key_status = if ai_config.api_key.trim().is_empty() {
        "<span class=\"badge bad\">未配置</span>"
    } else if ai_config.api_key_from_database {
        "<span class=\"badge ok\">已配置（后台保存）</span>"
    } else {
        "<span class=\"badge ok\">已配置（环境变量）</span>"
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
        escape_html(&ai_config.base_url),
        escape_html(&ai_config.model),
        ai_config.timeout_seconds,
        ai_config.max_concurrency,
        errors,
        escape_html(csrf)
    )
}

fn ai_config_html(config: &AiRuntimeConfig, csrf: &str, message: &str, success: bool) -> String {
    let key_status = if config.api_key.trim().is_empty() {
        "<span class=\"badge bad\">未配置</span>"
    } else if config.api_key_from_database {
        "<span class=\"badge ok\">已由后台保存</span>"
    } else {
        "<span class=\"badge ok\">使用服务器环境变量</span>"
    };
    let message_html = if message.is_empty() {
        String::new()
    } else if success {
        format!("<p class=\"notice\">{}</p>", escape_html(message))
    } else {
        format!("<p class=\"error\">{}</p>", escape_html(message))
    };
    format!(
        "<section><h2>AI 配置</h2>{}<form method=\"post\" action=\"/admin/ai-config\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label>Base URL<input name=\"base_url\" value=\"{}\" required></label><label>模型<input name=\"model\" value=\"{}\" required></label><label>API Key<input name=\"api_key\" type=\"text\" value=\"{}\" autocomplete=\"off\"></label><p class=\"muted\">当前 Key：{}，已按管理员要求明文显示。</p><label class=\"check\"><input name=\"clear_api_key\" type=\"checkbox\" value=\"1\">清空后台保存的 Key，恢复使用服务器环境变量</label><label>超时秒数<input name=\"timeout_seconds\" type=\"number\" min=\"5\" max=\"300\" value=\"{}\" required></label><label>最大并发<input name=\"max_concurrency\" type=\"number\" min=\"1\" max=\"32\" value=\"{}\" required></label><button type=\"submit\">保存 AI 配置</button></form></section>",
        message_html,
        escape_html(csrf),
        escape_html(&config.base_url),
        escape_html(&config.model),
        escape_html(&config.api_key),
        key_status,
        config.timeout_seconds,
        config.max_concurrency
    )
}

fn users_html(rows: &[String], csrf: &str, filter: &UserListFilter) -> String {
    let body = if rows.is_empty() {
        "<tr><td colspan=\"7\" class=\"muted\">暂无用户</td></tr>".into()
    } else {
        rows.join("").replace("__CSRF__", &escape_html(csrf))
    };
    let all_selected = selected(&filter.status, "all");
    let active_selected = selected(&filter.status, "active");
    let disabled_selected = selected(&filter.status, "disabled");
    format!(
        "<section><div class=\"section-head\"><h2>用户管理</h2><a class=\"button-link\" href=\"/admin/users/new\">创建用户</a></div><form class=\"inline-form\" method=\"get\" action=\"/admin/users\"><label>邮箱搜索<input name=\"q\" value=\"{}\" placeholder=\"user@example.com\"></label><label>状态<select name=\"status\"><option value=\"all\" {}>全部</option><option value=\"active\" {}>正常</option><option value=\"disabled\" {}>已封禁</option></select></label><button type=\"submit\">筛选</button><a class=\"button-link secondary\" href=\"/admin/users\">重置</a></form><table><tr><th>邮箱</th><th>状态</th><th>注册时间</th><th>最后登录</th><th>AI 次数</th><th>生成条数</th><th>操作</th></tr>{}</table></section>",
        escape_html(&filter.q),
        all_selected,
        active_selected,
        disabled_selected,
        body
    )
}

fn new_user_html(csrf: &str, error: &str) -> String {
    let error_html = if error.is_empty() {
        String::new()
    } else {
        format!("<p class=\"error\">{}</p>", escape_html(error))
    };
    format!(
        "<section><h2>创建用户</h2>{}<form method=\"post\" action=\"/admin/users\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label>邮箱<input name=\"email\" type=\"email\" autocomplete=\"off\" required></label><label>初始密码<input name=\"password\" type=\"text\" autocomplete=\"off\" minlength=\"8\" required></label><label class=\"check\"><input name=\"disabled\" type=\"checkbox\" value=\"1\">创建后先封禁</label><button type=\"submit\">创建用户</button><a class=\"button-link secondary\" href=\"/admin/users\">返回</a></form></section>",
        error_html,
        escape_html(csrf)
    )
}

fn user_detail_html(detail: &UserDetailData, csrf: &str, message: &str, success: bool) -> String {
    let message_html = if message.is_empty() {
        String::new()
    } else if success {
        format!("<p class=\"notice\">{}</p>", escape_html(message))
    } else {
        format!("<p class=\"error\">{}</p>", escape_html(message))
    };
    let status = if detail.disabled {
        "<span class=\"badge bad\">已封禁</span>"
    } else {
        "<span class=\"badge ok\">正常</span>"
    };
    let action = if detail.disabled { "enable" } else { "disable" };
    let action_label = if detail.disabled {
        "解封用户"
    } else {
        "封禁用户"
    };
    let action_class = if detail.disabled {
        "secondary"
    } else {
        "danger"
    };
    let usage_rows = if detail.recent_usage_rows.is_empty() {
        "<tr><td colspan=\"6\" class=\"muted\">暂无 AI 调用记录</td></tr>".to_string()
    } else {
        detail.recent_usage_rows.join("")
    };
    format!(
        "<section><div class=\"section-head\"><h2>用户详情</h2><a class=\"button-link secondary\" href=\"/admin/users\">返回用户列表</a></div>{}<table><tr><th>项目</th><th>值</th></tr><tr><td>邮箱</td><td>{}</td></tr><tr><td>状态</td><td>{}</td></tr><tr><td>注册时间</td><td>{}</td></tr><tr><td>最后登录</td><td>{}</td></tr><tr><td>活动会话</td><td>{} / {}</td></tr><tr><td>账号配置数</td><td>{}</td></tr><tr><td>文案库条数</td><td>{}</td></tr><tr><td>AI 调用 / 生成条数</td><td>{} / {}</td></tr></table><div class=\"actions\"><form method=\"post\" action=\"/admin/users/{}/{}\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"{}\" type=\"submit\">{}</button></form><form method=\"post\" action=\"/admin/users/{}/sessions/revoke\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"secondary\" type=\"submit\">强制下线</button></form></div></section><section><h2>重置密码</h2><form method=\"post\" action=\"/admin/users/{}/password\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><label>新密码<input name=\"password\" type=\"text\" autocomplete=\"off\" minlength=\"8\" required></label><button type=\"submit\">重置密码并下线</button></form></section><section><h2>最近 AI 调用</h2><table><tr><th>时间</th><th>接口</th><th>条数</th><th>状态</th><th>耗时</th><th>错误</th></tr>{}</table></section>",
        message_html,
        escape_html(&detail.email),
        status,
        escape_html(&format_time(detail.created_at)),
        escape_html(&detail.last_login_at.map(format_time).unwrap_or_else(|| "未登录".into())),
        detail.active_sessions,
        detail.total_sessions,
        detail.account_count,
        detail.copy_count,
        detail.ai_calls,
        detail.ai_items,
        escape_html(&detail.id),
        action,
        escape_html(csrf),
        action_class,
        action_label,
        escape_html(&detail.id),
        escape_html(csrf),
        escape_html(&detail.id),
        escape_html(csrf),
        usage_rows
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
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>{}</head><body><header><div><strong>台球图文生成器后台</strong><span>{}</span></div><nav><a class=\"{}\" href=\"/admin\">首页</a><a class=\"{}\" href=\"/admin/users\">用户</a><a class=\"{}\" href=\"/admin/ai-config\">AI 配置</a><a class=\"{}\" href=\"/admin/ai-usage\">AI 记录</a></nav></header><main>{}</main></body></html>",
        escape_html(title),
        css(),
        escape_html(admin_email),
        active_class(active, "dashboard"),
        active_class(active, "users"),
        active_class(active, "ai-config"),
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

fn selected(active: &str, current: &str) -> &'static str {
    if active == current {
        "selected"
    } else {
        ""
    }
}

fn admin_configured(state: &AppState) -> bool {
    !state.config.admin_email.trim().is_empty()
        && !state.config.admin_password_hash.trim().is_empty()
}

fn state_ai_config_fallback(state: &AppState) -> AiRuntimeConfig {
    AiRuntimeConfig {
        base_url: state.config.ai_base_url.clone(),
        model: state.config.ai_model.clone(),
        api_key: state.config.ai_api_key.clone(),
        timeout_seconds: state.config.ai_timeout.as_secs(),
        max_concurrency: state.config.ai_max_concurrency,
        api_key_from_database: false,
    }
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
    "<style>:root{color-scheme:light;background:#f6f7f9;color:#1f2933;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}body{margin:0}header{display:flex;justify-content:space-between;align-items:center;padding:18px 28px;background:#111827;color:white}header span{display:block;margin-top:4px;color:#aab2c0;font-size:13px}nav{display:flex;gap:8px}nav a{color:#cbd5e1;text-decoration:none;padding:8px 12px;border-radius:6px}nav a.active,nav a:hover{background:#263244;color:white}main{max-width:1180px;margin:28px auto;padding:0 20px}section{background:white;border:1px solid #e5e7eb;border-radius:8px;margin-bottom:18px;padding:18px;box-shadow:0 1px 2px rgba(15,23,42,.04)}h1,h2{margin:0 0 16px}table{width:100%;border-collapse:collapse;font-size:14px}th,td{padding:11px 10px;border-bottom:1px solid #edf0f3;text-align:left;vertical-align:middle}th{background:#f8fafc;color:#475569;font-weight:700}.grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;background:transparent;border:0;box-shadow:none;padding:0}.metric{background:white;border:1px solid #e5e7eb;border-radius:8px;padding:18px}.metric b{display:block;font-size:30px;margin-bottom:6px}.metric span,.muted{color:#64748b}.badge{display:inline-block;border-radius:999px;padding:3px 8px;font-size:12px;font-weight:700}.badge.ok{background:#dcfce7;color:#166534}.badge.bad{background:#fee2e2;color:#991b1b}button{appearance:none;border:0;border-radius:6px;background:#111827;color:white;padding:8px 12px;font-weight:700;cursor:pointer}.secondary{background:#475569}.danger{background:#b91c1c}.error{color:#b91c1c;background:#fef2f2;border:1px solid #fecaca;border-radius:6px;padding:10px 12px}.notice{color:#166534;background:#f0fdf4;border:1px solid #bbf7d0;border-radius:6px;padding:10px 12px}.login{display:grid;min-height:100vh;place-items:center;background:#eef2f7}.login-card{width:min(420px,calc(100vw - 32px));background:white;border:1px solid #e5e7eb;border-radius:8px;padding:26px;box-shadow:0 10px 30px rgba(15,23,42,.08)}label{display:block;margin:14px 0;color:#475569;font-weight:700}.check{display:flex;gap:8px;align-items:center;font-weight:600}.check input{width:auto;margin:0}input,select{box-sizing:border-box;width:100%;margin-top:6px;border:1px solid #cbd5e1;border-radius:6px;padding:10px 12px;font:inherit}.section-head{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:14px}.inline-form{display:grid;grid-template-columns:2fr 1fr auto auto;gap:12px;align-items:end;margin-bottom:16px}.actions{display:flex;gap:10px;flex-wrap:wrap;margin-top:16px}.button-link{display:inline-block;border-radius:6px;background:#111827;color:white;text-decoration:none;padding:8px 12px;font-weight:700}.button-link.secondary{background:#475569}input,select{box-sizing:border-box;width:100%;margin-top:6px;border:1px solid #cbd5e1;border-radius:6px;padding:10px 12px;font:inherit}@media(max-width:760px){header{display:block}nav{margin-top:14px;flex-wrap:wrap}.grid,.inline-form{grid-template-columns:1fr}.section-head{display:block}table{display:block;overflow-x:auto;white-space:nowrap}}</style>"
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
