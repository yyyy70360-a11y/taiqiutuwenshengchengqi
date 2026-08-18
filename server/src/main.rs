mod ai;
mod api;
mod auth;
mod config;
mod db;
mod errors;
mod models;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use config::Config;
use reqwest::Client;
use serde::Serialize;
use sqlx::PgPool;
use std::{env, sync::Arc};
use tokio::{net::TcpListener, sync::Semaphore};
use tower_http::trace::TraceLayer;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub ai_client: Client,
    pub ai_semaphore: Arc<Semaphore>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
    environment: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG").unwrap_or_else(|_| "billiards_api=info,tower_http=info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let config = Arc::new(Config::from_env()?);
    let db = db::connect(&config.database_url).await?;
    let ai_client = Client::builder().timeout(config.ai_timeout).build()?;
    let address = config.bind;
    let ai_semaphore = Arc::new(Semaphore::new(config.ai_max_concurrency));
    let state = AppState { config, db, ai_client, ai_semaphore };

    let app = Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/api/v1/version", get(version))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/me/settings", get(api::get_settings).put(api::put_settings))
        .route("/api/v1/me/accounts", get(api::get_accounts).put(api::put_accounts))
        .route("/api/v1/me/copy-library", get(api::get_copy_library).post(api::save_copy_library))
        .route("/api/v1/ai/generate-copy", post(ai::generate_copy))
        .route("/api/v1/ai/generate-batch-copy", post(ai::generate_batch_copy))
        .fallback(not_found)
        .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(address).await?;
    info!(%address, "billiards API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "billiards-api",
        version: env!("CARGO_PKG_VERSION"),
        environment: state.config.environment.clone(),
    })
}

async fn version() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "billiards-api",
        "version": env!("CARGO_PKG_VERSION"),
        "apiVersion": "v1"
    }))
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "not_found", "message": "接口不存在"})),
    )
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
