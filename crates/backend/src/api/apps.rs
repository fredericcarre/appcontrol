use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct ListAppsQuery {
    pub search: Option<String>,
    pub site_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAppRequest {
    pub name: String,
    pub description: Option<String>,
    pub site_id: Uuid,
    pub tags: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAppRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub site_id: Option<Uuid>,
    pub tags: Option<Value>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AppRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Uuid,
    pub site_id: Uuid,
    pub tags: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct StartAppRequest {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartBranchRequest {
    pub component_id: Option<Uuid>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartToRequest {
    pub target_component_id: Uuid,
    pub dry_run: Option<bool>,
}

pub async fn list_apps(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(params): Query<ListAppsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let apps = sqlx::query_as::<_, AppRow>(
        r#"
        SELECT a.id, a.name, a.description, a.organization_id, a.site_id, a.tags, a.created_at, a.updated_at
        FROM applications a
        WHERE a.organization_id = $1
          AND ($2::text IS NULL OR a.name ILIKE '%' || $2 || '%')
          AND ($3::uuid IS NULL OR a.site_id = $3)
        ORDER BY a.name
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(user.organization_id)
    .bind(&params.search)
    .bind(params.site_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "apps": apps, "total": apps.len() })))
}

pub async fn get_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let app = sqlx::query_as::<_, AppRow>(
        "SELECT id, name, description, organization_id, site_id, tags, created_at, updated_at \
         FROM applications WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(app)))
}

pub async fn create_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Input validation
    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    // Log before execute
    let app_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_app",
        "application",
        app_id,
        json!({ "name": body.name }),
    )
    .await?;

    let app = sqlx::query_as::<_, AppRow>(
        r#"
        INSERT INTO applications (id, name, description, organization_id, site_id, tags)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, name, description, organization_id, site_id, tags, created_at, updated_at
        "#,
    )
    .bind(app_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(user.organization_id)
    .bind(body.site_id)
    .bind(body.tags.as_ref().unwrap_or(&json!([])))
    .fetch_one(&state.db)
    .await?;

    // Grant owner permission to creator
    let _ = sqlx::query(
        "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) \
         VALUES ($1, $2, 'owner', $2)",
    )
    .bind(app_id)
    .bind(user.user_id)
    .execute(&state.db)
    .await;

    Ok((StatusCode::CREATED, Json(json!(app))))
}

pub async fn update_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAppRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    if let Some(ref name) = body.name {
        validate_length("name", name, 1, 200)?;
    }
    validate_optional_length("description", &body.description, 2000)?;

    log_action(
        &state.db,
        user.user_id,
        "update_app",
        "application",
        id,
        json!({"changes": body.name}),
    )
    .await?;

    let app = sqlx::query_as::<_, AppRow>(
        r#"
        UPDATE applications SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            site_id = COALESCE($4, site_id),
            tags = COALESCE($5, tags),
            updated_at = now()
        WHERE id = $1 AND organization_id = $6
        RETURNING id, name, description, organization_id, site_id, tags, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.site_id)
    .bind(&body.tags)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(app)))
}

pub async fn delete_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Owner {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_app",
        "application",
        id,
        json!({}),
    )
    .await?;

    let result = sqlx::query("DELETE FROM applications WHERE id = $1 AND organization_id = $2")
        .bind(id)
        .bind(user.organization_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<Option<StartAppRequest>>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let dry_run = body.and_then(|b| b.dry_run).unwrap_or(false);

    log_action(
        &state.db,
        user.user_id,
        "start_app",
        "application",
        id,
        json!({"dry_run": dry_run}),
    )
    .await?;

    let plan = crate::core::sequencer::build_start_plan(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if dry_run {
        return Ok(Json(json!({ "dry_run": true, "plan": plan })));
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(id, "start", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        if let Err(e) = crate::core::sequencer::execute_start(&state_clone, id).await {
            tracing::error!("Failed to start app {}: {}", id, e);
        }
    });

    Ok(Json(json!({ "status": "starting", "plan": plan })))
}

pub async fn stop_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(id, "stop", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    log_action(
        &state.db,
        user.user_id,
        "stop_app",
        "application",
        id,
        json!({}),
    )
    .await?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        if let Err(e) = crate::core::sequencer::execute_stop(&state_clone, id).await {
            tracing::error!("Failed to stop app {}: {}", id, e);
        }
    });

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn start_branch(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<StartBranchRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // If no component_id provided, find all FAILED components in this application.
    let target_component_ids: Vec<Uuid> = if let Some(cid) = body.component_id {
        vec![cid]
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM components WHERE application_id = $1 AND current_state = 'FAILED'",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    };

    if target_component_ids.is_empty() {
        return Ok(Json(
            json!({ "status": "no_failed_components", "message": "No FAILED components found to restart" }),
        ));
    }

    log_action(
        &state.db,
        user.user_id,
        "start_branch",
        "application",
        id,
        json!({"component_ids": target_component_ids}),
    )
    .await?;

    let branch = crate::core::branch::detect_error_branch(&state.db, id, target_component_ids[0])
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let dry_run = body.dry_run.unwrap_or(false);
    if dry_run {
        return Ok(Json(json!({ "dry_run": true, "branch": branch })));
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(id, "start_branch", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        for component_id in &target_component_ids {
            if let Err(e) = crate::core::fsm::transition_component(
                &state_clone,
                *component_id,
                appcontrol_common::ComponentState::Failed,
            )
            .await
            {
                tracing::warn!(
                    "Could not force component {} to FAILED for branch restart: {}",
                    component_id,
                    e
                );
            }
        }
        if let Err(e) = crate::core::sequencer::execute_start(&state_clone, id).await {
            tracing::error!("Failed to restart branch for app {}: {}", id, e);
        }
    });

    Ok(Json(
        json!({ "status": "starting_branch", "branch": branch }),
    ))
}

pub async fn start_to(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<StartToRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Verify the target component belongs to this application
    let target_app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(body.target_component_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or(ApiError::NotFound)?;

    if target_app_id != id {
        return Err(ApiError::Conflict(
            "Target component does not belong to this application".to_string(),
        ));
    }

    // Build DAG and find all upstream dependencies of the target
    let dag = crate::core::dag::build_dag(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut subset = dag.find_all_dependencies(body.target_component_id);
    subset.insert(body.target_component_id); // Include the target itself

    log_action(
        &state.db,
        user.user_id,
        "start_to",
        "application",
        id,
        json!({
            "target_component_id": body.target_component_id,
            "total_components": subset.len(),
        }),
    )
    .await?;

    let dry_run = body.dry_run.unwrap_or(false);
    if dry_run {
        // Build a plan for the subset
        let sub_dag = dag.sub_dag(&subset);
        let levels = sub_dag
            .topological_levels()
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        let mut plan_levels = Vec::new();
        for level in &levels {
            let mut level_info = Vec::new();
            for &comp_id in level {
                let name =
                    sqlx::query_scalar::<_, String>("SELECT name FROM components WHERE id = $1")
                        .bind(comp_id)
                        .fetch_optional(&state.db)
                        .await
                        .map_err(|e| ApiError::Internal(e.to_string()))?
                        .unwrap_or_else(|| comp_id.to_string());
                level_info.push(json!({"component_id": comp_id, "name": name}));
            }
            plan_levels.push(level_info);
        }

        return Ok(Json(json!({
            "dry_run": true,
            "target_component_id": body.target_component_id,
            "plan": { "levels": plan_levels, "total_levels": levels.len() },
            "total_components": subset.len(),
        })));
    }

    // Acquire operation lock
    let guard = state
        .operation_lock
        .try_lock(id, "start_to", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let total_components = subset.len();
    let state_clone = state.clone();
    let target_id = body.target_component_id;
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(e) =
            crate::core::sequencer::execute_start_subset(&state_clone, id, &subset).await
        {
            tracing::error!(
                "Failed to start-to for app {} (target {}): {}",
                id,
                target_id,
                e
            );
        }
    });

    Ok(Json(json!({
        "status": "starting_to",
        "target_component_id": body.target_component_id,
        "total_components": total_components,
    })))
}
