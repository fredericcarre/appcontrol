//! CRUD operations for schedules.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::operation_scheduler::{calculate_next_run, is_valid_cron, preset_to_cron};
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit;
use crate::repository::schedule_queries as sched_repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

use super::types::*;

// ============================================================================
// Application-level
// ============================================================================

pub async fn list_app_schedules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(query): Query<ListSchedulesQuery>,
) -> Result<Json<Vec<ScheduleResponse>>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let include_disabled = query.include_disabled.unwrap_or(false);
    let rows = if include_disabled {
        sched_repo::list_app_schedules_all(&state.db, app_id).await.map_err(|e| ApiError::Internal(e.to_string()))?
    } else {
        sched_repo::list_app_schedules(&state.db, app_id).await.map_err(|e| ApiError::Internal(e.to_string()))?
    };

    let app_name = sched_repo::get_app_name_by_id(&state.db, app_id).await;
    let target_name = app_name.unwrap_or_else(|| app_id.to_string());

    let responses = rows.into_iter()
        .map(|row| row_to_response(row, "application".to_string(), app_id.into(), target_name.clone()))
        .collect();
    Ok(Json(responses))
}

pub async fn create_app_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleResponse>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate { return Err(ApiError::Forbidden); }

    if !["start", "stop", "restart"].contains(&req.operation.as_str()) {
        return Err(ApiError::Validation(format!("Invalid operation '{}'", req.operation)));
    }

    let cron_expression = resolve_cron(&req)?;
    if !is_valid_cron(&cron_expression) {
        return Err(ApiError::Validation(format!("Invalid cron expression '{}'", cron_expression)));
    }

    let org_id = sched_repo::get_org_id_for_app_sched(&state.db, app_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let next_run_at = calculate_next_run(&cron_expression, &req.timezone);
    let _ = audit::log_action(&state.db, user.user_id, "create_schedule", "application", app_id,
        serde_json::json!({"schedule_name": req.name, "operation": req.operation, "cron_expression": cron_expression}),
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    let schedule_id = Uuid::new_v4();
    sched_repo::create_operation_schedule(&state.db, schedule_id, org_id, app_id, None,
        &req.name, req.description.as_deref(), &req.operation, &cron_expression, &req.timezone,
        next_run_at, *user.user_id,
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    let row = sched_repo::get_schedule_by_id(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("Schedule not found after creation".to_string()))?;

    let app_name = sched_repo::get_app_name_by_id(&state.db, app_id).await;
    let target_name = app_name.unwrap_or_else(|| app_id.to_string());
    Ok((StatusCode::CREATED, Json(row_to_response(row, "application".to_string(), app_id.into(), target_name))))
}

// ============================================================================
// Component-level
// ============================================================================

pub async fn list_component_schedules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(comp_id): Path<Uuid>,
    Query(query): Query<ListSchedulesQuery>,
) -> Result<Json<Vec<ScheduleResponse>>, ApiError> {
    let app_id = get_app_id_for_component(&state.db, comp_id.into()).await.ok_or(ApiError::NotFound)?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let include_disabled = query.include_disabled.unwrap_or(false);
    let rows = if include_disabled {
        sched_repo::list_component_schedules_all(&state.db, comp_id).await.map_err(|e| ApiError::Internal(e.to_string()))?
    } else {
        sched_repo::list_component_schedules_enabled(&state.db, comp_id).await.map_err(|e| ApiError::Internal(e.to_string()))?
    };

    let comp_name = sched_repo::get_comp_display_name_for_sched(&state.db, comp_id).await;
    let target_name = comp_name.unwrap_or_else(|| comp_id.to_string());

    let responses = rows.into_iter()
        .map(|row| row_to_response(row, "component".to_string(), comp_id.into(), target_name.clone()))
        .collect();
    Ok(Json(responses))
}

pub async fn create_component_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(comp_id): Path<Uuid>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleResponse>), ApiError> {
    let app_id = get_app_id_for_component(&state.db, comp_id.into()).await.ok_or(ApiError::NotFound)?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate { return Err(ApiError::Forbidden); }

    if !["start", "stop", "restart"].contains(&req.operation.as_str()) {
        return Err(ApiError::Validation(format!("Invalid operation '{}'", req.operation)));
    }

    let cron_expression = resolve_cron(&req)?;
    if !is_valid_cron(&cron_expression) {
        return Err(ApiError::Validation(format!("Invalid cron expression '{}'", cron_expression)));
    }

    let org_id = sched_repo::get_org_id_for_component_app(&state.db, app_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let next_run_at = calculate_next_run(&cron_expression, &req.timezone);
    let _ = audit::log_action(&state.db, user.user_id, "create_schedule", "component", comp_id,
        serde_json::json!({"schedule_name": req.name, "operation": req.operation, "cron_expression": cron_expression}),
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    let schedule_id = Uuid::new_v4();
    sched_repo::create_component_schedule(&state.db, schedule_id, org_id, comp_id,
        &req.name, req.description.as_deref(), &req.operation, &cron_expression, &req.timezone,
        next_run_at, *user.user_id,
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("Schedule not found after creation".to_string()))?;

    let comp_name = sched_repo::get_comp_display_name_for_sched(&state.db, comp_id).await;
    let target_name = comp_name.unwrap_or_else(|| comp_id.to_string());
    Ok((StatusCode::CREATED, Json(row_to_response(row, "component".to_string(), comp_id.into(), target_name))))
}

// ============================================================================
// Individual schedule endpoints
// ============================================================================

pub async fn get_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let app_id = resolve_app_id(&state.db, &row).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let (target_type, target_id, target_name) = get_target_info(&state.db, row.application_id, row.component_id).await;
    Ok(Json(row_to_response(row, target_type, target_id, target_name)))
}

pub async fn update_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let app_id = resolve_app_id(&state.db, &row).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate { return Err(ApiError::Forbidden); }

    let name = req.name.unwrap_or_else(|| row.name.clone());
    let description = req.description.or_else(|| row.description.clone());
    let operation = req.operation.unwrap_or_else(|| row.operation.clone());
    let is_enabled = req.is_enabled.unwrap_or(row.is_enabled);
    let cron_changed = req.cron_expression.is_some() || req.preset.is_some();
    let timezone_changed = req.timezone.is_some();
    let timezone = req.timezone.unwrap_or_else(|| row.timezone.clone());

    let cron_expression = if let Some(cron) = &req.cron_expression { cron.clone() }
    else if let Some(preset) = &req.preset {
        preset_to_cron(preset).ok_or_else(|| ApiError::Validation(format!("Invalid preset '{}'", preset)))?.to_string()
    } else { row.cron_expression.clone() };

    if cron_changed && !is_valid_cron(&cron_expression) {
        return Err(ApiError::Validation(format!("Invalid cron expression '{}'", cron_expression)));
    }

    let next_run_at = if cron_changed || timezone_changed { calculate_next_run(&cron_expression, &timezone) } else { row.next_run_at };

    let _ = audit::log_action(&state.db, user.user_id, "update_schedule", "schedule", schedule_id,
        serde_json::json!({"changes": {"cron_changed": cron_changed, "is_enabled": req.is_enabled}}),
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    sched_repo::update_schedule_fields(&state.db, schedule_id, &name, &description, &operation,
        &cron_expression, &timezone, is_enabled, next_run_at,
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    let updated_row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let (target_type, target_id, target_name) = get_target_info(&state.db, updated_row.application_id, updated_row.component_id).await;
    Ok(Json(row_to_response(updated_row, target_type, target_id, target_name)))
}

pub async fn delete_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let row = sched_repo::fetch_schedule_row(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let app_id = resolve_app_id(&state.db, &row).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate { return Err(ApiError::Forbidden); }

    let _ = audit::log_action(&state.db, user.user_id, "delete_schedule", "schedule", schedule_id,
        serde_json::json!({"schedule_name": row.name, "operation": row.operation}),
    ).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    sched_repo::delete_operation_schedule(&state.db, schedule_id)
        .await.map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Helpers
// ============================================================================

fn resolve_cron(req: &CreateScheduleRequest) -> Result<String, ApiError> {
    if let Some(cron) = &req.cron_expression { Ok(cron.clone()) }
    else if let Some(preset) = &req.preset {
        preset_to_cron(preset).ok_or_else(|| ApiError::Validation(format!("Invalid preset '{}'", preset))).map(|s| s.to_string())
    } else {
        Err(ApiError::Validation("Either cron_expression or preset must be provided".to_string()))
    }
}

async fn resolve_app_id(db: &crate::db::DbPool, row: &ScheduleRow) -> Result<crate::db::DbUuid, ApiError> {
    if let Some(aid) = row.application_id { Ok(aid) }
    else if let Some(cid) = row.component_id {
        get_app_id_for_component(db, cid).await.ok_or(ApiError::NotFound)
    } else {
        Err(ApiError::Internal("Invalid schedule target".to_string()))
    }
}
