use crate::config::Config;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

const KEY_BASE_URL: &str = "ai_base_url";
const KEY_MODEL: &str = "ai_model";
const KEY_API_KEY: &str = "ai_api_key";
const KEY_TIMEOUT_SECONDS: &str = "ai_timeout_seconds";
const KEY_MAX_CONCURRENCY: &str = "ai_max_concurrency";

#[derive(Debug, Clone)]
pub struct AiRuntimeConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub timeout_seconds: u64,
    pub max_concurrency: usize,
    pub api_key_from_database: bool,
}

#[derive(Debug)]
pub struct AiConfigUpdate {
    pub base_url: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub max_concurrency: usize,
    pub api_key: Option<String>,
    pub clear_database_api_key: bool,
}

pub async fn load(pool: &PgPool, defaults: &Config) -> Result<AiRuntimeConfig, sqlx::Error> {
    let values = load_values(pool).await?;
    let base_url = values
        .get(KEY_BASE_URL)
        .map(String::as_str)
        .unwrap_or(&defaults.ai_base_url)
        .trim()
        .trim_end_matches('/')
        .to_string();
    let model = values
        .get(KEY_MODEL)
        .map(String::as_str)
        .unwrap_or(&defaults.ai_model)
        .trim()
        .to_string();
    let api_key_from_database = values
        .get(KEY_API_KEY)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let api_key = values
        .get(KEY_API_KEY)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| defaults.ai_api_key.clone());
    let timeout_seconds = values
        .get(KEY_TIMEOUT_SECONDS)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(defaults.ai_timeout.as_secs())
        .clamp(5, 300);
    let max_concurrency = values
        .get(KEY_MAX_CONCURRENCY)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(defaults.ai_max_concurrency)
        .clamp(1, 32);

    Ok(AiRuntimeConfig {
        base_url,
        model,
        api_key,
        timeout_seconds,
        max_concurrency,
        api_key_from_database,
    })
}

pub async fn save(
    pool: &PgPool,
    defaults: &Config,
    input: AiConfigUpdate,
) -> Result<AiRuntimeConfig, String> {
    let base_url = normalize_base_url(&input.base_url)?;
    let model = normalize_model(&input.model)?;
    let timeout_seconds = input.timeout_seconds.clamp(5, 300);
    let max_concurrency = input.max_concurrency.clamp(1, 32);

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("保存 AI 配置失败: {error}"))?;
    upsert_setting(&mut tx, KEY_BASE_URL, &base_url).await?;
    upsert_setting(&mut tx, KEY_MODEL, &model).await?;
    upsert_setting(&mut tx, KEY_TIMEOUT_SECONDS, &timeout_seconds.to_string()).await?;
    upsert_setting(&mut tx, KEY_MAX_CONCURRENCY, &max_concurrency.to_string()).await?;
    if input.clear_database_api_key {
        sqlx::query("DELETE FROM server_settings WHERE key = $1")
            .bind(KEY_API_KEY)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("清理 AI Key 失败: {error}"))?;
    } else if let Some(api_key) = input.api_key {
        let api_key = api_key.trim();
        if !api_key.is_empty() {
            upsert_setting(&mut tx, KEY_API_KEY, api_key).await?;
        }
    }
    tx.commit()
        .await
        .map_err(|error| format!("保存 AI 配置失败: {error}"))?;

    load(pool, defaults)
        .await
        .map_err(|error| format!("读取 AI 配置失败: {error}"))
}

async fn load_values(pool: &PgPool) -> Result<HashMap<String, String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT key, value FROM server_settings WHERE key IN ('ai_base_url', 'ai_model', 'ai_api_key', 'ai_timeout_seconds', 'ai_max_concurrency')",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("key"), row.get("value")))
        .collect())
}

async fn upsert_setting(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO server_settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
    )
    .bind(key)
    .bind(value)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("保存 AI 配置失败: {error}"))?;
    Ok(())
}

fn normalize_base_url(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/').to_string();
    if value.is_empty() || value.len() > 2048 {
        return Err("AI Base URL 不能为空且不能过长".into());
    }
    if !(value.starts_with("https://") || value.starts_with("http://")) {
        return Err("AI Base URL 必须以 http:// 或 https:// 开头".into());
    }
    if value.contains('?') || value.contains('#') {
        return Err("AI Base URL 不能包含查询参数或片段".into());
    }
    Ok(value)
}

fn normalize_model(value: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() || value.len() > 256 {
        return Err("模型名不能为空且不能过长".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ai_base_url() {
        assert_eq!(
            normalize_base_url(" https://api.deepseek.com/v1/ ").unwrap(),
            "https://api.deepseek.com/v1"
        );
        assert!(normalize_base_url("ftp://example.com").is_err());
        assert!(normalize_base_url("https://example.com?token=x").is_err());
    }

    #[test]
    fn validates_model_name() {
        assert_eq!(normalize_model(" deepseek-chat ").unwrap(), "deepseek-chat");
        assert!(normalize_model("").is_err());
    }
}
