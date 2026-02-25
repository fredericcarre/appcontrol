use axum::{extract::State, http::StatusCode, response::Json};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::ApiError;
use crate::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "git_hash": env!("GIT_HASH"),
    }))
}

pub async fn ready(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    // Check database connectivity
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    Ok(Json(json!({ "status": "ready" })))
}

/// Global handle set once at startup.
static PROMETHEUS_HANDLE: std::sync::OnceLock<
    &'static metrics_exporter_prometheus::PrometheusHandle,
> = std::sync::OnceLock::new();

pub fn set_prometheus_handle(handle: &'static metrics_exporter_prometheus::PrometheusHandle) {
    let _ = PROMETHEUS_HANDLE.set(handle);
}

/// Prometheus metrics endpoint — returns text/plain in OpenMetrics format.
pub async fn metrics() -> (
    StatusCode,
    [(axum::http::header::HeaderName, &'static str); 1],
    String,
) {
    match PROMETHEUS_HANDLE.get() {
        Some(handle) => {
            let body = handle.render();
            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )],
                body,
            )
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            "metrics recorder not initialized".to_string(),
        ),
    }
}

/// OpenAPI 3.0 specification endpoint.
pub async fn openapi_spec() -> (
    StatusCode,
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        include_str!("../../openapi.json"),
    )
}
