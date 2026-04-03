//! CRUD endpoints for operation schedules (start/stop/restart automation).
//!
//! Schedules allow automating start/stop/restart operations on applications or components
//! based on cron expressions. Use case: stop app at night, restart every morning.

use crate::db::DbUuid;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::operation_scheduler::{
    calculate_next_run, cron_to_human, is_valid_cron, preset_to_cron,
};
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit;
use crate::AppState;
use appcontrol_common::PermissionLevel;

use std::sync::Arc;

// ============================================================================
// Request/Response DTOs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub operation: String, // "start", "stop", "restart"
    #[serde(default)]
    pub cron_expression: Option<String>, // Raw cron or null for preset
    #[serde(default)]
    pub preset: Option<String>, // "daily_7am", "weekdays_7am", etc.
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "Europe/Paris".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleResponse {
    pub id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub operation: String,
    pub cron_expression: String,
    pub cron_human: String, // "Every day at 7:00 AM"
    pub timezone: String,
    pub is_enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub next_run_relative: Option<String>, // "In 2 hours"
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_status: Option<String>,
    pub last_run_message: Option<String>,
    pub target_type: String, // "application" or "component"
    pub target_id: DbUuid,
    pub target_name: String,
    pub created_by: Option<DbUuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleExecutionResponse {
    pub id: DbUuid,
    pub schedule_id: DbUuid,
    pub action_log_id: Option<DbUuid>,
    pub executed_at: DateTime<Utc>,
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct PresetInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    pub cron: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSchedulesQuery {
    #[serde(default)]
    pub include_disabled: Option<bool>,
}

// ============================================================================
// Database row types
// ============================================================================

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
pub struct ScheduleRow {
    id: DbUuid,
    organization_id: DbUuid,
    application_id: Option<DbUuid>,
    component_id: Option<DbUuid>,
    name: String,
    description: Option<String>,
    operation: String,
    cron_expression: String,
    timezone: String,
    is_enabled: bool,
    last_run_at: Option<DateTime<Utc>>,
    next_run_at: Option<DateTime<Utc>>,
    last_run_status: Option<String>,
    last_run_message: Option<String>,
    created_by: Option<DbUuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct ExecutionRow {
    id: DbUuid,
    schedule_id: DbUuid,
    action_log_id: Option<DbUuid>,
    executed_at: DateTime<Utc>,
    status: String,
    message: Option<String>,
    duration_ms: Option<i32>,
}

// ============================================================================
// Helper functions
// ============================================================================

fn relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = dt - now;
    let secs = diff.num_seconds();

    if secs < 0 {
        return "Overdue".to_string();
    }

    if secs < 60 {
        return format!("In {} seconds", secs);
    }
    if secs < 3600 {
        let mins = secs / 60;
        return format!("In {} minute{}", mins, if mins == 1 { "" } else { "s" });
    }
    if secs < 86400 {
        let hours = secs / 3600;
        return format!("In {} hour{}", hours, if hours == 1 { "" } else { "s" });
    }
    let days = secs / 86400;
    format!("In {} day{}", days, if days == 1 { "" } else { "s" })
}

async fn get_target_info(
    db: &crate::db::DbPool,
    application_id: Option<DbUuid>,
    component_id: Option<DbUuid>,
) -> (String, DbUuid, String) {
    use crate::repository::schedule_queries as sched_repo;
    if let Some(app_id) = application_id {
        let name = sched_repo::get_app_name_by_id(db, *app_id).await;
        (
            "application".to_string(),
            app_id,
            name.unwrap_or_else(|| app_id.to_string()),
        )
    } else if let Some(comp_id) = component_id {
        let name = sched_repo::get_comp_display_name(db, *comp_id).await;
        (
            "component".to_string(),
            comp_id,
            name.unwrap_or_else(|| comp_id.to_string()),
        )
    } else {
        ("unknown".to_string(), DbUuid::nil(), "Unknown".to_string())
    }
}

fn row_to_response(
    row: ScheduleRow,
    target_type: String,
    target_id: DbUuid,
    target_name: String,
) -> ScheduleResponse {
    let next_run_relative = row.next_run_at.map(relative_time);
    let cron_human = cron_to_human(&row.cron_expression);

    ScheduleResponse {
        id: row.id,
        name: row.name,
        description: row.description,
        operation: row.operation,
        cron_expression: row.cron_expression,
        cron_human,
        timezone: row.timezone,
        is_enabled: row.is_enabled,
        next_run_at: row.next_run_at,
        next_run_relative,
        last_run_at: row.last_run_at,
        last_run_status: row.last_run_status,
        last_run_message: row.last_run_message,
        target_type,
        target_id,
        target_name,
        created_by: row.created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Get app_id from component_id for permission checks
async fn get_app_id_for_component(db: &crate::db::DbPool, component_id: DbUuid) -> Option<DbUuid> {
    crate::repository::schedule_queries::get_component_app_id_sched(db, *component_id)
        .await
        .ok()
        .flatten()
        .map(DbUuid::from)
}

// ============================================================================
// Application-level schedule endpoints
// ============================================================================

/// GET /api/v1/apps/:app_id/schedules - List all schedules for an application
pub async fn list_app_schedules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(query): Query<ListSchedulesQuery>,
) -> Result<Json<Vec<ScheduleResponse>>, ApiError> {
    // Check permission (view is enough to list schedules)
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let include_disabled = query.include_disabled.unwrap_or(false);

    use crate::repository::schedule_queries as sched_repo;
    let rows = if include_disabled {
        sched_repo::list_app_schedules_all(&state.db, app_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    } else {
        sched_repo::list_app_schedules(&state.db, app_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    };

    // Get app name once for all schedules
    let app_name = sched_repo::get_app_name_by_id(&state.db, app_id).await;
    let target_name = app_name.unwrap_or_else(|| app_id.to_string());

    let responses: Vec<ScheduleResponse> = rows
        .into_iter()
        .map(|row| {
            row_to_response(
                row,
                "application".to_string(),
                app_id.into(),
                target_name.clone(),
            )
        })
        .collect();

    Ok(Json(responses))
}

/// POST /api/v1/apps/:app_id/schedules - Create a new schedule for an application
pub async fn create_app_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleResponse>), ApiError> {
    // Check permission (operate required to create schedules)
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Validate operation
    if !["start", "stop", "restart"].contains(&req.operation.as_str()) {
        return Err(ApiError::Validation(format!(
            "Invalid operation '{}'. Must be 'start', 'stop', or 'restart'.",
            req.operation
        )));
    }

    // Resolve cron expression
    let cron_expression = if let Some(cron) = &req.cron_expression {
        cron.clone()
    } else if let Some(preset) = &req.preset {
        preset_to_cron(preset)
            .ok_or_else(|| ApiError::Validation(format!("Invalid preset '{}'", preset)))?
            .to_string()
    } else {
        return Err(ApiError::Validation(
            "Either cron_expression or preset must be provided".to_string(),
        ));
    };

    // Validate cron expression
    if !is_valid_cron(&cron_expression) {
        return Err(ApiError::Validation(format!(
            "Invalid cron expression '{}'",
            cron_expression
        )));
    }

    // Get org_id from application
    let org_id: Uuid = sqlx::query_scalar("SELECT organization_id FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound)?;

    // Calculate initial next_run_at
    let next_run_at = calculate_next_run(&cron_expression, &req.timezone);

    // Log action
    let _action_id = audit::log_action(
        &state.db,
        user.user_id,
        "create_schedule",
        "application",
        app_id,
        serde_json::json!({
            "schedule_name": req.name,
            "operation": req.operation,
            "cron_expression": cron_expression,
        }),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Create schedule
    let schedule_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO operation_schedules
            (id, organization_id, application_id, name, description, operation,
             cron_expression, timezone, is_enabled, next_run_at, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, $9, $10)
        "#,
    )
    .bind(schedule_id)
    .bind(org_id)
    .bind(crate::db::bind_id(app_id))
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.operation)
    .bind(&cron_expression)
    .bind(&req.timezone)
    .bind(next_run_at)
    .bind(crate::db::bind_id(user.user_id))
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Fetch the created schedule
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let app_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
            .bind(crate::db::bind_id(app_id))
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let target_name = app_name.unwrap_or_else(|| app_id.to_string());

    let response = row_to_response(row, "application".to_string(), app_id.into(), target_name);

    Ok((StatusCode::CREATED, Json(response)))
}

// ============================================================================
// Component-level schedule endpoints
// ============================================================================

/// GET /api/v1/components/:comp_id/schedules - List all schedules for a component
pub async fn list_component_schedules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(comp_id): Path<Uuid>,
    Query(query): Query<ListSchedulesQuery>,
) -> Result<Json<Vec<ScheduleResponse>>, ApiError> {
    // Get app_id for permission check
    let app_id = get_app_id_for_component(&state.db, comp_id.into())
        .await
        .ok_or_else(|| ApiError::NotFound)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let include_disabled = query.include_disabled.unwrap_or(false);

    let rows = if include_disabled {
        sqlx::query_as::<_, ScheduleRow>(
            r#"
            SELECT id, organization_id, application_id, component_id, name, description,
                   operation, cron_expression, timezone, is_enabled,
                   last_run_at, next_run_at, last_run_status, last_run_message,
                   created_by, created_at, updated_at
            FROM operation_schedules
            WHERE component_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(crate::db::bind_id(comp_id))
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    } else {
        sqlx::query_as::<_, ScheduleRow>(
            r#"
            SELECT id, organization_id, application_id, component_id, name, description,
                   operation, cron_expression, timezone, is_enabled,
                   last_run_at, next_run_at, last_run_status, last_run_message,
                   created_by, created_at, updated_at
            FROM operation_schedules
            WHERE component_id = $1 AND is_enabled = true
            ORDER BY created_at DESC
            "#,
        )
        .bind(crate::db::bind_id(comp_id))
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    };

    // Get component name
    let comp_name: Option<String> =
        sqlx::query_scalar("SELECT COALESCE(display_name, name) FROM components WHERE id = $1")
            .bind(crate::db::bind_id(comp_id))
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let target_name = comp_name.unwrap_or_else(|| comp_id.to_string());

    let responses: Vec<ScheduleResponse> = rows
        .into_iter()
        .map(|row| {
            row_to_response(
                row,
                "component".to_string(),
                comp_id.into(),
                target_name.clone(),
            )
        })
        .collect();

    Ok(Json(responses))
}

/// POST /api/v1/components/:comp_id/schedules - Create a new schedule for a component
pub async fn create_component_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(comp_id): Path<Uuid>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleResponse>), ApiError> {
    // Get app_id for permission check
    let app_id = get_app_id_for_component(&state.db, comp_id.into())
        .await
        .ok_or_else(|| ApiError::NotFound)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Validate operation
    if !["start", "stop", "restart"].contains(&req.operation.as_str()) {
        return Err(ApiError::Validation(format!(
            "Invalid operation '{}'. Must be 'start', 'stop', or 'restart'.",
            req.operation
        )));
    }

    // Resolve cron expression
    let cron_expression = if let Some(cron) = &req.cron_expression {
        cron.clone()
    } else if let Some(preset) = &req.preset {
        preset_to_cron(preset)
            .ok_or_else(|| ApiError::Validation(format!("Invalid preset '{}'", preset)))?
            .to_string()
    } else {
        return Err(ApiError::Validation(
            "Either cron_expression or preset must be provided".to_string(),
        ));
    };

    // Validate cron expression
    if !is_valid_cron(&cron_expression) {
        return Err(ApiError::Validation(format!(
            "Invalid cron expression '{}'",
            cron_expression
        )));
    }

    // Get org_id from application
    let org_id: Uuid = sqlx::query_scalar("SELECT organization_id FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound)?;

    // Calculate initial next_run_at
    let next_run_at = calculate_next_run(&cron_expression, &req.timezone);

    // Log action
    let _action_id = audit::log_action(
        &state.db,
        user.user_id,
        "create_schedule",
        "component",
        comp_id,
        serde_json::json!({
            "schedule_name": req.name,
            "operation": req.operation,
            "cron_expression": cron_expression,
        }),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Create schedule
    let schedule_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO operation_schedules
            (id, organization_id, component_id, name, description, operation,
             cron_expression, timezone, is_enabled, next_run_at, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, $9, $10)
        "#,
    )
    .bind(schedule_id)
    .bind(org_id)
    .bind(crate::db::bind_id(comp_id))
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.operation)
    .bind(&cron_expression)
    .bind(&req.timezone)
    .bind(next_run_at)
    .bind(crate::db::bind_id(user.user_id))
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Fetch the created schedule
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let comp_name: Option<String> =
        sqlx::query_scalar("SELECT COALESCE(display_name, name) FROM components WHERE id = $1")
            .bind(crate::db::bind_id(comp_id))
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let target_name = comp_name.unwrap_or_else(|| comp_id.to_string());

    let response = row_to_response(row, "component".to_string(), comp_id.into(), target_name);

    Ok((StatusCode::CREATED, Json(response)))
}

// ============================================================================
// Individual schedule endpoints
// ============================================================================

/// GET /api/v1/schedules/:id - Get a single schedule
pub async fn get_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound)?;

    // Check permission
    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or_else(|| ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let (target_type, target_id, target_name) =
        get_target_info(&state.db, row.application_id, row.component_id).await;

    Ok(Json(row_to_response(
        row,
        target_type,
        target_id,
        target_name,
    )))
}

/// PUT /api/v1/schedules/:id - Update a schedule
pub async fn update_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    // Fetch schedule first
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound)?;

    // Check permission (operate required to modify schedules)
    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or_else(|| ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Track which fields were provided for audit log
    let name_changed = req.name.is_some();
    let operation_changed = req.operation.is_some();
    let cron_changed = req.cron_expression.is_some() || req.preset.is_some();
    let timezone_changed = req.timezone.is_some();

    // Build update values
    let name = req.name.unwrap_or_else(|| row.name.clone());
    let description = req.description.or_else(|| row.description.clone());
    let operation = req.operation.unwrap_or_else(|| row.operation.clone());
    let is_enabled = req.is_enabled.unwrap_or(row.is_enabled);
    let timezone = req.timezone.unwrap_or_else(|| row.timezone.clone());

    // Handle cron expression update
    let cron_expression = if let Some(cron) = &req.cron_expression {
        cron.clone()
    } else if let Some(preset) = &req.preset {
        preset_to_cron(preset)
            .ok_or_else(|| ApiError::Validation(format!("Invalid preset '{}'", preset)))?
            .to_string()
    } else {
        row.cron_expression.clone()
    };

    // Validate operation if changed
    if operation_changed && !["start", "stop", "restart"].contains(&operation.as_str()) {
        return Err(ApiError::Validation(format!(
            "Invalid operation '{}'. Must be 'start', 'stop', or 'restart'.",
            operation
        )));
    }

    // Validate cron if changed
    if cron_changed && !is_valid_cron(&cron_expression) {
        return Err(ApiError::Validation(format!(
            "Invalid cron expression '{}'",
            cron_expression
        )));
    }

    // Recalculate next_run_at if cron or timezone changed
    let next_run_at = if cron_changed || timezone_changed {
        calculate_next_run(&cron_expression, &timezone)
    } else {
        row.next_run_at
    };

    // Log action
    let _action_id = audit::log_action(
        &state.db,
        user.user_id,
        "update_schedule",
        "schedule",
        schedule_id,
        serde_json::json!({
            "changes": {
                "name_changed": name_changed,
                "operation_changed": operation_changed,
                "cron_changed": cron_changed,
                "is_enabled": req.is_enabled,
            }
        }),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Update schedule
    sqlx::query(&format!(
        "UPDATE operation_schedules
             SET name = $2, description = $3, operation = $4, cron_expression = $5,
                 timezone = $6, is_enabled = $7, next_run_at = $8, updated_at = {}
             WHERE id = $1",
        crate::db::sql::now()
    ))
    .bind(schedule_id)
    .bind(&name)
    .bind(&description)
    .bind(&operation)
    .bind(&cron_expression)
    .bind(&timezone)
    .bind(is_enabled)
    .bind(next_run_at)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Fetch updated schedule
    let updated_row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let (target_type, target_id, target_name) = get_target_info(
        &state.db,
        updated_row.application_id,
        updated_row.component_id,
    )
    .await;

    Ok(Json(row_to_response(
        updated_row,
        target_type,
        target_id,
        target_name,
    )))
}

/// DELETE /api/v1/schedules/:id - Delete a schedule
pub async fn delete_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Fetch schedule first to check permission
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound)?;

    // Check permission (operate required to delete schedules)
    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or_else(|| ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Log action
    let _action_id = audit::log_action(
        &state.db,
        user.user_id,
        "delete_schedule",
        "schedule",
        schedule_id,
        serde_json::json!({
            "schedule_name": row.name,
            "operation": row.operation,
        }),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Delete schedule (CASCADE will delete executions)
    sqlx::query("DELETE FROM operation_schedules WHERE id = $1")
        .bind(schedule_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/schedules/:id/toggle - Toggle schedule enabled/disabled
pub async fn toggle_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    // Fetch schedule
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound)?;

    // Check permission
    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or_else(|| ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let new_enabled = !row.is_enabled;

    // Recalculate next_run_at if enabling
    let next_run_at = if new_enabled {
        calculate_next_run(&row.cron_expression, &row.timezone)
    } else {
        None
    };

    // Log action
    let _action_id = audit::log_action(
        &state.db,
        user.user_id,
        if new_enabled {
            "enable_schedule"
        } else {
            "disable_schedule"
        },
        "schedule",
        schedule_id,
        serde_json::json!({
            "schedule_name": row.name,
        }),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Update
    sqlx::query(&format!(
        "UPDATE operation_schedules
             SET is_enabled = $2, next_run_at = $3, updated_at = {}
             WHERE id = $1",
        crate::db::sql::now()
    ))
    .bind(schedule_id)
    .bind(new_enabled)
    .bind(next_run_at)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Return updated schedule
    get_schedule(State(state), Extension(user), Path(schedule_id)).await
}

/// POST /api/v1/schedules/:id/run-now - Execute schedule immediately
pub async fn run_schedule_now(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Fetch schedule
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound)?;

    // Check permission (operate required to run schedules)
    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or_else(|| ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Set next_run_at to now - the scheduler will pick it up on next tick
    sqlx::query(&format!(
        "UPDATE operation_schedules
             SET next_run_at = {now}, updated_at = {now}
             WHERE id = $1",
        now = crate::db::sql::now()
    ))
    .bind(schedule_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Log action
    let _action_id = audit::log_action(
        &state.db,
        user.user_id,
        "run_schedule_now",
        "schedule",
        schedule_id,
        serde_json::json!({
            "schedule_name": row.name,
            "operation": row.operation,
        }),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "message": "Schedule queued for immediate execution",
        "schedule_id": schedule_id,
    })))
}

/// GET /api/v1/schedules/:id/executions - List execution history for a schedule
pub async fn list_schedule_executions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<Vec<ScheduleExecutionResponse>>, ApiError> {
    // Fetch schedule first to check permission
    let row = sqlx::query_as::<_, ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound)?;

    // Check permission
    let app_id = if let Some(aid) = row.application_id {
        aid
    } else if let Some(cid) = row.component_id {
        get_app_id_for_component(&state.db, cid)
            .await
            .ok_or_else(|| ApiError::NotFound)?
    } else {
        return Err(ApiError::Internal("Invalid schedule target".to_string()));
    };

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let executions = sqlx::query_as::<_, ExecutionRow>(
        r#"
        SELECT id, schedule_id, action_log_id, executed_at, status, message, duration_ms
        FROM operation_schedule_executions
        WHERE schedule_id = $1
        ORDER BY executed_at DESC
        LIMIT 100
        "#,
    )
    .bind(schedule_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let responses: Vec<ScheduleExecutionResponse> = executions
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

/// GET /api/v1/schedules/presets - Get available preset options
pub async fn list_presets() -> Json<Vec<PresetInfo>> {
    let presets = vec![
        PresetInfo {
            id: "daily_7am".to_string(),
            label: "Daily at 7:00 AM".to_string(),
            description: "Every day at 7 AM".to_string(),
            cron: "0 7 * * *".to_string(),
        },
        PresetInfo {
            id: "daily_8am".to_string(),
            label: "Daily at 8:00 AM".to_string(),
            description: "Every day at 8 AM".to_string(),
            cron: "0 8 * * *".to_string(),
        },
        PresetInfo {
            id: "daily_19h".to_string(),
            label: "Daily at 7:00 PM".to_string(),
            description: "Every day at 7 PM".to_string(),
            cron: "0 19 * * *".to_string(),
        },
        PresetInfo {
            id: "daily_22h".to_string(),
            label: "Daily at 10:00 PM".to_string(),
            description: "Every day at 10 PM".to_string(),
            cron: "0 22 * * *".to_string(),
        },
        PresetInfo {
            id: "weekdays_7am".to_string(),
            label: "Weekdays at 7:00 AM".to_string(),
            description: "Monday to Friday at 7 AM".to_string(),
            cron: "0 7 * * 1-5".to_string(),
        },
        PresetInfo {
            id: "weekdays_8am".to_string(),
            label: "Weekdays at 8:00 AM".to_string(),
            description: "Monday to Friday at 8 AM".to_string(),
            cron: "0 8 * * 1-5".to_string(),
        },
        PresetInfo {
            id: "weekdays_19h".to_string(),
            label: "Weekdays at 7:00 PM".to_string(),
            description: "Monday to Friday at 7 PM".to_string(),
            cron: "0 19 * * 1-5".to_string(),
        },
        PresetInfo {
            id: "weekdays_22h".to_string(),
            label: "Weekdays at 10:00 PM".to_string(),
            description: "Monday to Friday at 10 PM".to_string(),
            cron: "0 22 * * 1-5".to_string(),
        },
        PresetInfo {
            id: "weekly_sunday_3am".to_string(),
            label: "Sundays at 3:00 AM".to_string(),
            description: "Every Sunday at 3 AM".to_string(),
            cron: "0 3 * * 0".to_string(),
        },
        PresetInfo {
            id: "weekly_saturday_3am".to_string(),
            label: "Saturdays at 3:00 AM".to_string(),
            description: "Every Saturday at 3 AM".to_string(),
            cron: "0 3 * * 6".to_string(),
        },
        PresetInfo {
            id: "monthly_1st_3am".to_string(),
            label: "Monthly (1st at 3 AM)".to_string(),
            description: "First day of each month at 3 AM".to_string(),
            cron: "0 3 1 * *".to_string(),
        },
        PresetInfo {
            id: "every_hour".to_string(),
            label: "Every hour".to_string(),
            description: "At the start of every hour".to_string(),
            cron: "0 * * * *".to_string(),
        },
        PresetInfo {
            id: "every_30min".to_string(),
            label: "Every 30 minutes".to_string(),
            description: "Every 30 minutes".to_string(),
            cron: "*/30 * * * *".to_string(),
        },
    ];

    Json(presets)
}
