use crate::{
    auth,
    errors::{ApiError, ApiResult},
    models::{Account, CopyItem, Settings},
};
use axum::{extract::State, http::HeaderMap, Json};
use sqlx::Row;
use uuid::Uuid;

pub async fn get_settings(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
) -> ApiResult<Settings> {
    let user_id = auth::authenticate(&state.db, &headers).await?;
    let row = sqlx::query("SELECT api_url, api_model, output_dir FROM user_settings WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal_db("读取设置失败"))?;
    let settings = row
        .map(|row| Settings {
            api_url: row.get("api_url"),
            api_model: row.get("api_model"),
            output_dir: row.get("output_dir"),
        })
        .unwrap_or_default();
    Ok(Json(settings))
}

pub async fn put_settings(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(input): Json<Settings>,
) -> ApiResult<Settings> {
    let user_id = auth::authenticate(&state.db, &headers).await?;
    validate_setting("API 地址", &input.api_url, 2048)?;
    validate_setting("模型名", &input.api_model, 256)?;
    validate_setting("输出路径", &input.output_dir, 2048)?;
    sqlx::query("INSERT INTO user_settings (user_id, api_url, api_model, output_dir) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id) DO UPDATE SET api_url = EXCLUDED.api_url, api_model = EXCLUDED.api_model, output_dir = EXCLUDED.output_dir, updated_at = NOW()")
        .bind(user_id)
        .bind(input.api_url.trim())
        .bind(input.api_model.trim())
        .bind(input.output_dir.trim())
        .execute(&state.db)
        .await
        .map_err(internal_db("保存设置失败"))?;
    Ok(Json(input))
}

pub async fn get_accounts(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<Account>> {
    let user_id = auth::authenticate(&state.db, &headers).await?;
    let rows = sqlx::query("SELECT id, name, region, level, persona, tone, status FROM accounts WHERE user_id = $1 ORDER BY updated_at DESC")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(internal_db("读取账号失败"))?;
    let accounts = rows
        .into_iter()
        .map(|row| Account {
            id: Some(row.get("id")),
            name: row.get("name"),
            region: row.get("region"),
            level: row.get("level"),
            persona: row.get("persona"),
            tone: row.get("tone"),
            status: row.get("status"),
        })
        .collect();
    Ok(Json(accounts))
}

pub async fn put_accounts(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(input): Json<Vec<Account>>,
) -> ApiResult<Vec<Account>> {
    let user_id = auth::authenticate(&state.db, &headers).await?;
    if input.len() > 50 {
        return Err(ApiError::bad_request("账号数量不能超过 50 个"));
    }
    for account in &input {
        if account.name.trim().is_empty() || account.name.chars().count() > 100 {
            return Err(ApiError::bad_request("账号名称不能为空且不能超过 100 个字符"));
        }
    }
    let mut tx = state.db.begin().await.map_err(internal_db("保存账号失败"))?;
    sqlx::query("DELETE FROM accounts WHERE user_id = $1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_db("保存账号失败"))?;
    let mut saved = Vec::with_capacity(input.len());
    for account in input {
        let id = account.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        sqlx::query("INSERT INTO accounts (id, user_id, name, region, level, persona, tone, status) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(&id)
            .bind(&user_id)
            .bind(account.name.trim())
            .bind(account.region.trim())
            .bind(account.level.trim())
            .bind(account.persona.trim())
            .bind(account.tone.trim())
            .bind(account.status.trim())
            .execute(&mut *tx)
            .await
            .map_err(internal_db("保存账号失败"))?;
        saved.push(Account { id: Some(id), ..account });
    }
    tx.commit().await.map_err(internal_db("保存账号失败"))?;
    Ok(Json(saved))
}

pub async fn get_copy_library(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<CopyItem>> {
    let user_id = auth::authenticate(&state.db, &headers).await?;
    let rows = sqlx::query("SELECT id, title, body, tags FROM copy_library WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 500")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(internal_db("读取文案库失败"))?;
    Ok(Json(rows.into_iter().map(|row| CopyItem {
        id: Some(row.get("id")),
        title: row.get("title"),
        body: row.get("body"),
        tags: row.get("tags"),
    }).collect()))
}

pub async fn save_copy_library(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(mut input): Json<CopyItem>,
) -> ApiResult<CopyItem> {
    let user_id = auth::authenticate(&state.db, &headers).await?;
    if input.title.trim().is_empty() || input.body.trim().is_empty() {
        return Err(ApiError::bad_request("标题和正文不能为空"));
    }
    if input.title.chars().count() > 300 || input.body.chars().count() > 10000 || input.tags.chars().count() > 1000 {
        return Err(ApiError::bad_request("文案长度超出限制"));
    }
    let id = input.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    sqlx::query("INSERT INTO copy_library (id, user_id, title, body, tags) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, body = EXCLUDED.body, tags = EXCLUDED.tags, updated_at = NOW() WHERE copy_library.user_id = $2")
        .bind(&id)
        .bind(&user_id)
        .bind(input.title.trim())
        .bind(input.body.trim())
        .bind(input.tags.trim())
        .execute(&state.db)
        .await
        .map_err(internal_db("保存文案失败"))?;
    input.id = Some(id);
    Ok(Json(input))
}

fn validate_setting(label: &str, value: &str, max: usize) -> Result<(), ApiError> {
    if value.chars().count() > max {
        return Err(ApiError::bad_request(format!("{label}过长")));
    }
    Ok(())
}

fn internal_db(message: &'static str) -> impl FnOnce(sqlx::Error) -> ApiError {
    move |error| {
        tracing::error!(error = %error, message, "database operation failed");
        ApiError::internal(message)
    }
}
