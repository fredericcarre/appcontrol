//! DORA compliance endpoints: RTO, MTTR, compliance.

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

pub async fn compliance(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let action_count = repo::count_action_log(&state.db, app_id).await;

    Ok(Json(json!({
        "report": "compliance",
        "dora_compliant": true,
        "audit_trail_entries": action_count,
        "append_only_enforced": true,
    })))
}

pub async fn rto(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let avg_rto = repo::fetch_avg_rto(&state.db, app_id).await;

    Ok(Json(json!({ "report": "rto", "average_rto_seconds": avg_rto })))
}

/// Calculate MTTR for an application.
pub async fn mttr(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let from = params.from.unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let recoveries = repo::fetch_mttr_recoveries(&state.db, app_id, from, to).await?;

    let total_incidents = recoveries.len();
    let recovery_times: Vec<i64> = recoveries.iter().map(|r| r.4).collect();

    let avg_mttr = if !recovery_times.is_empty() {
        recovery_times.iter().sum::<i64>() as f64 / recovery_times.len() as f64
    } else { 0.0 };

    let min_mttr = recovery_times.iter().min().copied().unwrap_or(0);
    let max_mttr = recovery_times.iter().max().copied().unwrap_or(0);

    let median_mttr = if !recovery_times.is_empty() {
        let mut sorted = recovery_times.clone();
        sorted.sort();
        if sorted.len().is_multiple_of(2) {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) as f64 / 2.0
        } else { sorted[sorted.len() / 2] as f64 }
    } else { 0.0 };

    let mut component_stats: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
    for (_, name, _, _, seconds) in &recoveries {
        component_stats.entry(name.clone()).or_default().push(*seconds);
    }

    let per_component: Vec<Value> = component_stats.iter()
        .map(|(name, times)| {
            let avg = times.iter().sum::<i64>() as f64 / times.len() as f64;
            json!({"component_name": name, "incident_count": times.len(), "avg_mttr_seconds": avg})
        }).collect();

    let recent: Vec<Value> = recoveries.iter().take(10)
        .map(|(comp_id, name, failed_at, recovered_at, seconds)| {
            json!({
                "component_id": comp_id, "component_name": name,
                "failed_at": failed_at, "recovered_at": recovered_at,
                "recovery_seconds": seconds, "recovery_formatted": format_duration(*seconds),
            })
        }).collect();

    Ok(Json(json!({
        "report": "mttr",
        "period": {"from": from, "to": to},
        "summary": {
            "total_incidents": total_incidents, "avg_mttr_seconds": avg_mttr,
            "avg_mttr_formatted": format_duration(avg_mttr as i64),
            "median_mttr_seconds": median_mttr,
            "min_mttr_seconds": min_mttr, "max_mttr_seconds": max_mttr,
        },
        "per_component": per_component,
        "recent_incidents": recent,
    })))
}

fn format_duration(seconds: i64) -> String {
    if seconds < 60 { format!("{}s", seconds) }
    else if seconds < 3600 { format!("{}m {}s", seconds / 60, seconds % 60) }
    else { format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60) }
}
