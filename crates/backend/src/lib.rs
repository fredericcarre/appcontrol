pub mod api;
pub mod auth;
pub mod config;
pub mod core;
pub mod db;
pub mod middleware;
pub mod websocket;

// MCP module is internal-only
mod mcp;

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub ws_hub: websocket::Hub,
    pub config: config::AppConfig,
    pub rate_limiter: middleware::rate_limit::RateLimitState,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(api::health::health))
        .route("/ready", get(api::health::ready))
        // Auth routes (no auth middleware — these ARE the login endpoints)
        .nest("/api/v1", auth::oidc::oidc_routes())
        .nest("/api/v1", auth::saml::saml_routes())
        // Break-glass activation (no auth — this IS the emergency access)
        .route(
            "/api/v1/break-glass/activate",
            post(api::break_glass::activate_break_glass),
        )
        // Protected API routes (includes auth middleware layer)
        .nest("/api/v1", api::api_routes(state.clone()))
        .route("/ws", get(websocket::ws_handler))
        .route("/ws/gateway", get(websocket::gateway_ws_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
