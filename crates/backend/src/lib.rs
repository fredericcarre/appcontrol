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
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub ws_hub: websocket::Hub,
    pub config: config::AppConfig,
    pub rate_limiter: middleware::rate_limit::RateLimitState,
    pub heartbeat_batcher: core::heartbeat_batcher::HeartbeatBatcher,
    pub redis: Option<redis::aio::ConnectionManager>,
}

/// Build a CORS layer based on configuration.
fn build_cors_layer(config: &config::AppConfig) -> CorsLayer {
    use axum::http::{HeaderValue, Method};

    if config.cors_origins.is_empty() {
        if config.app_env == "production" {
            // Production with no origins configured: deny cross-origin
            CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers(tower_http::cors::Any)
        } else {
            // Development: permissive for local dev
            CorsLayer::permissive()
        }
    } else {
        let origins: Vec<HeaderValue> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::PATCH])
            .allow_headers(tower_http::cors::Any)
            .allow_credentials(true)
    }
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = build_cors_layer(&state.config);

    Router::new()
        .route("/health", get(api::health::health))
        .route("/ready", get(api::health::ready))
        .route("/metrics", get(api::health::metrics))
        .route("/openapi.json", get(api::health::openapi_spec))
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
        .layer(cors)
        .with_state(state)
}
