//! Discovery report listing and retrieval.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::AppState;

/// List recent discovery reports.
pub async fn list_reports(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = crate::repository::discovery_queries::list_discovery_reports(&state.db).await?;

    let reports: Vec<Value> = rows
        .iter()
        .map(|(id, agent_id, hostname, scanned_at)| {
            json!({
                "id": id,
                "agent_id": agent_id,
                "hostname": hostname,
                "scanned_at": scanned_at,
            })
        })
        .collect();

    Ok(Json(json!({ "reports": reports })))
}

/// Get full discovery report with raw process/listener/connection data.
pub async fn get_report(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(report_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let row =
        crate::repository::discovery_queries::get_discovery_report(&state.db, report_id).await?;

    match row {
        Some((id, agent_id, hostname, report, scanned_at)) => Ok(Json(json!({
            "id": *id,
            "agent_id": *agent_id,
            "hostname": hostname,
            "report": report,
            "scanned_at": scanned_at,
        }))),
        None => Err(ApiError::NotFound),
    }
}
