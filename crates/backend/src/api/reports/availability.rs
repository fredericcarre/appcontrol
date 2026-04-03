//! Availability report and health summary endpoints.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::repository::report_queries as repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

pub async fn availability(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let from = params.from.unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let stats = repo::fetch_availability_stats(&state.db, app_id, from, to).await?;

    let data: Vec<Value> = stats.iter().map(|(cid, date, running, total)| {
        let pct = if *total > 0 { (*running as f64 / *total as f64) * 100.0 } else { 0.0 };
        json!({"component_id": cid, "date": date, "running_seconds": running, "total_seconds": total, "availability_pct": pct})
    }).collect();

    Ok(Json(json!({ "report": "availability", "data": data })))
}

/// Real-time health summary for an application.
pub async fn health_summary(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let states = repo::get_state_breakdown(&state.db, app_id).await?;
    let state_breakdown: Value = states.iter().map(|(s, c)| json!({"state": s, "count": c})).collect();

    let error_components = repo::get_error_components(&state.db, app_id).await?;
    let errors: Vec<Value> = error_components.iter()
        .map(|(id, name, state, at)| json!({"component_id": id, "name": name, "state": state, "since": at}))
        .collect();

    let agents = repo::get_app_agents(&state.db, app_id).await?;
    let agent_status: Vec<Value> = agents.iter()
        .map(|(id, hostname, active, heartbeat)| {
            let stale = heartbeat.is_none_or(|hb| (chrono::Utc::now() - hb).num_seconds() > 120);
            json!({"agent_id": id, "hostname": hostname, "active": active, "last_heartbeat": heartbeat, "stale": stale})
        }).collect();

    let recent_incidents = repo::get_recent_incidents(&state.db, app_id, 10).await?;
    let incidents_list: Vec<Value> = recent_incidents.iter()
        .map(|(cid, name, from, _to, at)| json!({"component_id": cid, "component_name": name, "from_state": from, "at": at}))
        .collect();

    let total_components = repo::count_components(&state.db, app_id).await;

    Ok(Json(json!({
        "total_components": total_components,
        "state_breakdown": state_breakdown,
        "error_components": errors,
        "agents": agent_status,
        "recent_incidents": incidents_list,
    })))
}
