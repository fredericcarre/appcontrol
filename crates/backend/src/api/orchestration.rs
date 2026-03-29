use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbUuid;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    pub dry_run: Option<bool>,
    /// If true, skip pre-flight checks (not recommended for production)
    pub skip_preflight: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct WaitQuery {
    pub timeout: Option<u64>,
}

/// Pre-flight check result for an application start
#[derive(Debug, Clone)]
pub struct PreflightResult {
    pub can_start: bool,
    pub unreachable_agents: Vec<(DbUuid, String)>, // (agent_id, hostname)
    pub disconnected_gateways: Vec<(DbUuid, String)>, // (gateway_id, name)
    pub components_without_agent: Vec<(DbUuid, String)>, // (component_id, name)
}

/// Check if all agents for an application are reachable before starting
pub async fn preflight_check(state: &AppState, app_id: Uuid) -> PreflightResult {
    // Get all components with their agent information
    let components =
        sqlx::query_as::<_, (DbUuid, String, Option<DbUuid>, Option<String>, Option<DbUuid>)>(
            r#"
        SELECT c.id, c.name, c.agent_id, a.hostname, a.gateway_id
        FROM components c
        LEFT JOIN agents a ON c.agent_id = a.id
        WHERE c.application_id = $1 AND c.is_optional = false
        "#,
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    // Get connected agents and gateways from WebSocket hub
    let connected_agents: HashSet<Uuid> = state.ws_hub.connected_agent_ids().into_iter().collect();
    let connected_gateways: HashSet<Uuid> =
        state.ws_hub.connected_gateway_ids().into_iter().collect();

    let mut unreachable_agents = Vec::new();
    let mut disconnected_gateways = Vec::new();
    let mut components_without_agent = Vec::new();
    let mut seen_gateways: HashSet<Uuid> = HashSet::new();

    for (comp_id, comp_name, agent_id, agent_hostname, gateway_id) in components {
        match agent_id {
            None => {
                // Component has no agent assigned
                components_without_agent.push((comp_id, comp_name));
            }
            Some(aid) => {
                let hostname = agent_hostname.unwrap_or_else(|| "unknown".to_string());

                // Check if agent is connected
                if !connected_agents.contains(&aid) {
                    unreachable_agents.push((aid, hostname.clone()));
                }

                // Check if gateway is connected (if agent has one)
                if let Some(gid) = gateway_id {
                    if !seen_gateways.contains(&gid) && !connected_gateways.contains(&gid) {
                        // Get gateway name
                        let gw_name = sqlx::query_scalar::<_, String>(
                            "SELECT name FROM gateways WHERE id = $1",
                        )
                        .bind(gid)
                        .fetch_optional(&state.db)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| gid.to_string());

                        disconnected_gateways.push((gid, gw_name));
                        seen_gateways.insert(*gid);
                    }
                }
            }
        }
    }

    let can_start = unreachable_agents.is_empty()
        && disconnected_gateways.is_empty()
        && components_without_agent.is_empty();

    PreflightResult {
        can_start,
        unreachable_agents,
        disconnected_gateways,
        components_without_agent,
    }
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<Option<StartRequest>>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let dry_run = body.as_ref().and_then(|b| b.dry_run).unwrap_or(false);
    let skip_preflight = body
        .as_ref()
        .and_then(|b| b.skip_preflight)
        .unwrap_or(false);

    // Pre-flight checks: verify all agents are reachable before starting
    let preflight = preflight_check(&state, app_id).await;

    if !preflight.can_start && !skip_preflight && !dry_run {
        // Build detailed error message for scheduler integration
        let mut issues = Vec::new();

        if !preflight.unreachable_agents.is_empty() {
            let agents: Vec<String> = preflight
                .unreachable_agents
                .iter()
                .map(|(_, h)| h.clone())
                .collect();
            issues.push(format!("unreachable_agents: [{}]", agents.join(", ")));
        }

        if !preflight.disconnected_gateways.is_empty() {
            let gateways: Vec<String> = preflight
                .disconnected_gateways
                .iter()
                .map(|(_, n)| n.clone())
                .collect();
            issues.push(format!("disconnected_gateways: [{}]", gateways.join(", ")));
        }

        if !preflight.components_without_agent.is_empty() {
            let components: Vec<String> = preflight
                .components_without_agent
                .iter()
                .map(|(_, n)| n.clone())
                .collect();
            issues.push(format!(
                "components_without_agent: [{}]",
                components.join(", ")
            ));
        }

        tracing::warn!(
            app_id = %app_id,
            "Start blocked by pre-flight check: {}",
            issues.join("; ")
        );

        return Ok(Json(json!({
            "status": "blocked",
            "error": "preflight_check_failed",
            "message": "Cannot start application: some agents or gateways are not reachable",
            "details": {
                "unreachable_agents": preflight.unreachable_agents.iter()
                    .map(|(id, h)| json!({"id": id, "hostname": h}))
                    .collect::<Vec<_>>(),
                "disconnected_gateways": preflight.disconnected_gateways.iter()
                    .map(|(id, n)| json!({"id": id, "name": n}))
                    .collect::<Vec<_>>(),
                "components_without_agent": preflight.components_without_agent.iter()
                    .map(|(id, n)| json!({"id": id, "name": n}))
                    .collect::<Vec<_>>(),
            }
        })));
    }

    log_action(
        &state.db,
        user.user_id,
        "orchestration_start",
        "application",
        app_id,
        json!({
            "dry_run": dry_run,
            "skip_preflight": skip_preflight,
            "preflight_passed": preflight.can_start
        }),
    )
    .await?;

    let plan = crate::core::sequencer::build_start_plan(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if dry_run {
        return Ok(Json(json!({
            "status": "dry_run",
            "plan": plan,
            "preflight": {
                "passed": preflight.can_start,
                "unreachable_agents": preflight.unreachable_agents.len(),
                "disconnected_gateways": preflight.disconnected_gateways.len(),
                "components_without_agent": preflight.components_without_agent.len(),
            }
        })));
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(app_id, "orchestration_start", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        if let Err(e) = crate::core::sequencer::execute_start(&state_clone, app_id).await {
            tracing::error!("Orchestration start failed for {}: {}", app_id, e);
        }
    });

    Ok(Json(json!({
        "status": "starting",
        "plan": plan,
        "preflight": {
            "passed": preflight.can_start,
            "skipped": skip_preflight && !preflight.can_start,
        }
    })))
}

pub async fn stop(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Acquire operation lock — prevents concurrent start/stop on the same app
    let guard = state
        .operation_lock
        .try_lock(app_id, "orchestration_stop", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    log_action(
        &state.db,
        user.user_id,
        "orchestration_stop",
        "application",
        app_id,
        json!({}),
    )
    .await?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard; // Hold the lock until the operation completes
        if let Err(e) = crate::core::sequencer::execute_stop(&state_clone, app_id).await {
            tracing::error!("Orchestration stop failed for {}: {}", app_id, e);
        }
    });

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let components = sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"
        SELECT c.id, c.name, c.current_state
        FROM components c
        WHERE c.application_id = $1
        ORDER BY c.name
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = components
        .iter()
        .map(|(id, name, state)| json!({"component_id": id, "name": name, "state": state}))
        .collect();

    let all_running = components.iter().all(|(_, _, s)| s == "RUNNING");

    Ok(Json(json!({
        "app_id": app_id,
        "components": data,
        "all_running": all_running,
    })))
}

pub async fn wait_running(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<WaitQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let timeout = std::time::Duration::from_secs(params.timeout.unwrap_or(300));
    let start_time = std::time::Instant::now();

    loop {
        let components = sqlx::query_as::<_, (DbUuid, String, String)>(
            r#"
            SELECT c.id, c.name, c.current_state
            FROM components c
            WHERE c.application_id = $1 AND c.is_optional = false
            "#,
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await?;

        let total = components.len();
        let running_count = components.iter().filter(|(_, _, s)| s == "RUNNING").count();
        let failed: Vec<_> = components
            .iter()
            .filter(|(_, _, s)| s == "FAILED")
            .collect();
        let unreachable: Vec<_> = components
            .iter()
            .filter(|(_, _, s)| s == "UNREACHABLE")
            .collect();
        let starting_count = components
            .iter()
            .filter(|(_, _, s)| s == "STARTING")
            .count();
        let stopped_count = components.iter().filter(|(_, _, s)| s == "STOPPED").count();

        // All components running → success
        if running_count == total {
            return Ok(Json(json!({
                "status": "running",
                "components": {
                    "total": total,
                    "running": running_count,
                }
            })));
        }

        // Any failed → fail immediately
        if !failed.is_empty() {
            let failed_names: Vec<String> = failed.iter().map(|(_, n, _)| n.clone()).collect();
            return Ok(Json(json!({
                "status": "failed",
                "error": "component_failed",
                "message": format!("Components failed: {}", failed_names.join(", ")),
                "failed_components": failed.iter()
                    .map(|(id, name, _)| json!({"id": id, "name": name}))
                    .collect::<Vec<_>>(),
                "components": {
                    "total": total,
                    "running": running_count,
                    "failed": failed.len(),
                    "starting": starting_count,
                }
            })));
        }

        // Any unreachable → fail immediately (agent/gateway down)
        if !unreachable.is_empty() {
            let unreachable_names: Vec<String> =
                unreachable.iter().map(|(_, n, _)| n.clone()).collect();
            return Ok(Json(json!({
                "status": "unreachable",
                "error": "agent_unreachable",
                "message": format!("Components unreachable (agent/gateway down): {}", unreachable_names.join(", ")),
                "unreachable_components": unreachable.iter()
                    .map(|(id, name, _)| json!({"id": id, "name": name}))
                    .collect::<Vec<_>>(),
                "components": {
                    "total": total,
                    "running": running_count,
                    "unreachable": unreachable.len(),
                    "starting": starting_count,
                }
            })));
        }

        // Timeout
        if start_time.elapsed() > timeout {
            let not_running: Vec<_> = components
                .iter()
                .filter(|(_, _, s)| s != "RUNNING")
                .collect();
            let not_running_names: Vec<String> =
                not_running.iter().map(|(_, n, _)| n.clone()).collect();

            return Ok(Json(json!({
                "status": "timeout",
                "error": "start_timeout",
                "message": format!("Timeout waiting for components: {}", not_running_names.join(", ")),
                "timeout_seconds": timeout.as_secs(),
                "components": {
                    "total": total,
                    "running": running_count,
                    "starting": starting_count,
                    "stopped": stopped_count,
                }
            })));
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

/// GET /api/v1/orchestration/:app_id/health
///
/// Health check for scheduler integration - returns clear, machine-readable status.
/// Returns HTTP 200 with status "healthy" when all components are RUNNING.
/// Returns HTTP 503 with detailed error when not healthy.
pub async fn health(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Get component states
    let components = sqlx::query_as::<_, (DbUuid, String, String, Option<DbUuid>)>(
        r#"
        SELECT c.id, c.name, c.current_state, c.agent_id
        FROM components c
        WHERE c.application_id = $1 AND c.is_optional = false
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Pre-flight check for real-time agent status
    let preflight = preflight_check(&state, app_id).await;

    let total = components.len();
    let running = components
        .iter()
        .filter(|(_, _, s, _)| s == "RUNNING")
        .count();
    let failed: Vec<_> = components
        .iter()
        .filter(|(_, _, s, _)| s == "FAILED")
        .collect();
    let unreachable: Vec<_> = components
        .iter()
        .filter(|(_, _, s, _)| s == "UNREACHABLE")
        .collect();
    let stopped: Vec<_> = components
        .iter()
        .filter(|(_, _, s, _)| s == "STOPPED")
        .collect();
    let starting: Vec<_> = components
        .iter()
        .filter(|(_, _, s, _)| s == "STARTING")
        .collect();

    // Determine overall health
    if running == total && preflight.can_start {
        return Ok((
            StatusCode::OK,
            Json(json!({
                "status": "healthy",
                "app_id": app_id,
                "components": {
                    "total": total,
                    "running": running,
                },
                "agents": {
                    "all_reachable": true,
                }
            })),
        ));
    }

    // Not healthy - return 503 with details
    let mut issues = Vec::new();

    if !failed.is_empty() {
        issues.push("components_failed");
    }
    if !unreachable.is_empty() {
        issues.push("components_unreachable");
    }
    if !stopped.is_empty() {
        issues.push("components_stopped");
    }
    if !starting.is_empty() {
        issues.push("components_starting");
    }
    if !preflight.unreachable_agents.is_empty() {
        issues.push("agents_disconnected");
    }
    if !preflight.disconnected_gateways.is_empty() {
        issues.push("gateways_disconnected");
    }

    Ok((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "status": "unhealthy",
            "app_id": app_id,
            "issues": issues,
            "components": {
                "total": total,
                "running": running,
                "failed": failed.len(),
                "unreachable": unreachable.len(),
                "stopped": stopped.len(),
                "starting": starting.len(),
            },
            "agents": {
                "all_reachable": preflight.can_start,
                "unreachable": preflight.unreachable_agents.iter()
                    .map(|(id, h)| json!({"id": id, "hostname": h}))
                    .collect::<Vec<_>>(),
                "disconnected_gateways": preflight.disconnected_gateways.iter()
                    .map(|(id, n)| json!({"id": id, "name": n}))
                    .collect::<Vec<_>>(),
            },
            "failed_components": failed.iter()
                .map(|(id, name, _, _)| json!({"id": id, "name": name}))
                .collect::<Vec<_>>(),
            "unreachable_components": unreachable.iter()
                .map(|(id, name, _, _)| json!({"id": id, "name": name}))
                .collect::<Vec<_>>(),
        })),
    ))
}

/// GET /api/v1/orchestration/:app_id/preflight
///
/// Pre-flight check endpoint for scheduler integration.
/// Verifies all agents and gateways are reachable before attempting to start.
/// Use this before calling /start to avoid failed starts due to connectivity issues.
pub async fn preflight(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let result = preflight_check(&state, app_id).await;

    Ok(Json(json!({
        "app_id": app_id,
        "can_start": result.can_start,
        "unreachable_agents": result.unreachable_agents.iter()
            .map(|(id, h)| json!({"id": id, "hostname": h}))
            .collect::<Vec<_>>(),
        "disconnected_gateways": result.disconnected_gateways.iter()
            .map(|(id, n)| json!({"id": id, "name": n}))
            .collect::<Vec<_>>(),
        "components_without_agent": result.components_without_agent.iter()
            .map(|(id, n)| json!({"id": id, "name": n}))
            .collect::<Vec<_>>(),
    })))
}
