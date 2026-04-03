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
use crate::db::{DbJson, DbUuid};
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::middleware::audit::{complete_action_failed, complete_action_success, log_action};
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
    /// Site ID (optional - auto-selects default site if not provided)
    pub site_id: Option<Uuid>,
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
    pub id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: DbUuid,
    pub site_id: DbUuid,
    pub tags: DbJson,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComponentRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<DbUuid>,
    pub component_type: String,
    pub host: Option<String>,
    pub agent_id: Option<DbUuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub current_state: String,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: DbUuid,
    pub from_component_id: DbUuid,
    pub to_component_id: DbUuid,
}

#[derive(Debug, Deserialize)]
pub struct StartAppRequest {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StopAppRequest {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartBranchRequest {
    pub component_id: Option<DbUuid>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartToRequest {
    pub target_component_id: DbUuid,
    pub dry_run: Option<bool>,
}

/// Computed application status based on component states
#[derive(Debug, Serialize)]
pub struct AppWithStatus {
    #[serde(flatten)]
    pub app: AppRow,
    pub component_count: i64,
    pub running_count: i64,
    pub stopped_count: i64,
    pub failed_count: i64,
    pub global_state: String,
    pub weather: String,
}

/// Compute global state and weather from component counts
fn compute_app_status(
    running: i64,
    stopped: i64,
    failed: i64,
    starting: i64,
    stopping: i64,
    total: i64,
) -> (String, String) {
    if total == 0 {
        return ("UNKNOWN".to_string(), "cloudy".to_string());
    }

    let global_state;
    let weather;

    if failed > 0 {
        global_state = "FAILED".to_string();
        weather = "stormy".to_string();
    } else if starting > 0 || stopping > 0 {
        // Transitional states take precedence (operation in progress)
        if starting > 0 {
            global_state = "STARTING".to_string();
        } else {
            global_state = "STOPPING".to_string();
        }
        weather = "rainy".to_string();
    } else if running == total {
        global_state = "RUNNING".to_string();
        weather = "sunny".to_string();
    } else if stopped == total {
        global_state = "STOPPED".to_string();
        weather = "cloudy".to_string();
    } else if running > 0 && stopped > 0 {
        global_state = "DEGRADED".to_string();
        weather = "rainy".to_string();
    } else if running > 0 {
        global_state = "RUNNING".to_string();
        weather = "fair".to_string();
    } else {
        global_state = "UNKNOWN".to_string();
        weather = "cloudy".to_string();
    }

    (global_state, weather)
}

pub async fn list_apps(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(params): Query<ListAppsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    use crate::repository::apps::ListAppsParams;

    let apps = state
        .app_repo
        .list_apps(ListAppsParams {
            organization_id: *user.organization_id,
            search: params.search.clone(),
            site_id: params.site_id,
            limit,
            offset,
        })
        .await?;

    // Transform to response with computed status
    let apps_with_status: Vec<_> = apps
        .into_iter()
        .map(|a| {
            let component_count = a.component_count;
            let running_count = a.running_count;
            let starting_count = a.starting_count;
            let stopping_count = a.stopping_count;
            let stopped_count = a.stopped_count;
            let failed_count = a.failed_count;
            let unreachable_count = a.unreachable_count;

            let (global_state, weather) = compute_app_status(
                running_count,
                stopped_count,
                failed_count,
                starting_count,
                stopping_count,
                component_count,
            );

            json!({
                "id": a.id,
                "name": a.name,
                "description": a.description,
                "org_id": a.organization_id,
                "site_id": a.site_id,
                "tags": a.tags,
                "created_at": a.created_at,
                "updated_at": a.updated_at,
                "component_count": component_count,
                "running_count": running_count,
                "starting_count": starting_count,
                "stopping_count": stopping_count,
                "stopped_count": stopped_count,
                "failed_count": failed_count,
                "unreachable_count": unreachable_count,
                "global_state": global_state,
                "weather": weather,
            })
        })
        .collect();

    let total = apps_with_status.len();
    Ok(Json(json!({ "apps": apps_with_status, "total": total })))
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

    let app = state
        .app_repo
        .get_app(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    // Fetch components with agent info via repository
    let components = state.app_repo.get_components_with_agents(id).await?;

    // Collect referenced app IDs to compute their statuses (for application-type components)
    let referenced_app_ids: Vec<Uuid> = components
        .iter()
        .filter_map(|c| c.referenced_app_id)
        .collect();

    // Fetch status counts and names for referenced apps
    let mut referenced_app_statuses: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    let mut referenced_app_names: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();

    if !referenced_app_ids.is_empty() {
        let status_rows = state
            .app_repo
            .get_referenced_app_statuses(&referenced_app_ids)
            .await?;
        for ref_status in status_rows {
            let (computed_state, _) = compute_app_status(
                ref_status.running_count,
                ref_status.stopped_count,
                ref_status.failed_count,
                ref_status.starting_count,
                ref_status.stopping_count,
                ref_status.component_count,
            );
            referenced_app_names.insert(ref_status.app_id, ref_status.app_name);
            referenced_app_statuses.insert(ref_status.app_id, computed_state);
        }
    }

    // Fetch dependencies via repository
    let dependencies = state.app_repo.get_app_dependencies(id).await?;

    // Get live connection status from WebSocket hub
    let connected_agents: std::collections::HashSet<Uuid> =
        state.ws_hub.connected_agent_ids().into_iter().collect();
    let connected_gateways: std::collections::HashSet<Uuid> =
        state.ws_hub.connected_gateway_ids().into_iter().collect();

    // Enrich components with connectivity status
    let components_json: Vec<Value> = components
        .into_iter()
        .map(|c| {
            let agent_connected = c
                .agent_id
                .map(|aid| connected_agents.contains(&aid))
                .unwrap_or(false);
            let gateway_connected = c
                .gateway_id
                .map(|gid| connected_gateways.contains(&gid))
                .unwrap_or(false);

            // Determine connectivity status
            let connectivity_status = if c.agent_id.is_none() {
                "no_agent"
            } else if !gateway_connected && c.gateway_id.is_some() {
                "gateway_disconnected"
            } else if !agent_connected {
                "agent_disconnected"
            } else {
                "connected"
            };

            // For application-type components, derive state from referenced app
            // If component_type is 'application' but no referenced_app_id, show UNKNOWN
            let derived_state = if c.component_type == "application" {
                match c.referenced_app_id {
                    Some(ref_id) => referenced_app_statuses
                        .get(&ref_id)
                        .cloned()
                        .unwrap_or_else(|| "UNKNOWN".to_string()),
                    None => "UNKNOWN".to_string(), // Misconfigured: application type without referenced app
                }
            } else {
                c.current_state.clone()
            };

            json!({
                "id": c.id,
                "application_id": c.application_id,
                "name": c.name,
                "display_name": c.display_name,
                "description": c.description,
                "icon": c.icon,
                "group_id": c.group_id,
                "component_type": c.component_type,
                "host": c.host,
                "agent_id": c.agent_id,
                "check_cmd": c.check_cmd,
                "start_cmd": c.start_cmd,
                "stop_cmd": c.stop_cmd,
                "check_interval_seconds": c.check_interval_seconds,
                "start_timeout_seconds": c.start_timeout_seconds,
                "stop_timeout_seconds": c.stop_timeout_seconds,
                "is_optional": c.is_optional,
                "current_state": derived_state,
                "position_x": c.position_x,
                "position_y": c.position_y,
                "cluster_size": c.cluster_size,
                "cluster_nodes": c.cluster_nodes,
                "referenced_app_id": c.referenced_app_id,
                "referenced_app_name": c.referenced_app_id.and_then(|ref_id| referenced_app_names.get(&ref_id).cloned()),
                "created_at": c.created_at,
                "updated_at": c.updated_at,
                // Connectivity info
                "agent_hostname": c.agent_hostname,
                "agent_connected": agent_connected,
                "gateway_id": c.gateway_id,
                "gateway_name": c.gateway_name,
                "gateway_connected": gateway_connected,
                "connectivity_status": connectivity_status,
                // Latest metrics from check command
                "last_check_metrics": c.last_check_metrics,
            })
        })
        .collect();

    Ok(Json(json!({
        "id": app.id,
        "name": app.name,
        "description": app.description,
        "organization_id": app.organization_id,
        "site_id": app.site_id,
        "tags": app.tags,
        "created_at": app.created_at,
        "updated_at": app.updated_at,
        "components": components_json,
        "dependencies": dependencies
    })))
}

pub async fn create_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Input validation
    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    // Resolve site_id: use provided value or auto-select default site
    let site_id = match body.site_id {
        Some(id) => id,
        None => {
            match state
                .app_repo
                .find_default_site(*user.organization_id)
                .await?
            {
                Some(id) => id,
                None => {
                    // Create a default site if none exists
                    let new_site_id = Uuid::new_v4();
                    state
                        .app_repo
                        .create_default_site(new_site_id, *user.organization_id)
                        .await?;
                    new_site_id
                }
            }
        }
    };

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

    use crate::repository::apps::CreateApp;

    let app = state
        .app_repo
        .create_app(CreateApp {
            id: app_id,
            name: body.name.clone(),
            description: body.description.clone(),
            organization_id: *user.organization_id,
            site_id,
            tags: body.tags.clone().unwrap_or(json!([])),
        })
        .await?;

    // Grant owner permission to creator
    let _ = state
        .app_repo
        .grant_owner_permission(app_id, *user.user_id)
        .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": app.id,
            "name": app.name,
            "description": app.description,
            "organization_id": app.organization_id,
            "site_id": app.site_id,
            "tags": app.tags,
            "created_at": app.created_at,
            "updated_at": app.updated_at,
        })),
    ))
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

    // Snapshot before state for config_versions
    let before_snapshot = state
        .app_repo
        .get_app(id, *user.organization_id)
        .await?
        .map(|a| {
            json!({
                "id": a.id, "name": a.name, "description": a.description,
                "organization_id": a.organization_id, "site_id": a.site_id,
                "tags": a.tags, "created_at": a.created_at, "updated_at": a.updated_at,
            })
        });

    let app = state
        .app_repo
        .update_app(
            id,
            *user.organization_id,
            body.name.as_deref(),
            body.description.as_deref(),
            body.tags.as_ref(),
            body.site_id,
        )
        .await?
        .ok_or_not_found()?;

    // Record config_versions snapshot
    let after_snapshot = json!({
        "id": app.id, "name": app.name, "description": app.description,
        "organization_id": app.organization_id, "site_id": app.site_id,
        "tags": app.tags, "created_at": app.created_at, "updated_at": app.updated_at,
    });
    let before_json = before_snapshot
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let after_json = after_snapshot.to_string();

    state
        .app_repo
        .insert_config_version("application", id, *user.user_id, &before_json, &after_json)
        .await?;

    Ok(Json(after_snapshot))
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

    let deleted = state.app_repo.delete_app(id, *user.organization_id).await?;

    if !deleted {
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
    // Verify app belongs to user's org
    state
        .app_repo
        .verify_app_org(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let dry_run = body.and_then(|b| b.dry_run).unwrap_or(false);

    let action_id = log_action(
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
        // Mark action as success for dry run
        let _ = complete_action_success(&state.db, action_id).await;
        return Ok(Json(json!({ "dry_run": true, "plan": plan })));
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(id, "start", *user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        match crate::core::sequencer::execute_start(&state_clone, id).await {
            Ok(()) => {
                let _ = complete_action_success(&state_clone.db, action_id).await;
                tracing::info!("Successfully started app {}", id);
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                let _ = complete_action_failed(&state_clone.db, action_id, &error_msg).await;
                tracing::error!("Failed to start app {}: {}", id, e);
            }
        }
    });

    Ok(Json(json!({ "status": "starting", "plan": plan })))
}

pub async fn stop_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<Option<StopAppRequest>>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let dry_run = body.and_then(|b| b.dry_run).unwrap_or(false);

    let action_id = log_action(
        &state.db,
        user.user_id,
        "stop_app",
        "application",
        id,
        json!({"dry_run": dry_run}),
    )
    .await?;

    if dry_run {
        // Build a stop plan (reverse of start plan)
        let plan = crate::core::sequencer::build_start_plan(&state.db, id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let _ = complete_action_success(&state.db, action_id).await;
        return Ok(Json(json!({ "dry_run": true, "plan": plan })));
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(id, "stop", *user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        match crate::core::sequencer::execute_stop(&state_clone, id).await {
            Ok(()) => {
                let _ = complete_action_success(&state_clone.db, action_id).await;
                tracing::info!("Successfully stopped app {}", id);
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                let _ = complete_action_failed(&state_clone.db, action_id, &error_msg).await;
                tracing::error!("Failed to stop app {}: {}", id, e);
            }
        }
    });

    Ok(Json(json!({ "status": "stopping" })))
}

/// Request cancellation of a running operation on an application.
/// The operation will check for cancellation and stop gracefully.
pub async fn cancel_operation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Get current operation info before cancelling
    let active_op = state
        .operation_lock
        .get_active(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    log_action(
        &state.db,
        user.user_id,
        "cancel_operation",
        "application",
        id,
        json!({ "active_operation": active_op.as_ref().map(|o| &o.operation) }),
    )
    .await?;

    let cancelled = state
        .operation_lock
        .request_cancel(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if cancelled {
        tracing::warn!(
            app_id = %id,
            user_id = %user.user_id,
            "Operation cancellation requested"
        );
        Ok(Json(json!({
            "status": "cancelling",
            "message": "Cancellation requested. The operation will stop at the next check point."
        })))
    } else {
        Ok(Json(json!({
            "status": "no_operation",
            "message": "No active operation to cancel"
        })))
    }
}

/// Force-release an operation lock. Use as last resort when cancel doesn't work.
/// This immediately removes the lock, potentially leaving the operation orphaned.
pub async fn force_unlock_operation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        // Force unlock requires higher privileges
        return Err(ApiError::Forbidden);
    }

    // Get current operation info before force-unlocking
    let active_op = state
        .operation_lock
        .get_active(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    log_action(
        &state.db,
        user.user_id,
        "force_unlock_operation",
        "application",
        id,
        json!({ "active_operation": active_op.as_ref().map(|o| &o.operation) }),
    )
    .await?;

    let released = state
        .operation_lock
        .force_unlock(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if released {
        tracing::warn!(
            app_id = %id,
            user_id = %user.user_id,
            "Operation lock force-released"
        );
        Ok(Json(json!({
            "status": "force_unlocked",
            "message": "Lock force-released. Any running operation may be orphaned."
        })))
    } else {
        Ok(Json(json!({
            "status": "no_lock",
            "message": "No lock to release"
        })))
    }
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
        vec![*cid]
    } else {
        state
            .app_repo
            .get_failed_component_ids(id)
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
        .try_lock(id, "start_branch", *user.user_id)
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
    let target_app_id = state
        .component_repo
        .get_component_app_id(*body.target_component_id)
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
    subset.insert(*body.target_component_id); // Include the target itself

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
                let name = state
                    .app_repo
                    .get_component_name(comp_id)
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
        .try_lock(id, "start_to", *user.user_id)
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

/// PUT /api/v1/applications/:id/suspend
///
/// Suspend an application. The agent will stop health checks for all components
/// in this application until it is resumed.
pub async fn suspend_application(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Check permission (requires manage or higher)
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Check if already suspended
    let is_suspended = state
        .app_repo
        .is_app_suspended(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if is_suspended {
        return Err(ApiError::Conflict(
            "Application is already suspended".to_string(),
        ));
    }

    // Suspend the application
    state
        .app_repo
        .suspend_app(id, *user.user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Log action
    let _ = log_action(
        &state.db,
        user.user_id,
        "suspend_application",
        "application",
        id,
        json!({}),
    )
    .await;

    // Push config update to affected agents (they will stop checking these components)
    crate::websocket::push_config_to_affected_agents(&state, Some(id), None, None).await;

    // Get app name for response
    let name: String = state
        .app_repo
        .get_app_name(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .unwrap_or_default();

    tracing::info!(
        application_id = %id,
        user_id = %user.user_id,
        "Application suspended"
    );

    Ok(Json(json!({
        "id": id,
        "name": name,
        "is_suspended": true,
        "suspended_at": chrono::Utc::now(),
        "message": "Application suspended. Agent will stop health checks for all components."
    })))
}

/// PUT /api/v1/applications/:id/resume
///
/// Resume a suspended application. The agent will restart health checks for all
/// components in this application.
pub async fn resume_application(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Check permission (requires manage or higher)
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Check if suspended
    let is_suspended = state
        .app_repo
        .is_app_suspended(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !is_suspended {
        return Err(ApiError::Conflict(
            "Application is not suspended".to_string(),
        ));
    }

    // Resume the application
    state
        .app_repo
        .resume_app(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Log action
    let _ = log_action(
        &state.db,
        user.user_id,
        "resume_application",
        "application",
        id,
        json!({}),
    )
    .await;

    // Push config update to affected agents (they will start checking these components again)
    crate::websocket::push_config_to_affected_agents(&state, Some(id), None, None).await;

    // Get app name for response
    let name: String = state
        .app_repo
        .get_app_name(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .unwrap_or_default();

    tracing::info!(
        application_id = %id,
        user_id = %user.user_id,
        "Application resumed"
    );

    Ok(Json(json!({
        "id": id,
        "name": name,
        "is_suspended": false,
        "message": "Application resumed. Agent will restart health checks for all components."
    })))
}

/// GET /api/v1/apps/:app_id/site-bindings
///
/// Returns all site bindings for components in the given application.
/// This is based on binding profiles (which define where components run)
/// merged with any command overrides from site_overrides table.
///
/// Each component shows:
/// - All sites where it has a binding (from binding_profile_mappings)
/// - The agent assigned for each site
/// - Any command overrides for that site (from site_overrides)
/// - Whether this is the active profile
pub async fn get_site_overrides(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Verify app belongs to org
    state
        .app_repo
        .verify_app_org(app_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    // Fetch the application's primary site info
    let primary_site = state.app_repo.get_app_site_info(app_id).await?;

    // Fetch all binding profile mappings with site info
    use crate::repository::apps::CmdOverride;

    let bindings = state.app_repo.get_site_bindings(app_id).await?;

    // Fetch command overrides from site_overrides table
    let cmd_overrides = state.app_repo.get_cmd_overrides(app_id).await?;

    // Create a lookup map for command overrides
    let cmd_override_map: std::collections::HashMap<(Uuid, Uuid), &CmdOverride> = cmd_overrides
        .iter()
        .map(|o| ((o.component_id, o.site_id), o))
        .collect();

    // Build the response - group bindings by component
    let mut component_map: std::collections::HashMap<Uuid, Vec<Value>> =
        std::collections::HashMap::new();

    for binding in &bindings {
        let cmd_override = cmd_override_map.get(&(binding.component_id, binding.site_id));

        let has_overrides = cmd_override.is_some_and(|o| {
            o.check_cmd_override.is_some()
                || o.start_cmd_override.is_some()
                || o.stop_cmd_override.is_some()
        });

        let site_binding = json!({
            "site_id": binding.site_id,
            "site_name": binding.site_name,
            "site_code": binding.site_code,
            "site_type": binding.site_type,
            "profile_id": binding.profile_id,
            "profile_name": binding.profile_name,
            "profile_type": binding.profile_type,
            "is_active": binding.is_active,
            "agent_id": binding.agent_id,
            "agent_hostname": binding.agent_hostname,
            "has_command_overrides": has_overrides,
            "command_overrides": cmd_override.map(|o| json!({
                "check_cmd": o.check_cmd_override,
                "start_cmd": o.start_cmd_override,
                "stop_cmd": o.stop_cmd_override,
                "rebuild_cmd": o.rebuild_cmd_override,
                "env_vars": o.env_vars_override,
            })),
        });

        component_map
            .entry(binding.component_id)
            .or_default()
            .push(site_binding);
    }

    // Convert to array format
    let component_bindings: Vec<Value> = bindings
        .iter()
        .map(|b| b.component_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .filter_map(|comp_id| {
            let comp_name = bindings
                .iter()
                .find(|b| b.component_id == comp_id)?
                .component_name
                .clone();
            let sites = component_map.remove(&comp_id)?;
            Some(json!({
                "component_id": comp_id,
                "component_name": comp_name,
                "site_bindings": sites,
            }))
        })
        .collect();

    Ok(Json(json!({
        "primary_site": primary_site.map(|s| json!({
            "id": s.site_id,
            "name": s.site_name,
            "code": s.site_code,
            "site_type": s.site_type,
        })),
        "component_bindings": component_bindings,
    })))
}

// Helper functions removed — all SQL queries moved to repository::apps
