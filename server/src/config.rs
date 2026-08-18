use std::{env, net::SocketAddr, time::Duration};

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub environment: String,
    pub database_url: String,
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,
    pub ai_timeout: Duration,
    pub ai_max_concurrency: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind = env::var("BILLIARDS_BIND")
            .unwrap_or_else(|_| "127.0.0.1:38123".into())
            .parse()?;
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
        Ok(Self {
            bind,
            environment: env::var("BILLIARDS_ENV").unwrap_or_else(|_| "development".into()),
            database_url,
            ai_base_url: env::var("AI_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com/v1".into()),
            ai_model: env::var("AI_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
            ai_api_key: env::var("AI_API_KEY").unwrap_or_default(),
            ai_timeout: Duration::from_secs(
                env::var("AI_TIMEOUT_SECONDS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(45),
            ),
            ai_max_concurrency: env::var("AI_MAX_CONCURRENCY")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4),
        })
    }
}
