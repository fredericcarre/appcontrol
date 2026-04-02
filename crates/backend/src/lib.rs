pub mod api;
pub mod auth;
pub mod config;
pub mod core;
pub mod db;
pub mod error;
pub mod middleware;
pub mod repository;
pub mod terminal;
pub mod websocket;

// MCP module is internal-only
mod mcp;

use axum::{
    http::{header, HeaderValue},
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub db: crate::db::DbPool,
    pub ws_hub: websocket::Hub,
    pub config: config::AppConfig,
    pub rate_limiter: middleware::rate_limit::RateLimitState,
    pub heartbeat_batcher: core::heartbeat_batcher::HeartbeatBatcher,
    pub operation_lock: core::operation_lock::OperationLock,
    pub terminal_sessions: terminal::TerminalSessionManager,
    pub log_subscriptions: websocket::LogSubscriptionManager,
    pub pending_log_requests: websocket::PendingLogRequests,
    // Repository instances — all database queries go through these
    pub app_repo: Box<dyn repository::apps::AppRepository>,
    pub component_repo: Box<dyn repository::components::ComponentRepository>,
    pub team_repo: Box<dyn repository::teams::TeamRepository>,
    pub permission_repo: Box<dyn repository::permissions::PermissionRepository>,
    pub site_repo: Box<dyn repository::sites::SiteRepository>,
    pub enrollment_repo: Box<dyn repository::enrollment::EnrollmentRepository>,
    pub agent_repo: Box<dyn repository::agents::AgentRepository>,
    pub gateway_repo: Box<dyn repository::gateways::GatewayRepository>,
}

/// Build a CORS layer based on configuration.
fn build_cors_layer(config: &config::AppConfig) -> CorsLayer {
    use axum::http::Method;

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
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
            ])
            .allow_headers(tower_http::cors::Any)
            .allow_credentials(true)
    }
}

/// Security headers middleware — CSP, HSTS, X-Frame-Options, X-Content-Type-Options.
fn security_headers_layer() -> tower_http::set_header::SetResponseHeaderLayer<HeaderValue> {
    tower_http::set_header::SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    )
}

/// Axum middleware that adds security headers to every response.
async fn security_headers_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        header::X_XSS_PROTECTION,
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; connect-src 'self' wss: ws:; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    if let Ok(val) = HeaderValue::from_str("camera=(), microphone=(), geolocation=()") {
        headers.insert(
            axum::http::HeaderName::from_static("permissions-policy"),
            val,
        );
    }

    response
}

/// Axum middleware that records HTTP metrics (requests total, duration histogram).
async fn metrics_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let start = std::time::Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    let response = next.run(request).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "status" => status
    )
    .increment(1);
    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => normalize_path(&path)
    )
    .record(duration);

    response
}

/// Normalize API paths to avoid high-cardinality labels in Prometheus.
/// /api/v1/apps/550e8400-... -> /api/v1/apps/:id
fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let normalized: Vec<&str> = parts
        .iter()
        .map(|p| {
            if p.len() == 36 && p.chars().filter(|c| *c == '-').count() == 4 {
                ":id"
            } else {
                p
            }
        })
        .collect();
    normalized.join("/")
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = build_cors_layer(&state.config);

    let router = Router::new()
        .route("/health", get(api::health::health))
        .route("/ready", get(api::health::ready))
        .route("/metrics", get(api::health::metrics))
        .route("/openapi.json", get(api::health::openapi_spec))
        // Auth routes (no auth middleware — these ARE the login endpoints)
        .nest("/api/v1", auth::oidc::oidc_routes())
        .nest("/api/v1", auth::saml::saml_routes())
        .nest("/api/v1", auth::auth_routes())
        // Break-glass activation (no auth — this IS the emergency access)
        .route(
            "/api/v1/break-glass/activate",
            post(api::break_glass::activate_break_glass),
        )
        // Agent/Gateway enrollment (no auth — token-based)
        .route("/api/v1/enroll", post(api::enrollment::enroll))
        // Public CA certificate (no auth — for init containers and trust establishment)
        .route("/api/v1/pki/ca-public", get(api::pki_export::get_ca_public))
        // Share link info (no auth — allows preview before login)
        .route(
            "/api/v1/share/:token",
            get(api::permissions::get_share_link_info),
        )
        // Protected API routes (includes auth middleware layer)
        .nest("/api/v1", api::api_routes(state.clone()))
        .route("/ws", get(websocket::ws_handler))
        .route("/ws/gateway", get(websocket::gateway_ws_handler));

    // Serve frontend static files if a frontend directory exists.
    // This enables standalone deployment without nginx.
    // Looks for: <exe_dir>/frontend/, <exe_dir>/../frontend/, ./frontend/
    let frontend_dir = find_frontend_dir();
    let router = if let Some(dir) = frontend_dir {
        tracing::info!(path = %dir.display(), "Serving frontend static files");
        router.fallback_service(
            tower_http::services::ServeDir::new(&dir)
                .fallback(tower_http::services::ServeFile::new(dir.join("index.html"))),
        )
    } else {
        tracing::debug!("No frontend directory found — static file serving disabled");
        router
    };

    router
        .layer(axum::middleware::from_fn(metrics_middleware))
        .layer(axum::middleware::from_fn(security_headers_middleware))
        .layer(security_headers_layer())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Find the frontend static files directory.
/// Checks multiple locations for standalone deployment compatibility.
fn find_frontend_dir() -> Option<std::path::PathBuf> {
    let candidates = [
        // Relative to executable (standalone deployment)
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("frontend"))),
        // Relative to exe parent (exe in bin/, frontend as sibling)
        std::env::current_exe().ok().and_then(|p| {
            p.parent()
                .and_then(|d| d.parent())
                .map(|d| d.join("frontend"))
        }),
        // Current working directory
        Some(std::path::PathBuf::from("frontend")),
        // Parent of CWD (CWD is bin/)
        Some(std::path::PathBuf::from("../frontend")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|candidate| candidate.join("index.html").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_replaces_uuids() {
        assert_eq!(
            normalize_path("/api/v1/apps/550e8400-e29b-41d4-a716-446655440000"),
            "/api/v1/apps/:id"
        );
    }

    #[test]
    fn test_normalize_path_preserves_non_uuid() {
        assert_eq!(normalize_path("/api/v1/apps"), "/api/v1/apps");
    }

    #[test]
    fn test_normalize_path_nested_uuids() {
        assert_eq!(
            normalize_path("/api/v1/apps/550e8400-e29b-41d4-a716-446655440000/components"),
            "/api/v1/apps/:id/components"
        );
    }
}
