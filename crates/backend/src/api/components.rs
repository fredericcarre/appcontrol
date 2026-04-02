use crate::db::DbUuid;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::{ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct CreateComponentRequest {
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    /// Host as entered by user: FQDN or IP address.
    /// Resolved to agent_id by matching agents.hostname or agents.ip_addresses.
    pub host: Option<String>,
    /// Alias for `host` — backward compatibility with old API callers.
    #[serde(default)]
    pub hostname: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub post_start_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    pub rebuild_infra_cmd: Option<String>,
    pub rebuild_agent_id: Option<Uuid>,
    pub rebuild_protected: Option<bool>,
    pub check_interval_seconds: Option<i32>,
    pub start_timeout_seconds: Option<i32>,
    pub stop_timeout_seconds: Option<i32>,
    pub is_optional: Option<bool>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub env_vars: Option<Value>,
    pub tags: Option<Value>,
    /// Cluster size: number of nodes (NULL = not a cluster, >= 2 for cluster)
    pub cluster_size: Option<i32>,
    /// List of node hostnames/IPs in the cluster
    pub cluster_nodes: Option<Vec<String>>,
    /// Reference to another application (for app-type synthetic components)
    pub referenced_app_id: Option<Uuid>,
}

impl CreateComponentRequest {
    /// Returns the effective host value (prefers `host`, falls back to `hostname` alias).
    pub fn effective_host(&self) -> Option<&str> {
        self.host.as_deref().or(self.hostname.as_deref())
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct UpdateComponentRequest {
    pub name: Option<String>,
    pub component_type: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    /// Host as entered by user: FQDN or IP address.
    pub host: Option<String>,
    /// Alias for `host` — backward compatibility.
    #[serde(default)]
    pub hostname: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub post_start_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    pub rebuild_infra_cmd: Option<String>,
    pub rebuild_agent_id: Option<Uuid>,
    pub rebuild_protected: Option<bool>,
    pub check_interval_seconds: Option<i32>,
    pub start_timeout_seconds: Option<i32>,
    pub stop_timeout_seconds: Option<i32>,
    pub is_optional: Option<bool>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub env_vars: Option<Value>,
    pub tags: Option<Value>,
    /// Cluster size: number of nodes (NULL = not a cluster, >= 2 for cluster)
    pub cluster_size: Option<i32>,
    /// List of node hostnames/IPs in the cluster
    pub cluster_nodes: Option<Vec<String>>,
    /// Reference to another application (for app-type synthetic components)
    pub referenced_app_id: Option<Uuid>,
}

impl UpdateComponentRequest {
    /// Returns the effective host value (prefers `host`, falls back to `hostname` alias).
    pub fn effective_host(&self) -> Option<&str> {
        self.host.as_deref().or(self.hostname.as_deref())
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComponentRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<DbUuid>,
    pub host: Option<String>,
    pub agent_id: Option<DbUuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Value>,
    pub referenced_app_id: Option<DbUuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDependencyRequest {
    pub from_component_id: DbUuid,
    pub to_component_id: DbUuid,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub from_component_id: DbUuid,
    pub to_component_id: DbUuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_components(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let components = state.component_repo.list_components(app_id).await?;

    let result: Vec<Value> = components
        .into_iter()
        .map(|c| component_to_json(&c))
        .collect();

    Ok(Json(json!({ "components": result })))
}

fn component_to_json(c: &crate::repository::components::Component) -> Value {
    json!({
        "id": c.id,
        "application_id": c.application_id,
        "name": c.name,
        "component_type": c.component_type,
        "display_name": c.display_name,
        "description": c.description,
        "icon": c.icon,
        "group_id": c.group_id,
        "host": c.host,
        "agent_id": c.agent_id,
        "check_cmd": c.check_cmd,
        "start_cmd": c.start_cmd,
        "stop_cmd": c.stop_cmd,
        "check_interval_seconds": c.check_interval_seconds,
        "start_timeout_seconds": c.start_timeout_seconds,
        "stop_timeout_seconds": c.stop_timeout_seconds,
        "is_optional": c.is_optional,
        "position_x": c.position_x,
        "position_y": c.position_y,
        "cluster_size": c.cluster_size,
        "cluster_nodes": c.cluster_nodes,
        "referenced_app_id": c.referenced_app_id,
        "created_at": c.created_at,
        "updated_at": c.updated_at,
    })
}

pub async fn get_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let component = state
        .component_repo
        .get_component(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    Ok(Json(component_to_json(&component)))
}

pub async fn create_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateComponentRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    let comp_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_component",
        "component",
        comp_id,
        json!({"name": body.name}),
    )
    .await?;

    // Use effective_host() to support both "host" and "hostname" JSON fields
    let effective_host = body.effective_host().map(|s| s.to_string());

    // Resolve host → agent_id if host is provided but agent_id is not.
    // No multicast: one host matches exactly one agent.
    let resolved_agent_id = if body.agent_id.is_some() {
        body.agent_id
    } else if let Some(ref host) = effective_host {
        resolve_host_to_agent(&state.db, host).await
    } else {
        None
    };

    // Convert cluster_nodes Vec<String> to JSONB Value
    let cluster_nodes_json = body
        .cluster_nodes
        .as_ref()
        .map(|nodes| serde_json::to_value(nodes).unwrap_or(json!([])));

    use crate::repository::components::CreateComponent;

    let component = state
        .component_repo
        .create_component(CreateComponent {
            id: comp_id,
            application_id: app_id,
            name: body.name.clone(),
            component_type: body.component_type.clone(),
            display_name: body.display_name.clone(),
            description: body.description.clone(),
            icon: body.icon.clone().unwrap_or_else(|| "box".to_string()),
            group_id: body.group_id,
            host: effective_host.clone(),
            agent_id: resolved_agent_id,
            check_cmd: body.check_cmd.clone(),
            start_cmd: body.start_cmd.clone(),
            stop_cmd: body.stop_cmd.clone(),
            check_interval_seconds: body.check_interval_seconds.unwrap_or(30),
            start_timeout_seconds: body.start_timeout_seconds.unwrap_or(120),
            stop_timeout_seconds: body.stop_timeout_seconds.unwrap_or(60),
            is_optional: body.is_optional.unwrap_or(false),
            position_x: body.position_x.unwrap_or(0.0),
            position_y: body.position_y.unwrap_or(0.0),
            env_vars: body.env_vars.clone().unwrap_or(json!({})),
            tags: body.tags.clone().unwrap_or(json!([])),
            cluster_size: body.cluster_size,
            cluster_nodes: cluster_nodes_json.clone(),
            referenced_app_id: body.referenced_app_id,
        })
        .await?;

    // Push config to affected agent so it starts health checks immediately
    let agent_ids = resolved_agent_id.map(|id| vec![id]);
    crate::websocket::push_config_to_affected_agents(
        &state,
        Some(app_id),
        None,
        agent_ids.as_deref(),
    )
    .await;

    Ok((StatusCode::CREATED, Json(component_to_json(&component))))
}

pub async fn update_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateComponentRequest>,
) -> Result<Json<Value>, ApiError> {
    // Get current component to check app permission
    let current_app_id = state
        .component_repo
        .get_component_app_id(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, current_app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "update_component",
        "component",
        id,
        json!({}),
    )
    .await?;

    // Snapshot before state for config_versions
    let before_snapshot = state
        .component_repo
        .get_component(id, *user.organization_id)
        .await?
        .map(|c| component_to_json(&c));

    // Use effective_host() to support both "host" and "hostname" JSON fields
    let effective_host = body.effective_host().map(|s| s.to_string());

    // If host changed, re-resolve agent_id
    let resolved_agent_id = if body.agent_id.is_some() {
        body.agent_id
    } else if let Some(ref host) = effective_host {
        resolve_host_to_agent(&state.db, host).await
    } else {
        None
    };

    // Convert cluster_nodes Vec<String> to JSONB Value
    let cluster_nodes_json = body
        .cluster_nodes
        .as_ref()
        .map(|nodes| serde_json::to_value(nodes).unwrap_or(json!([])));

    use crate::repository::components::UpdateComponent;

    let component = state
        .component_repo
        .update_component(
            id,
            &UpdateComponent {
                name: body.name.clone(),
                component_type: body.component_type.clone(),
                display_name: body.display_name.clone(),
                description: body.description.clone(),
                icon: body.icon.clone(),
                group_id: body.group_id,
                host: effective_host.clone(),
                agent_id: resolved_agent_id,
                check_cmd: body.check_cmd.clone(),
                start_cmd: body.start_cmd.clone(),
                stop_cmd: body.stop_cmd.clone(),
                check_interval_seconds: body.check_interval_seconds,
                start_timeout_seconds: body.start_timeout_seconds,
                stop_timeout_seconds: body.stop_timeout_seconds,
                is_optional: body.is_optional,
                position_x: body.position_x,
                position_y: body.position_y,
                cluster_size: body.cluster_size,
                cluster_nodes: cluster_nodes_json.clone(),
                referenced_app_id: body.referenced_app_id,
            },
        )
        .await?
        .ok_or_not_found()?;

    // Record config_versions snapshot
    {
        let after_snapshot = component_to_json(&component);
        let before_json = before_snapshot
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string());
        let after_json = after_snapshot.to_string();

        state.component_repo.insert_config_version(
            "component", id, *user.user_id, &before_json, &after_json,
        ).await?;
    }

    // Push config to affected agent so it picks up the changes
    let agent_ids: Option<Vec<uuid::Uuid>> = component.agent_id.map(|id| vec![id]);
    crate::websocket::push_config_to_affected_agents(
        &state,
        Some(current_app_id),
        None,
        agent_ids.as_deref(),
    )
    .await;

    Ok(Json(component_to_json(&component)))
}

pub async fn delete_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Get app_id and agent_id before deleting
    let (app_id, agent_id) = state
        .component_repo
        .get_component_app_and_agent(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_component",
        "component",
        id,
        json!({}),
    )
    .await?;

    state.component_repo.delete_component(id).await?;

    // Push config to affected agent so it stops checking this component
    let agent_ids = agent_id.map(|id| vec![id]);
    crate::websocket::push_config_to_affected_agents(
        &state,
        Some(app_id),
        None,
        agent_ids.as_deref(),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ── Position Update (for Map Designer) ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdatePositionRequest {
    pub x: f32,
    pub y: f32,
}

/// PATCH /api/v1/components/:id/position
/// Update component position on the map canvas (for drag & drop editing).
/// Requires Edit permission (not just Operate) since it modifies configuration.
pub async fn update_position(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePositionRequest>,
) -> Result<StatusCode, ApiError> {
    // Get app_id for permission check
    let app_id = state
        .component_repo
        .get_component_app_id(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Note: We don't log position updates to avoid spamming action_log during drag operations.
    // Position is not a critical operational parameter.

    state
        .component_repo
        .update_position(id, body.x, body.y)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/v1/components/batch-positions
/// Update multiple component positions at once (for batch save after editing session).
#[derive(Debug, Deserialize)]
pub struct BatchPositionUpdate {
    pub positions: Vec<ComponentPosition>,
}

#[derive(Debug, Deserialize)]
pub struct ComponentPosition {
    pub id: DbUuid,
    pub x: f32,
    pub y: f32,
}

pub async fn update_positions_batch(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<BatchPositionUpdate>,
) -> Result<StatusCode, ApiError> {
    if body.positions.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Get app_id from first component (all should be in same app)
    let first_id = *body.positions[0].id;
    let app_id = state.component_repo.get_component_app_id(first_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Update all positions in a transaction
    let positions: Vec<(Uuid, f32, f32)> = body.positions.iter().map(|p| (*p.id, p.x, p.y)).collect();
    state.component_repo.batch_update_positions(app_id, &positions).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Get component info including referenced_app_id
    let (comp_app_id, comp_ref_app_id) = state.component_repo.get_component_refs(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        comp_app_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "start_component",
        "component",
        id,
        json!({"referenced_app_id": comp_ref_app_id}),
    )
    .await?;

    // For application-type components, start the referenced app instead
    if let Some(ref_app_id) = comp_ref_app_id {
        // Check permission on referenced app as well
        let ref_perm =
            effective_permission(&state.db, user.user_id, ref_app_id, user.is_admin()).await;
        if ref_perm < PermissionLevel::Operate {
            return Err(ApiError::Forbidden);
        }

        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::core::sequencer::execute_start(&state_clone, ref_app_id).await {
                tracing::error!("Start referenced app {} failed: {}", ref_app_id, e);
            }
        });

        return Ok(Json(
            json!({ "status": "starting", "propagated_to_app": ref_app_id }),
        ));
    }

    // Start the component: transition to Starting, dispatch start_cmd to agent
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::start_single_component(&state_clone, id).await {
            tracing::error!("Start component failed for {}: {}", id, e);
        }
    });

    Ok(Json(json!({ "status": "starting" })))
}

pub async fn stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Get component info including referenced_app_id
    let (comp_app_id, comp_ref_app_id) = state.component_repo.get_component_refs(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        comp_app_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "stop_component",
        "component",
        id,
        json!({"referenced_app_id": comp_ref_app_id}),
    )
    .await?;

    // For application-type components, stop the referenced app instead
    if let Some(ref_app_id) = comp_ref_app_id {
        // Check permission on referenced app as well
        let ref_perm =
            effective_permission(&state.db, user.user_id, ref_app_id, user.is_admin()).await;
        if ref_perm < PermissionLevel::Operate {
            return Err(ApiError::Forbidden);
        }

        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::core::sequencer::execute_stop(&state_clone, ref_app_id).await {
                tracing::error!("Stop referenced app {} failed: {}", ref_app_id, e);
            }
        });

        return Ok(Json(
            json!({ "status": "stopping", "propagated_to_app": ref_app_id }),
        ));
    }

    // Stop the component and all its dependents in reverse DAG order
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::stop_with_dependents(&state_clone, id).await {
            tracing::error!("Stop component with dependents failed for {}: {}", id, e);
        }
    });

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn force_stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "force_stop_component",
        "component",
        id,
        json!({"reason": "production_incident", "bypass_dependencies": true}),
    )
    .await?;

    // Force stop: bypass FSM and DAG dependencies
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::force_stop_single_component(&state_clone, id).await
        {
            tracing::error!("Force stop failed for component {}: {}", id, e);
        }
    });

    Ok(Json(
        json!({ "status": "force_stopping", "message": "Force stop initiated (bypassing dependencies)" }),
    ))
}

pub async fn start_with_deps(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Build DAG and find all upstream dependencies of this component
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut subset = dag.find_all_dependencies(id);
    subset.insert(id); // Include the target component itself

    log_action(
        &state.db,
        user.user_id,
        "start_with_deps",
        "component",
        id,
        json!({"component_id": id, "dependency_count": subset.len() - 1}),
    )
    .await?;

    // Acquire operation lock
    let guard = state
        .operation_lock
        .try_lock(app_id, "start_with_deps", *user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let total_components = subset.len();
    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(e) =
            crate::core::sequencer::execute_start_subset(&state_clone, app_id, &subset).await
        {
            tracing::error!("Failed to start component {} with deps: {}", id, e);
        }
    });

    Ok(Json(json!({
        "status": "starting_with_deps",
        "component_id": id,
        "total_components": total_components,
    })))
}

/// Repair/restart a component by:
/// 1. Stopping all dependents (components that depend on this one)
/// 2. Stopping this component
/// 3. Starting this component
/// 4. Starting all dependents (in DAG order)
pub async fn restart_with_dependents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Build DAG and find all downstream dependents of this component
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let dependents = dag.find_all_dependents(id);
    let dependent_count = dependents.len();

    log_action(
        &state.db,
        user.user_id,
        "restart_with_dependents",
        "component",
        id,
        json!({"component_id": id, "dependent_count": dependent_count}),
    )
    .await?;

    // Acquire operation lock
    let guard = state
        .operation_lock
        .try_lock(app_id, "restart_with_dependents", *user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    // Build set of all components to restart (target + dependents)
    let mut all_components = dependents.clone();
    all_components.insert(id);
    let total_components = all_components.len();

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard;

        // Phase 1: Stop the target component and all its dependents (in reverse DAG order)
        tracing::info!(
            component_id = %id,
            dependent_count = dependent_count,
            "Phase 1: Stopping component and dependents"
        );
        if let Err(e) = crate::core::sequencer::stop_with_dependents(&state_clone, id).await {
            tracing::error!("Failed to stop component {} with dependents: {}", id, e);
            return;
        }

        // Phase 2: Start all components (target + dependents) in DAG order
        tracing::info!(
            component_id = %id,
            total_components = total_components,
            "Phase 2: Starting component and dependents"
        );
        if let Err(e) =
            crate::core::sequencer::execute_start_subset(&state_clone, app_id, &all_components)
                .await
        {
            tracing::error!("Failed to start component {} with dependents: {}", id, e);
            return;
        }

        tracing::info!(component_id = %id, "Restart with dependents completed");
    });

    Ok(Json(json!({
        "status": "restarting_with_dependents",
        "component_id": id,
        "dependent_count": dependent_count,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandBody {
    #[serde(default)]
    pub parameters: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub confirmed: Option<bool>,
}

pub async fn execute_command(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((id, cmd)): Path<(Uuid, String)>,
    body: Option<Json<ExecuteCommandBody>>,
) -> Result<Json<Value>, ApiError> {
    // Fetch component with all command columns
    #[derive(sqlx::FromRow)]
    struct ComponentCmd {
        application_id: DbUuid,
        agent_id: Option<DbUuid>,
        check_cmd: Option<String>,
        start_cmd: Option<String>,
        stop_cmd: Option<String>,
        integrity_check_cmd: Option<String>,
        infra_check_cmd: Option<String>,
    }

    let comp = state.component_repo.get_component_commands(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        comp.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Check if this is a built-in command
    let builtin_cmd = match cmd.as_str() {
        "check" => comp.check_cmd.clone(),
        "start" => comp.start_cmd.clone(),
        "stop" => comp.stop_cmd.clone(),
        // restart_cmd is not a standard column; handled as custom command
        "integrity_check" => comp.integrity_check_cmd.clone(),
        "infra_check" => comp.infra_check_cmd.clone(),
        _ => None,
    };

    // If it's a built-in command, use it directly; otherwise look up in component_commands
    let (command_id, command_template): (Option<Uuid>, String) = if let Some(builtin) = builtin_cmd
    {
        (None, builtin)
    } else {
        // Look up the command definition from component_commands
        let custom_cmd = state.component_repo.get_custom_command(id, &cmd)
            .await?
            .ok_or_not_found()?;

        // Check confirmation requirement
        let requires_confirmation = custom_cmd.requires_confirmation;
        if requires_confirmation {
            let confirmed = body.as_ref().and_then(|b| b.confirmed).unwrap_or(false);
            if !confirmed {
                return Err(ApiError::Validation(
                    "This command requires confirmation. Set 'confirmed: true' to execute."
                        .to_string(),
                ));
            }
        }

        (Some(custom_cmd.id), custom_cmd.command)
    };

    let agent_id = comp.agent_id;

    // Load parameter definitions and validate/interpolate (only for custom commands)
    let params = if let Some(cid) = command_id {
        {
            let cp = state.component_repo.list_command_params(cid).await?;
            cp.into_iter().map(|p| crate::api::command_params::InputParamRow {
                id: crate::db::DbUuid::from(p.id),
                command_id: crate::db::DbUuid::from(p.command_id),
                name: p.name, description: p.description,
                default_value: p.default_value, validation_regex: p.validation_regex,
                required: p.required, display_order: p.display_order,
                param_type: p.param_type, enum_values: p.enum_values, created_at: p.created_at,
            }).collect::<Vec<_>>()
        }
    } else {
        vec![]
    };

    let param_values = body
        .as_ref()
        .and_then(|b| b.parameters.clone())
        .unwrap_or_default();

    let final_command = if params.is_empty() {
        command_template.clone()
    } else {
        crate::api::command_params::validate_and_interpolate_params(
            &command_template,
            &params,
            &param_values,
        )
        .map_err(|errors| ApiError::Validation(errors.join("; ")))?
    };

    log_action(
        &state.db,
        user.user_id,
        "execute_command",
        "component",
        id,
        json!({"command": cmd, "parameters": param_values}),
    )
    .await?;

    let agent_id = agent_id.ok_or(ApiError::Conflict(
        "No agent assigned to this component".to_string(),
    ))?;

    let request_id = Uuid::new_v4();

    // Record dispatch in command_executions for audit trail
    let command_type_label = if command_id.is_none() { &cmd } else { "custom" };
    if let Err(e) = state.component_repo.insert_command_execution(
        request_id, id, agent_id, command_type_label, *user.user_id, &final_command,
    ).await
    {
        tracing::warn!(request_id = %request_id, "Failed to record command dispatch: {}", e);
    }

    // Dispatch to agent via WebSocket hub (sync mode for custom commands)
    let message = appcontrol_common::BackendMessage::ExecuteCommand {
        request_id,
        component_id: id,
        command: final_command.clone(),
        timeout_seconds: 300,
        exec_mode: "sync".to_string(),
    };

    let dispatched = state.ws_hub.send_to_agent(agent_id, message);
    if !dispatched {
        return Err(ApiError::Conflict(
            "Agent is not reachable — no gateway route available".to_string(),
        ));
    }

    metrics::counter!("commands_executed_total", "command" => cmd.clone()).increment(1);

    tracing::info!(
        request_id = %request_id,
        component_id = %id,
        agent_id = %agent_id,
        "Custom command dispatched to agent (sync)"
    );

    Ok(Json(json!({
        "request_id": request_id,
        "command": final_command,
        "status": "dispatched",
        "component_id": id,
        "agent_id": agent_id,
    })))
}

pub async fn list_dependencies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let deps_data = state.component_repo.list_dependencies(app_id).await?;
    let deps: Vec<serde_json::Value> = deps_data.into_iter().map(|d| serde_json::json!({"id": d.id, "application_id": d.application_id, "from_component_id": d.from_component_id, "to_component_id": d.to_component_id, "created_at": d.created_at})).collect();

    Ok(Json(json!({ "dependencies": deps })))
}

pub async fn create_dependency(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateDependencyRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Validate both components belong to this application
    let from_app_id = state.component_repo.get_component_app_id(*body.from_component_id)
        .await?
        .ok_or(ApiError::Validation("from_component_id not found".to_string()))?;

    let to_app_id = state.component_repo.get_component_app_id(*body.to_component_id)
        .await?
        .ok_or(ApiError::Validation("to_component_id not found".to_string()))?;

    if from_app_id != app_id || to_app_id != app_id {
        return Err(ApiError::Validation(
            "Both components must belong to this application".to_string(),
        ));
    }

    // Check for cycles before inserting
    if crate::core::dag::validate_no_cycle(
        &state.db,
        app_id,
        body.from_component_id,
        body.to_component_id,
    )
    .await
    .is_err()
    {
        return Err(ApiError::Conflict(
            "Adding this dependency would create a cycle".to_string(),
        ));
    }

    let dep_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_dependency",
        "dependency",
        dep_id,
        json!({"from": body.from_component_id, "to": body.to_component_id}),
    )
    .await?;

    let dep_domain = state.component_repo.create_dependency(app_id, *body.from_component_id, *body.to_component_id).await?;

    Ok((StatusCode::CREATED, Json(json!({"id": dep_domain.id, "application_id": dep_domain.application_id, "from_component_id": dep_domain.from_component_id, "to_component_id": dep_domain.to_component_id, "created_at": dep_domain.created_at}))))
}

pub async fn delete_dependency(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let app_id = state.component_repo.get_dependency_app_id(id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_dependency",
        "dependency",
        id,
        json!({}),
    )
    .await?;

    state.component_repo.delete_dependency(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Custom Commands listing
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CustomCommandRow {
    pub id: DbUuid,
    pub component_id: DbUuid,
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub requires_confirmation: bool,
    pub min_permission_level: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List custom commands for a component.
pub async fn list_custom_commands(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let commands = state.component_repo.list_custom_commands_raw(component_id).await?;

    Ok(Json(json!({ "commands": commands })))
}

// ---------------------------------------------------------------------------
// Command Execution History
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CommandExecutionRow {
    pub id: DbUuid,
    pub request_id: DbUuid,
    pub component_id: DbUuid,
    pub agent_id: Option<DbUuid>,
    pub command_type: String,
    pub exit_code: Option<i16>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i32>,
    pub status: String,
    pub dispatched_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ExecutionHistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

/// List command execution history for a component.
pub async fn list_command_executions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ExecutionHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let executions = state.component_repo.list_command_executions(
        component_id, query.status.as_deref(), limit, offset,
    ).await?;

    Ok(Json(json!({ "executions": executions })))
}

// ---------------------------------------------------------------------------
// Component State Transitions History
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StateTransitionRow {
    pub id: DbUuid,
    pub component_id: DbUuid,
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List state transition history for a component.
pub async fn list_state_transitions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ExecutionHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let transitions = state.component_repo.list_state_transitions(component_id, limit, offset).await?;

    Ok(Json(json!({ "transitions": transitions })))
}

// ---------------------------------------------------------------------------
// Check Events History
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CheckEventRow {
    pub id: i64,
    pub component_id: DbUuid,
    pub check_type: String,
    pub exit_code: i16,
    pub stdout: Option<String>,
    pub duration_ms: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List recent check events (health/integrity/infra checks) for a component.
pub async fn list_check_events(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ExecutionHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let events = state.component_repo.list_check_events(component_id, limit, offset).await?;

    Ok(Json(json!({ "events": events })))
}

// ---------------------------------------------------------------------------
// Host → Agent resolution
// ---------------------------------------------------------------------------

/// Resolve a user-provided host (FQDN or IP) to an agent_id.
///
/// Match order:
/// 1. Exact hostname match in agents table
/// 2. IP address match in agents.ip_addresses JSONB array
///
/// No multicast: returns the first match only. If multiple agents
/// share an IP, the first one (by created_at) wins.
pub async fn resolve_host_to_agent(pool: &crate::db::DbPool, host: &str) -> Option<Uuid> {
    // Delegate to component repository for host resolution
    let repo = crate::repository::components::create_component_repository(pool.clone());
    repo.resolve_host_to_agent(host).await.ok().flatten()
}

/// Called when an agent registers: resolve all components that reference
/// this agent's hostname or IPs via the `host` field but have no agent_id yet.
///
/// This is the "late binding" path: user creates component with host="srv-oracle.prod",
/// agent registers later with hostname="srv-oracle.prod" → agent_id is set automatically.
pub async fn resolve_components_for_agent(
    pool: &crate::db::DbPool,
    agent_id: impl Into<Uuid> + Send,
    hostname: &str,
    ip_addresses: &[String],
) {
    let agent_id: Uuid = agent_id.into();
    let repo = crate::repository::components::create_component_repository(pool.clone());
    match repo.auto_bind_agent(agent_id, hostname, ip_addresses).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!(agent_id = %agent_id, hostname = %hostname, count = count, "Auto-bound components to agent");
            }
        }
        Err(e) => {
            tracing::warn!(agent_id = %agent_id, "Failed to auto-bind components: {}", e);
        }
    }
}

/// Get latest metrics for a component.
pub async fn get_component_metrics(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id = state.component_repo.get_component_app_id(component_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Get latest check event with metrics
    let latest = crate::repository::queries::get_latest_check_metrics(&state.db, component_id).await?;

    match latest {
        Some((metrics, exit_code, at)) => Ok(Json(json!({
            "component_id": component_id,
            "metrics": metrics,
            "exit_code": exit_code,
            "at": at,
        }))),
        None => Ok(Json(json!({
            "component_id": component_id,
            "metrics": null,
            "message": "No metrics available"
        }))),
    }
}

/// Get metrics history for a component (for charts).
pub async fn get_component_metrics_history(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Get last 100 check events with metrics
    let history = crate::repository::queries::get_metrics_history(&state.db, component_id, 100).await?;

    let points: Vec<Value> = history
        .into_iter()
        .map(|(metrics, exit_code, at)| {
            json!({
                "metrics": metrics,
                "exit_code": exit_code,
                "at": at,
            })
        })
        .collect();

    Ok(Json(json!({
        "component_id": component_id,
        "history": points,
    })))
}

// ============================================================================
// Site Overrides (Failover Configuration)
// ============================================================================

/// GET /api/v1/components/:id/site-overrides
///
/// Returns all site overrides for a component, with site and agent details.
pub async fn list_site_overrides(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // First get the app_id to check permissions
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    #[derive(Debug, sqlx::FromRow)]
    struct SiteOverrideRow {
        id: DbUuid,
        component_id: DbUuid,
        site_id: DbUuid,
        agent_id_override: Option<DbUuid>,
        check_cmd_override: Option<String>,
        start_cmd_override: Option<String>,
        stop_cmd_override: Option<String>,
        rebuild_cmd_override: Option<String>,
        env_vars_override: Option<Value>,
        created_at: chrono::DateTime<chrono::Utc>,
        // Joined
        site_name: String,
        site_code: String,
        site_type: String,
        agent_hostname: Option<String>,
    }

    let overrides = crate::repository::queries::list_site_overrides(&state.db, component_id).await?;

    let data: Vec<Value> = overrides
        .into_iter()
        .map(|o| {
            json!({
                "id": o.id,
                "component_id": o.component_id,
                "site_id": o.site_id,
                "site_name": o.site_name,
                "site_code": o.site_code,
                "site_type": o.site_type,
                "agent_id_override": o.agent_id_override,
                "agent_hostname": o.agent_hostname,
                "check_cmd_override": o.check_cmd_override,
                "start_cmd_override": o.start_cmd_override,
                "stop_cmd_override": o.stop_cmd_override,
                "rebuild_cmd_override": o.rebuild_cmd_override,
                "env_vars_override": o.env_vars_override,
                "created_at": o.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "overrides": data })))
}

#[derive(Debug, Deserialize)]
pub struct UpsertSiteOverrideRequest {
    pub agent_id_override: Option<Uuid>,
    pub check_cmd_override: Option<String>,
    pub start_cmd_override: Option<String>,
    pub stop_cmd_override: Option<String>,
    pub rebuild_cmd_override: Option<String>,
    pub env_vars_override: Option<Value>,
}

/// PUT /api/v1/components/:id/site-overrides/:site_id
///
/// Create or update a site override for a component.
pub async fn upsert_site_override(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, site_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpsertSiteOverrideRequest>,
) -> Result<Json<Value>, ApiError> {
    // First get the app_id to check permissions
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Verify site exists
    if !crate::repository::queries::site_exists(&state.db, site_id).await? {
        return Err(ApiError::NotFound);
    }

    // Log before execute
    log_action(
        &state.db,
        user.user_id,
        "upsert_site_override",
        "component",
        component_id,
        json!({
            "site_id": site_id,
            "agent_id_override": req.agent_id_override,
        }),
    )
    .await
    .ok();

    // Upsert via repository
    let id = crate::repository::queries::upsert_site_override(
        &state.db, component_id, site_id,
        req.check_cmd_override.as_deref(), req.start_cmd_override.as_deref(),
        req.stop_cmd_override.as_deref(), req.rebuild_cmd_override.as_deref(),
        req.env_vars_override.as_ref(), req.agent_id_override,
    ).await?;

    Ok(Json(json!({
        "id": id,
        "component_id": component_id,
        "site_id": site_id,
    })))
}

/// DELETE /api/v1/components/:id/site-overrides/:site_id
///
/// Remove a site override for a component.
pub async fn delete_site_override(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, site_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    // First get the app_id to check permissions
    let app_id =
        state.component_repo.get_component_app_id(component_id)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    log_action(
        &state.db,
        user.user_id,
        "delete_site_override",
        "component",
        component_id,
        json!({ "site_id": site_id }),
    )
    .await
    .ok();

    crate::repository::queries::delete_site_override(&state.db, component_id, site_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
