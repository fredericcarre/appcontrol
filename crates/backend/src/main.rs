mod api;
mod auth;
mod config;
mod core;
mod db;
mod middleware;
mod websocket;

use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    pub db: sqlx::PgPool,
    pub ws_hub: websocket::Hub,
    pub config: config::AppConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appcontrol_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::AppConfig::from_env();
    let pool = db::create_pool(&config.database_url).await?;
    let ws_hub = websocket::Hub::new();

    let state = Arc::new(AppState {
        db: pool,
        ws_hub,
        config,
    });

    let app = create_router(state.clone());

    let addr = format!("0.0.0.0:{}", state.config.port);
    tracing::info!("Starting AppControl backend on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(api::health::health))
        .route("/ready", get(api::health::ready))
        .nest("/api/v1", api::api_routes(state.clone()))
        .route("/ws", get(websocket::ws_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
