use crate::{
    errors::{ApiError, ApiResult},
    models::{
        AuthResponse, LoginRequest, RefreshRequest, RegisterApplicationRequest,
        RegisterApplicationResponse, RegisterRequest,
    },
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    Json,
};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn register(
    State(state): State<crate::AppState>,
    Json(input): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterApplicationResponse>), ApiError> {
    submit_registration_application(&state, &input.email, &input.password, &input.password).await
}

pub async fn register_application(
    State(state): State<crate::AppState>,
    Json(input): Json<RegisterApplicationRequest>,
) -> Result<(StatusCode, Json<RegisterApplicationResponse>), ApiError> {
    submit_registration_application(
        &state,
        &input.email,
        &input.password,
        &input.confirm_password,
    )
    .await
}

async fn submit_registration_application(
    state: &crate::AppState,
    email: &str,
    password: &str,
    confirm_password: &str,
) -> Result<(StatusCode, Json<RegisterApplicationResponse>), ApiError> {
    let email = normalize_email(email)?;
    if password != confirm_password {
        return Err(ApiError::bad_request("两次输入的密码不一致"));
    }
    validate_password(password)?;

    let user_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
            .bind(&email)
            .fetch_one(&state.db)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "registration user lookup failed");
                ApiError::internal("提交注册申请失败")
            })?;
    if user_exists {
        return Err(ApiError::coded(
            StatusCode::CONFLICT,
            "account_exists",
            "该邮箱已注册，请直接登录",
        ));
    }

    let existing = sqlx::query_as::<_, (String, chrono::DateTime<Utc>)>(
        "SELECT status, requested_at FROM registration_applications WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "registration application lookup failed");
        ApiError::internal("提交注册申请失败")
    })?;

    if let Some((status, requested_at)) = existing {
        if status == "pending" {
            return Err(ApiError::coded(
                StatusCode::CONFLICT,
                "application_pending",
                "注册申请正在审核，请勿重复提交",
            ));
        }
        // A rejected application is explicitly allowed to submit a new password
        // immediately; keep the cooldown for other non-pending retries.
        if status != "rejected" && Utc::now() - requested_at < Duration::minutes(1) {
            return Err(ApiError::too_many_requests("提交过于频繁，请稍后再试"));
        }
    }

    let password_hash = hash_password(password)?;
    sqlx::query(
        "INSERT INTO registration_applications (id, email, password_hash) VALUES ($1, $2, $3) \
         ON CONFLICT (email) DO UPDATE SET password_hash = EXCLUDED.password_hash, status = 'pending', requested_at = NOW(), reviewed_at = NULL, reviewed_by = NULL, review_note = ''",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&email)
    .bind(password_hash)
    .execute(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "registration application insert failed");
        ApiError::internal("提交注册申请失败，请稍后重试")
    })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RegisterApplicationResponse {
            status: "pending",
            message: "注册申请已提交，请等待管理员审核。批准后可直接使用该邮箱和密码登录。",
        }),
    ))
}

pub async fn login(
    State(state): State<crate::AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<AuthResponse> {
    let email = normalize_email(&input.email)?;
    let row = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT id, password_hash, disabled FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "login query failed");
        ApiError::internal("登录失败，请稍后重试")
    })?;
    let Some(row) = row else {
        return application_login_error(&state.db, &email, &input.password).await;
    };
    if !verify_password(&input.password, &row.1) {
        return Err(invalid_credentials());
    }
    if row.2 {
        return Err(ApiError::coded(
            StatusCode::FORBIDDEN,
            "account_disabled",
            "账号已停用，请联系管理员",
        ));
    }
    sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
        .bind(&row.0)
        .execute(&state.db)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "last login update failed");
            ApiError::internal("登录失败，请稍后重试")
        })?;
    issue_tokens(&state.db, &row.0).await.map(Json)
}

async fn application_login_error(
    pool: &PgPool,
    email: &str,
    password: &str,
) -> ApiResult<AuthResponse> {
    let application = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, password_hash FROM registration_applications WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "registration status lookup failed");
        ApiError::internal("登录失败，请稍后重试")
    })?;
    let Some((status, hash)) = application else {
        return Err(invalid_credentials());
    };
    if let Some(hash) = hash {
        if !verify_password(password, &hash) {
            return Err(invalid_credentials());
        }
    } else if status != "rejected" {
        return Err(invalid_credentials());
    }
    match status.as_str() {
        "pending" => Err(ApiError::coded(
            StatusCode::FORBIDDEN,
            "application_pending",
            "注册申请正在审核，批准后即可登录",
        )),
        "rejected" => Err(ApiError::coded(
            StatusCode::FORBIDDEN,
            "application_rejected",
            "注册申请未通过，可重新提交申请",
        )),
        _ => Err(invalid_credentials()),
    }
}

fn invalid_credentials() -> ApiError {
    ApiError::coded(
        StatusCode::UNAUTHORIZED,
        "invalid_credentials",
        "邮箱或密码错误",
    )
}

pub async fn refresh(
    State(state): State<crate::AppState>,
    Json(input): Json<RefreshRequest>,
) -> ApiResult<AuthResponse> {
    if input.refresh_token.trim().is_empty() {
        return Err(ApiError::unauthorized("刷新令牌为空"));
    }
    let token_hash = hash_token(&input.refresh_token);
    let row = sqlx::query_as::<_, (String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>)>(
        "SELECT user_id, refresh_expires_at, revoked_at FROM sessions WHERE refresh_token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "refresh query failed");
        ApiError::internal("刷新登录状态失败")
    })?
    .ok_or_else(|| ApiError::unauthorized("刷新令牌无效"))?;
    if row.2.is_some() || row.1 <= Utc::now() {
        return Err(ApiError::unauthorized("刷新令牌已过期"));
    }
    sqlx::query("UPDATE sessions SET revoked_at = NOW() WHERE refresh_token_hash = $1")
        .bind(hash_token(&input.refresh_token))
        .execute(&state.db)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "revoke old session failed");
            ApiError::internal("刷新登录状态失败")
        })?;
    issue_tokens(&state.db, &row.0).await.map(Json)
}

pub async fn logout(
    State(state): State<crate::AppState>,
    Json(input): Json<RefreshRequest>,
) -> ApiResult<serde_json::Value> {
    if !input.refresh_token.trim().is_empty() {
        sqlx::query("UPDATE sessions SET revoked_at = NOW() WHERE refresh_token_hash = $1")
            .bind(hash_token(&input.refresh_token))
            .execute(&state.db)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "logout failed");
                ApiError::internal("退出登录失败")
            })?;
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn authenticate(pool: &PgPool, headers: &HeaderMap) -> Result<String, ApiError> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::unauthorized("缺少登录令牌"))?;
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT s.user_id FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.access_token_hash = $1 AND s.revoked_at IS NULL AND s.access_expires_at > NOW() AND u.disabled = FALSE",
    )
    .bind(hash_token(header))
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "authenticate query failed");
        ApiError::internal("验证登录状态失败")
    })?
    .ok_or_else(|| ApiError::unauthorized("登录令牌无效或已过期"))?;
    Ok(row.0)
}

async fn issue_tokens(pool: &PgPool, user_id: &str) -> Result<AuthResponse, ApiError> {
    let access_token = Uuid::new_v4().to_string().replace('-', "");
    let refresh_token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
    let access_expires_at = Utc::now() + Duration::minutes(15);
    let refresh_expires_at = Utc::now() + Duration::days(30);
    sqlx::query("INSERT INTO sessions (id, user_id, access_token_hash, refresh_token_hash, access_expires_at, refresh_expires_at) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(hash_token(&access_token))
        .bind(hash_token(&refresh_token))
        .bind(access_expires_at)
        .bind(refresh_expires_at)
        .execute(pool)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "issue tokens failed");
            ApiError::internal("创建登录状态失败")
        })?;
    Ok(AuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: 900,
    })
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let value = email.trim().to_lowercase();
    let valid = value
        .rsplit_once('@')
        .map(|(local, domain)| {
            !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !value.chars().any(char::is_whitespace)
        })
        .unwrap_or(false);
    if value.is_empty() || value.len() > 254 || !valid {
        return Err(ApiError::bad_request("请输入有效邮箱"));
    }
    Ok(value)
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < 8 || password.len() > 256 {
        return Err(ApiError::bad_request("密码长度需为 8 至 256 个字符"));
    }
    if !password.chars().any(char::is_alphabetic) || !password.chars().any(char::is_numeric) {
        return Err(ApiError::bad_request("密码需同时包含字母和数字"));
    }
    Ok(())
}

pub(crate) fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::internal("密码处理失败"))
}

pub(crate) fn verify_password(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded)
        .ok()
        .map(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
        .unwrap_or(false)
}

pub(crate) fn hash_token(token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(token.as_bytes());
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_email_and_validates_password() {
        assert_eq!(
            normalize_email(" User@Example.COM ").unwrap(),
            "user@example.com"
        );
        assert!(normalize_email("invalid").is_err());
        assert!(validate_password("password8").is_ok());
        assert!(validate_password("12345678").is_err());
        assert!(validate_password("password").is_err());
        assert!(validate_password("short").is_err());
    }

    #[test]
    fn password_hashes_are_verifiable() {
        let encoded = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &encoded));
        assert!(!verify_password("wrong", &encoded));
    }

    #[test]
    fn token_hash_is_stable_and_not_plaintext() {
        assert_eq!(hash_token("token"), hash_token("token"));
        assert_ne!(hash_token("token"), "token");
    }
}
