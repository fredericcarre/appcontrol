//! Schedule execution endpoints: toggle, run-now, list-executions.

use axum::{
    extract::{Path, State},
    Extension, Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::operation_scheduler::calculate_next_run;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit;
use crate::repository::schedule_queries as sched_repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

use super::types::*;

pub async fn toggle_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or(ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let new_enabled = !row.is_enabled;
    let next_run_at = if new_enabled {
        calculate_next_run(&row.cron_expression, &row.timezone)
    } else {
        None
    };

    let _ = audit::log_action(
        &state.db,
        user.user_id,
        if new_enabled {
            "enable_schedule"
        } else {
            "disable_schedule"
        },
        "schedule",
        schedule_id,
        serde_json::json!({"schedule_name": row.name}),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    sched_repo::toggle_schedule_enabled(&state.db, schedule_id, new_enabled, next_run_at)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Return updated schedule
    super::crud::get_schedule(State(state), Extension(user), Path(schedule_id)).await
}

pub async fn run_schedule_now(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or(ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    sched_repo::set_schedule_run_now(&state.db, schedule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let _ = audit::log_action(
        &state.db,
        user.user_id,
        "run_schedule_now",
        "schedule",
        schedule_id,
        serde_json::json!({"schedule_name": row.name, "operation": row.operation}),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        serde_json::json!({"message": "Schedule queued for immediate execution", "schedule_id": schedule_id}),
    ))
}

pub async fn list_schedule_executions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<Vec<ScheduleExecutionResponse>>, ApiError> {
    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or(ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let executions = sched_repo::list_executions(&state.db, schedule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let responses = executions
        .into_iter()
        .map(|e| ScheduleExecutionResponse {
            id: e.id,
            schedule_id: e.schedule_id,
            action_log_id: e.action_log_id,
            executed_at: e.executed_at,
            status: e.status,
            message: e.message,
            duration_ms: e.duration_ms,
        })
        .collect();

    Ok(Json(responses))
}
