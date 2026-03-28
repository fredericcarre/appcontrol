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
use crate::db::DbPool;
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
        starting_count: Option<i64>,
        stopping_count: Option<i64>,
        stopped_count: Option<i64>,
        failed_count: Option<i64>,
        unreachable_count: Option<i64>,
    }

    let apps = sqlx::query_as::<_, AppWithCounts>(
        r#"
        SELECT
            a.id, a.name, a.description, a.organization_id, a.site_id, a.tags,
            a.created_at, a.updated_at,
            COUNT(c.id) as component_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'RUNNING') as running_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STARTING') as starting_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPING') as stopping_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPED') as stopped_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'FAILED') as failed_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'UNREACHABLE') as unreachable_count
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
            let starting_count = a.starting_count.unwrap_or(0);
            let stopping_count = a.stopping_count.unwrap_or(0);
            let stopped_count = a.stopped_count.unwrap_or(0);
            let failed_count = a.failed_count.unwrap_or(0);
            let unreachable_count = a.unreachable_count.unwrap_or(0);

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
        cluster_size: Option<i32>,
        cluster_nodes: Option<Value>,
        referenced_app_id: Option<Uuid>,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
        // Agent info
        agent_hostname: Option<String>,
        gateway_id: Option<Uuid>,
        gateway_name: Option<String>,
        // Latest metrics from check
        last_check_metrics: Option<Value>,
    }

    let components = sqlx::query_as::<_, ComponentWithAgent>(
        r#"SELECT c.id, c.application_id, c.name, c.display_name, c.description, c.icon, c.group_id,
                  c.component_type, c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd,
                  c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                  c.is_optional, c.current_state, c.position_x, c.position_y,
                  c.cluster_size, c.cluster_nodes, c.referenced_app_id, c.created_at, c.updated_at,
                  a.hostname as agent_hostname, a.gateway_id, g.name as gateway_name,
                  (SELECT ce.metrics FROM check_events ce
                   WHERE ce.component_id = c.id AND ce.metrics IS NOT NULL
                   ORDER BY ce.created_at DESC LIMIT 1) as last_check_metrics
           FROM components c
           LEFT JOIN agents a ON c.agent_id = a.id
           LEFT JOIN gateways g ON a.gateway_id = g.id
           WHERE c.application_id = $1 ORDER BY c.name"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

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
        let status_rows = fetch_referenced_app_statuses(&state.db, &referenced_app_ids).await?;
        for (app_id, app_name, counts) in status_rows {
            referenced_app_names.insert(app_id, app_name);
            let (state, _) =
                compute_app_status(counts.0, counts.1, counts.2, counts.3, counts.4, counts.5);
            referenced_app_statuses.insert(app_id, state);
        }
    }

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
            // Find default site for organization (prefer 'primary' type)
            let site: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM sites WHERE organization_id = $1 AND is_active = true \
                 ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
            )
            .bind(user.organization_id)
            .fetch_optional(&state.db)
            .await?;

            match site {
                Some((id,)) => id,
                None => {
                    // Create a default site if none exists
                    let new_site_id = Uuid::new_v4();
                    sqlx::query(
                        "INSERT INTO sites (id, organization_id, name, code, site_type) \
                         VALUES ($1, $2, $3, $4, $5)",
                    )
                    .bind(new_site_id)
                    .bind(user.organization_id)
                    .bind("Default Site")
                    .bind("DEFAULT")
                    .bind("primary")
                    .execute(&state.db)
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
    .bind(site_id)
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
        &format!(
            "UPDATE applications SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                site_id = COALESCE($4, site_id),
                tags = COALESCE($5, tags),
                updated_at = {}
            WHERE id = $1 AND organization_id = $6
            RETURNING id, name, description, organization_id, site_id, tags, created_at, updated_at",
            crate::db::sql::now()
        ),
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
        .try_lock(id, "start", user.user_id)
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

    let action_id = log_action(
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
        return Err(ApiError::Conflict(
            "Application is already suspended".to_string(),
        ));
    }

    // Suspend the application
    sqlx::query(&format!(
        "UPDATE applications
             SET is_suspended = true, suspended_at = {now}, suspended_by = $2, updated_at = {now}
             WHERE id = $1",
        now = crate::db::sql::now()
    ))
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
        return Err(ApiError::Conflict(
            "Application is not suspended".to_string(),
        ));
    }

    // Resume the application
    sqlx::query(&format!(
        "UPDATE applications
             SET is_suspended = false, suspended_at = NULL, suspended_by = NULL, updated_at = {}
             WHERE id = $1",
        crate::db::sql::now()
    ))
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
    let _app = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM applications WHERE id = $1 AND organization_id = $2",
    )
    .bind(app_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    // Fetch the application's primary site info
    #[derive(Debug, sqlx::FromRow)]
    struct AppSiteInfo {
        site_id: Uuid,
        site_name: String,
        site_code: String,
        site_type: String,
    }

    let primary_site = sqlx::query_as::<_, AppSiteInfo>(
        r#"
        SELECT a.site_id, s.name as site_name, s.code as site_code, s.site_type
        FROM applications a
        JOIN sites s ON a.site_id = s.id
        WHERE a.id = $1
        "#,
    )
    .bind(app_id)
    .fetch_optional(&state.db)
    .await?;

    // Fetch all binding profile mappings with site info
    // Each profile is associated with gateways, and gateways belong to sites
    #[derive(Debug, sqlx::FromRow)]
    struct BindingRow {
        component_id: Uuid,
        component_name: String,
        #[allow(dead_code)]
        component_host: Option<String>,
        profile_id: Uuid,
        profile_name: String,
        profile_type: String,
        is_active: bool,
        agent_id: Uuid,
        agent_hostname: String,
        site_id: Uuid,
        site_name: String,
        site_code: String,
        site_type: String,
    }

    let bindings = sqlx::query_as::<_, BindingRow>(
        r#"
        SELECT DISTINCT ON (c.id, s.id)
            c.id as component_id,
            c.name as component_name,
            c.host as component_host,
            bp.id as profile_id,
            bp.name as profile_name,
            bp.profile_type,
            -- is_active is true if the component's current agent_id matches this binding's agent
            -- This correctly handles SELECTIVE switchover where profile isn't changed but agent_id is
            (c.agent_id = bpm.agent_id) as is_active,
            bpm.agent_id,
            a.hostname as agent_hostname,
            s.id as site_id,
            s.name as site_name,
            s.code as site_code,
            s.site_type
        FROM components c
        JOIN binding_profile_mappings bpm ON bpm.component_name = c.name
        JOIN binding_profiles bp ON bpm.profile_id = bp.id AND bp.application_id = c.application_id
        JOIN agents a ON bpm.agent_id = a.id
        JOIN gateways g ON a.gateway_id = g.id
        JOIN sites s ON g.site_id = s.id
        WHERE c.application_id = $1
        ORDER BY c.id, s.id, (c.agent_id = bpm.agent_id) DESC
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Fetch command overrides from site_overrides table
    #[derive(Debug, sqlx::FromRow)]
    struct CmdOverrideRow {
        component_id: Uuid,
        site_id: Uuid,
        check_cmd_override: Option<String>,
        start_cmd_override: Option<String>,
        stop_cmd_override: Option<String>,
        rebuild_cmd_override: Option<String>,
        env_vars_override: Option<Value>,
    }

    let cmd_overrides = sqlx::query_as::<_, CmdOverrideRow>(
        r#"
        SELECT component_id, site_id, check_cmd_override, start_cmd_override,
               stop_cmd_override, rebuild_cmd_override, env_vars_override
        FROM site_overrides
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Create a lookup map for command overrides
    let cmd_override_map: std::collections::HashMap<(Uuid, Uuid), &CmdOverrideRow> = cmd_overrides
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

// ══════════════════════════════════════════════════════════════════════════════
// Helper functions for cross-database compatibility
// ══════════════════════════════════════════════════════════════════════════════

/// Fetch status counts for referenced applications
/// Returns Vec of (app_id, app_name, (running, stopped, failed, starting, stopping, total))
#[cfg(feature = "postgres")]
async fn fetch_referenced_app_statuses(
    pool: &DbPool,
    app_ids: &[Uuid],
) -> Result<Vec<(Uuid, String, (i64, i64, i64, i64, i64, i64))>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        app_id: Uuid,
        app_name: String,
        running_count: Option<i64>,
        starting_count: Option<i64>,
        stopping_count: Option<i64>,
        stopped_count: Option<i64>,
        failed_count: Option<i64>,
        component_count: Option<i64>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            a.id as app_id,
            a.name as app_name,
            COUNT(c.id) as component_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'RUNNING') as running_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STARTING') as starting_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPING') as stopping_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPED') as stopped_count,
            COUNT(c.id) FILTER (WHERE c.current_state = 'FAILED') as failed_count
        FROM applications a
        LEFT JOIN components c ON c.application_id = a.id
        WHERE a.id = ANY($1)
        GROUP BY a.id, a.name
        "#,
    )
    .bind(app_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.app_id,
                r.app_name,
                (
                    r.running_count.unwrap_or(0),
                    r.stopped_count.unwrap_or(0),
                    r.failed_count.unwrap_or(0),
                    r.starting_count.unwrap_or(0),
                    r.stopping_count.unwrap_or(0),
                    r.component_count.unwrap_or(0),
                ),
            )
        })
        .collect())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_referenced_app_statuses(
    pool: &DbPool,
    app_ids: &[Uuid],
) -> Result<Vec<(Uuid, String, (i64, i64, i64, i64, i64, i64))>, sqlx::Error> {
    if app_ids.is_empty() {
        return Ok(Vec::new());
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        app_id: String,
        app_name: String,
        running_count: i64,
        starting_count: i64,
        stopping_count: i64,
        stopped_count: i64,
        failed_count: i64,
        component_count: i64,
    }

    let placeholders: Vec<String> = (1..=app_ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        r#"
        SELECT
            a.id as app_id,
            a.name as app_name,
            COUNT(c.id) as component_count,
            SUM(CASE WHEN c.current_state = 'RUNNING' THEN 1 ELSE 0 END) as running_count,
            SUM(CASE WHEN c.current_state = 'STARTING' THEN 1 ELSE 0 END) as starting_count,
            SUM(CASE WHEN c.current_state = 'STOPPING' THEN 1 ELSE 0 END) as stopping_count,
            SUM(CASE WHEN c.current_state = 'STOPPED' THEN 1 ELSE 0 END) as stopped_count,
            SUM(CASE WHEN c.current_state = 'FAILED' THEN 1 ELSE 0 END) as failed_count
        FROM applications a
        LEFT JOIN components c ON c.application_id = a.id
        WHERE a.id IN ({})
        GROUP BY a.id, a.name
        "#,
        placeholders.join(", ")
    );

    let mut q = sqlx::query_as::<_, Row>(&query);
    for id in app_ids {
        q = q.bind(id.to_string());
    }

    let rows: Vec<Row> = q.fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .filter_map(|r| {
            Uuid::parse_str(&r.app_id).ok().map(|id| {
                (
                    id,
                    r.app_name,
                    (
                        r.running_count,
                        r.stopped_count,
                        r.failed_count,
                        r.starting_count,
                        r.stopping_count,
                        r.component_count,
                    ),
                )
            })
        })
        .collect())
}
