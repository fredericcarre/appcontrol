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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComponentRow {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    pub component_type: String,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
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
    pub id: Uuid,
    pub from_component_id: Uuid,
    pub to_component_id: Uuid,
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

    // Fetch apps with component state counts in a single query
    #[derive(Debug, sqlx::FromRow)]
    struct AppWithCounts {
        id: Uuid,
        name: String,
        description: Option<String>,
        organization_id: Uuid,
        site_id: Uuid,
        tags: Value,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
        component_count: Option<i64>,
        running_count: Option<i64>,
        stopped_count: Option<i64>,
        failed_count: Option<i64>,
    }

    let apps = sqlx::query_as::<_, AppWithCounts>(
        r#"
        SELECT
            a.id, a.name, a.description, a.organization_id, a.site_id, a.tags,
            a.created_at, a.updated_at,
            COUNT(c.id) as component_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'RUNNING') as running_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPED') as stopped_count,
            COUNT(c.id) FILTER (WHERE c.current_state IN ('FAILED', 'UNREACHABLE')) as failed_count
        FROM applications a
        LEFT JOIN components c ON c.application_id = a.id
        WHERE a.organization_id = $1
          AND ($2::text IS NULL OR a.name ILIKE '%' || $2 || '%')
          AND ($3::uuid IS NULL OR a.site_id = $3)
        GROUP BY a.id, a.name, a.description, a.organization_id, a.site_id, a.tags, a.created_at, a.updated_at
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

    // Transform to response with computed status
    let apps_with_status: Vec<_> = apps
        .into_iter()
        .map(|a| {
            let component_count = a.component_count.unwrap_or(0);
            let running_count = a.running_count.unwrap_or(0);
            let stopped_count = a.stopped_count.unwrap_or(0);
            let failed_count = a.failed_count.unwrap_or(0);

            let (global_state, weather) = compute_app_status(
                running_count,
                stopped_count,
                failed_count,
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
                "stopped_count": stopped_count,
                "failed_count": failed_count,
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

    let app = sqlx::query_as::<_, AppRow>(
        "SELECT id, name, description, organization_id, site_id, tags, created_at, updated_at \
         FROM applications WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    // Fetch components with agent info
    #[derive(Debug, sqlx::FromRow)]
    struct ComponentWithAgent {
        id: Uuid,
        application_id: Uuid,
        name: String,
        display_name: Option<String>,
        description: Option<String>,
        icon: Option<String>,
        group_id: Option<Uuid>,
        component_type: String,
        host: Option<String>,
        agent_id: Option<Uuid>,
        check_cmd: Option<String>,
        start_cmd: Option<String>,
        stop_cmd: Option<String>,
        check_interval_seconds: i32,
        start_timeout_seconds: i32,
        stop_timeout_seconds: i32,
        is_optional: bool,
        current_state: String,
        position_x: Option<f32>,
        position_y: Option<f32>,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
        // Agent info
        agent_hostname: Option<String>,
        gateway_id: Option<Uuid>,
        gateway_name: Option<String>,
    }

    let components = sqlx::query_as::<_, ComponentWithAgent>(
        r#"SELECT c.id, c.application_id, c.name, c.display_name, c.description, c.icon, c.group_id,
                  c.component_type, c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd,
                  c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                  c.is_optional, c.current_state, c.position_x, c.position_y, c.created_at, c.updated_at,
                  a.hostname as agent_hostname, a.gateway_id, g.name as gateway_name
           FROM components c
           LEFT JOIN agents a ON c.agent_id = a.id
           LEFT JOIN gateways g ON a.gateway_id = g.id
           WHERE c.application_id = $1 ORDER BY c.name"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    // Fetch dependencies
    let dependencies = sqlx::query_as::<_, DependencyRow>(
        "SELECT id, from_component_id, to_component_id FROM dependencies \
         WHERE from_component_id IN (SELECT id FROM components WHERE application_id = $1)",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    // Get live connection status from WebSocket hub
    let connected_agents: std::collections::HashSet<Uuid> =
        state.ws_hub.connected_agent_ids().into_iter().collect();
    let connected_gateways: std::collections::HashSet<Uuid> =
        state.ws_hub.connected_gateway_ids().into_iter().collect();

    // Enrich components with connectivity status
    let components_json: Vec<Value> = components
        .into_iter()
        .map(|c| {
            let agent_connected = c.agent_id.map(|aid| connected_agents.contains(&aid)).unwrap_or(false);
            let gateway_connected = c.gateway_id.map(|gid| connected_gateways.contains(&gid)).unwrap_or(false);

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
                "current_state": c.current_state,
                "position_x": c.position_x,
                "position_y": c.position_y,
                "created_at": c.created_at,
                "updated_at": c.updated_at,
                // Connectivity info
                "agent_hostname": c.agent_hostname,
                "agent_connected": agent_connected,
                "gateway_id": c.gateway_id,
                "gateway_name": c.gateway_name,
                "gateway_connected": gateway_connected,
                "connectivity_status": connectivity_status,
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

/// Cancel any running operation on an application and release its lock.
/// This is a last-resort operation for stuck operations.
pub async fn cancel_operation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "cancel_operation",
        "application",
        id,
        json!({}),
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
            "Operation cancelled and lock force-released"
        );
        Ok(Json(json!({ "status": "cancelled", "lock_released": true })))
    } else {
        Ok(Json(json!({ "status": "no_operation", "lock_released": false })))
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
    let is_suspended: bool =
        sqlx::query_scalar("SELECT is_suspended FROM applications WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .unwrap_or(false);

    if is_suspended {
        return Err(ApiError::Conflict("Application is already suspended".to_string()));
    }

    // Suspend the application
    sqlx::query(
        "UPDATE applications
         SET is_suspended = true, suspended_at = now(), suspended_by = $2, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(user.user_id)
    .execute(&state.db)
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
    let name: String = sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

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
    let is_suspended: bool =
        sqlx::query_scalar("SELECT is_suspended FROM applications WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .unwrap_or(false);

    if !is_suspended {
        return Err(ApiError::Conflict("Application is not suspended".to_string()));
    }

    // Resume the application
    sqlx::query(
        "UPDATE applications
         SET is_suspended = false, suspended_at = NULL, suspended_by = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
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
    let name: String = sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

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
