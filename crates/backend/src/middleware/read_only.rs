//! Read-only failsafe middleware for disaster recovery.
//!
//! When `READ_ONLY=true` is set in the environment, the backend boots
//! normally but every state-mutating HTTP method (POST, PUT, PATCH, DELETE)
//! is rejected with HTTP 503 (Service Unavailable) and a `Retry-After`
//! header.
//!
//! Endpoints that remain available even in read-only mode:
//! - Any `GET`/`HEAD`/`OPTIONS` request
//! - Authentication endpoints (the request to obtain a login session can be
//!   POST; we look them up by path prefix below)
//! - The break-glass activation endpoint (emergency access)
//! - Health, readiness, metrics, OpenAPI spec
//!
//! The middleware is applied as the outermost layer on the protected API
//! routes so that no mutation hits a handler when read-only mode is active.

use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::AppState;

/// Paths that remain writable even when READ_ONLY=true.
///
/// Auth/session endpoints must keep accepting POST so an operator can still
/// log in to inspect state during a DR incident. The break-glass activation
/// endpoint is the entry point for emergency access.
const READ_ONLY_BYPASS_PREFIXES: &[&str] = &[
    "/api/v1/auth/",
    "/api/v1/oidc/",
    "/api/v1/saml/",
    "/api/v1/break-glass/",
];

/// True if the given path is exempted from read-only enforcement.
fn is_bypass(path: &str) -> bool {
    READ_ONLY_BYPASS_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// True if the method is considered a mutation under read-only mode.
fn is_mutating(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}

/// Middleware: when `READ_ONLY=true`, reject mutating requests with 503.
pub async fn read_only_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if state.config.read_only && is_mutating(request.method()) && !is_bypass(request.uri().path()) {
        let path = request.uri().path().to_string();
        let method = request.method().clone();
        tracing::warn!(
            method = %method,
            path = %path,
            "Rejected mutating request — backend is in READ_ONLY mode (DR failsafe)"
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Retry-After", "60"), ("X-AppControl-Read-Only", "true")],
            "AppControl backend is in read-only mode (READ_ONLY=true). \
             Mutating requests are temporarily refused. Clear READ_ONLY \
             after the underlying incident is resolved.",
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutating_methods_are_detected() {
        assert!(is_mutating(&Method::POST));
        assert!(is_mutating(&Method::PUT));
        assert!(is_mutating(&Method::PATCH));
        assert!(is_mutating(&Method::DELETE));
        assert!(!is_mutating(&Method::GET));
        assert!(!is_mutating(&Method::HEAD));
        assert!(!is_mutating(&Method::OPTIONS));
    }

    #[test]
    fn auth_paths_are_bypassed() {
        assert!(is_bypass("/api/v1/auth/login"));
        assert!(is_bypass("/api/v1/auth/logout"));
        assert!(is_bypass("/api/v1/oidc/callback"));
        assert!(is_bypass("/api/v1/saml/acs"));
        assert!(is_bypass("/api/v1/break-glass/activate"));
        assert!(!is_bypass("/api/v1/apps"));
        assert!(!is_bypass("/api/v1/components/abc"));
    }
}
