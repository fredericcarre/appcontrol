use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};
use crate::error::{ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: DbUuid,
    pub hostname: String,
    pub organization_id: DbUuid,
    pub gateway_id: Option<DbUuid>,
    pub labels: Value,
    pub ip_addresses: Value,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Row type for agent list query with gateway info and system info
#[derive(Debug, sqlx::FromRow)]
pub struct AgentListRow {
    pub id: DbUuid,
    pub hostname: String,
    pub organization_id: DbUuid,
    pub gateway_id: Option<DbUuid>,
    pub labels: Value,
    pub ip_addresses: Value,
    pub version: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub cpu_arch: Option<String>,
    pub cpu_cores: Option<i32>,
    pub total_memory_mb: Option<i64>,
    pub disk_total_gb: Option<i64>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub gateway_name: Option<String>,
    pub gateway_zone: Option<String>,
}

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let agents = state
        .agent_repo
        .list_agents(*user.organization_id)
        .await?;

    // Get live connection status from the WebSocket hub
    let connected_agents = state.ws_hub.connected_agent_ids();
    let connected_agent_set: std::collections::HashSet<Uuid> =
        connected_agents.into_iter().collect();
    let connected_gateways = state.ws_hub.connected_gateway_ids();
    let connected_gateway_set: std::collections::HashSet<Uuid> =
        connected_gateways.into_iter().collect();

    // Enrich agents with connection status and gateway info
    let agents_with_status: Vec<Value> = agents
        .into_iter()
        .map(|a| {
            let connected = connected_agent_set.contains(&a.id);
            let gateway_connected = a
                .gateway_id
                .map(|gid| connected_gateway_set.contains(&gid))
                .unwrap_or(false);
            json!({
                "id": a.id,
                "hostname": a.hostname,
                "organization_id": a.organization_id,
                "gateway_id": a.gateway_id,
                "labels": a.labels,
                "ip_addresses": a.ip_addresses,
                "version": a.version,
                "os_name": a.os_name,
                "os_version": a.os_version,
                "cpu_arch": a.cpu_arch,
                "cpu_cores": a.cpu_cores,
                "total_memory_mb": a.total_memory_mb,
                "disk_total_gb": a.disk_total_gb,
                "last_heartbeat_at": a.last_heartbeat_at,
                "is_active": a.is_active,
                "created_at": a.created_at,
                "connected": connected,
                "gateway_name": a.gateway_name,
                "gateway_zone": a.gateway_zone,
                "gateway_connected": gateway_connected,
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agents_with_status })))
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let agent = state
        .agent_repo
        .get_agent(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!({
        "id": agent.id,
        "hostname": agent.hostname,
        "organization_id": agent.organization_id,
        "gateway_id": agent.gateway_id,
        "labels": agent.labels,
        "ip_addresses": agent.ip_addresses,
        "version": agent.version,
        "last_heartbeat_at": agent.last_heartbeat_at,
        "is_active": agent.is_active,
        "created_at": agent.created_at,
    })))
}

/// POST /api/v1/agents/:id/block — Block an agent (security action)
///
/// This suspends the agent, disconnects it from the gateway, and prevents reconnection.
/// Use for compromised machines that need to be isolated immediately.
pub async fn block_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get the agent to verify it exists and get info for logging
    let (hostname, gateway_id) = state
        .agent_repo
        .get_agent_info(agent_id, *user.organization_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "block_agent",
        "agent",
        agent_id,
        json!({ "hostname": &hostname, "gateway_id": gateway_id }),
    )
    .await
    .ok();

    // 1. Suspend the agent and clear gateway association
    state.agent_repo.block_agent(agent_id).await?;

    // 2. Log the block event in certificate_events if table exists
    crate::repository::agents::log_agent_block_event(&state.db, agent_id).await;

    // 3. Transition all components of this agent to UNREACHABLE
    let components_affected = transition_agent_components_to_unreachable(&state, agent_id).await;

    // 4. Send block command to all gateways — adds agent to blocklist
    // so it's rejected even on reconnection attempts
    state
        .ws_hub
        .block_agent(agent_id, "Agent blocked by administrator");

    Ok(Json(json!({
        "status": "blocked",
        "agent_id": agent_id,
        "hostname": hostname,
        "components_affected": components_affected,
    })))
}

/// POST /api/v1/agents/:id/unblock — Unblock a previously blocked agent
///
/// Reactivates the agent, allowing it to reconnect. The agent will need to
/// re-establish its gateway connection.
pub async fn unblock_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get the agent to verify it exists
    let (hostname, _) = state
        .agent_repo
        .get_agent_info(agent_id, *user.organization_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "unblock_agent",
        "agent",
        agent_id,
        json!({ "hostname": &hostname }),
    )
    .await
    .ok();

    // Reactivate the agent
    state.agent_repo.unblock_agent(agent_id).await?;

    // Send unblock command to all gateways — removes agent from blocklist
    state.ws_hub.unblock_agent(agent_id);

    Ok(Json(json!({
        "status": "unblocked",
        "agent_id": agent_id,
        "hostname": hostname,
    })))
}

/// Query parameters for metrics endpoint
#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    /// Number of minutes of history to retrieve (default: 60, max: 1440 = 24h)
    #[serde(default = "default_minutes")]
    pub minutes: i32,
}

fn default_minutes() -> i32 {
    60
}

/// Metric data point for time-series graphing
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MetricPoint {
    pub cpu_pct: f32,
    pub memory_pct: f32,
    pub disk_used_pct: Option<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/agents/:id/metrics — Get agent CPU/memory metrics for graphing
///
/// Returns time-series data for the specified agent.
/// Query params:
///   - minutes: Number of minutes of history (default: 60, max: 1440)
pub async fn get_agent_metrics(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
    Query(params): Query<MetricsQuery>,
) -> Result<Json<Value>, ApiError> {
    // Verify agent belongs to user's organization
    if !crate::repository::agents::agent_exists_in_org(&state.db, agent_id, *user.organization_id).await? {
        return Err(ApiError::NotFound);
    }

    // Clamp minutes to valid range
    let minutes = params.minutes.clamp(1, 1440);

    let metrics = crate::repository::agents::fetch_agent_metrics::<MetricPoint>(&state.db, agent_id, minutes).await?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "minutes": minutes,
        "metrics": metrics,
    })))
}

/// Helper: Transition all components of an agent to UNREACHABLE when agent is blocked/disconnected.
/// Returns the number of components affected.
async fn transition_agent_components_to_unreachable(state: &AppState, agent_id: Uuid) -> i32 {
    use appcontrol_common::ComponentState;

    // Get all components for this agent that are NOT already UNREACHABLE/STOPPED/STOPPING
    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        id: DbUuid,
        name: String,
        application_id: DbUuid,
        app_name: String,
    }

    let components: Vec<ComponentInfo> = crate::repository::agents::get_agent_components(&state.db, agent_id)
        .await
        .unwrap_or_default();

    let mut affected = 0;

    for comp in &components {
        // Get current state
        let current_state = match crate::core::fsm::get_current_state(&state.db, *comp.id).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Skip if already UNREACHABLE, STOPPED, or STOPPING
        match current_state {
            ComponentState::Unreachable | ComponentState::Stopped | ComponentState::Stopping => {
                continue;
            }
            _ => {}
        }

        // Insert state transition (append-only)
        let details_json = serde_json::json!({
            "previous_state": current_state.to_string(),
            "agent_id": agent_id.to_string(),
        });
        let result = crate::repository::agents::insert_unreachable_transition(
            &state.db, *comp.id, &current_state.to_string(), &details_json,
        ).await;

        if result.is_ok() {
            affected += 1;

            // Push WebSocket event
            state.ws_hub.broadcast(
                *comp.application_id,
                appcontrol_common::WsEvent::StateChange {
                    component_id: *comp.id,
                    app_id: *comp.application_id,
                    component_name: Some(comp.name.clone()),
                    app_name: Some(comp.app_name.clone()),
                    from: current_state,
                    to: ComponentState::Unreachable,
                    at: chrono::Utc::now(),
                },
            );

            tracing::info!(
                component_id = %comp.id,
                component_name = %comp.name,
                from = %current_state,
                agent_id = %agent_id,
                "Component transitioned to UNREACHABLE (agent blocked)"
            );
        }
    }

    affected
}

// ===========================================================================
// Delete single agent
// ===========================================================================

/// DELETE /api/v1/agents/:id — Delete a single agent
///
/// This permanently deletes an agent. Components associated with this agent
/// will have their agent_id set to NULL and will transition to UNREACHABLE.
/// Only admin users can perform this operation.
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Verify agent exists and belongs to user's organization
    let agent: Option<(DbUuid, String)> = crate::repository::agents::get_agent_in_org(&state.db, agent_id, *user.organization_id).await?;

    let (_, hostname) = agent.ok_or(ApiError::NotFound)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "delete_agent",
        "agent",
        agent_id,
        json!({
            "hostname": hostname,
        }),
    )
    .await
    .ok();

    // 1. Transition components to UNREACHABLE
    let components_affected = transition_agent_components_to_unreachable(&state, agent_id).await;

    // 2-6. Delete agent and cascade
    crate::repository::agents::delete_agent_cascade(&state.db, agent_id).await?;

    tracing::info!(
        agent_id = %agent_id,
        hostname = %hostname,
        components_affected = components_affected,
        "Agent deleted"
    );

    Ok(Json(json!({
        "status": "deleted",
        "agent_id": agent_id,
        "hostname": hostname,
        "components_affected": components_affected,
    })))
}

// ===========================================================================
// Bulk delete stale agents
// ===========================================================================

/// Request body for bulk agent deletion
#[derive(Debug, Deserialize)]
pub struct BulkDeleteRequest {
    pub agent_ids: Vec<Uuid>,
}

/// POST /api/v1/agents/bulk-delete — Delete multiple stale agents
///
/// This permanently deletes agents that are no longer connected or needed.
/// Only admin users can perform this operation.
/// Components associated with these agents will have their agent_id set to NULL.
pub async fn bulk_delete_agents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<BulkDeleteRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if body.agent_ids.is_empty() {
        return Err(ApiError::Validation(
            "At least one agent_id is required".to_string(),
        ));
    }

    // Verify all agents belong to the user's organization
    let valid_agents =
        crate::repository::agents::verify_agents_in_org(&state.db, &body.agent_ids, *user.organization_id).await?;

    if valid_agents.is_empty() {
        return Err(ApiError::NotFound);
    }

    let valid_ids: Vec<Uuid> = valid_agents.iter().map(|(id, _)| *id).collect();

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "bulk_delete_agents",
        "agent",
        Uuid::nil(),
        json!({
            "agent_ids": valid_ids,
            "count": valid_ids.len(),
        }),
    )
    .await
    .ok();

    let mut tx = state.db.begin().await?;

    // 1. Transition components to UNREACHABLE and clear agent_id
    let mut components_affected = 0;
    for agent_id in &valid_ids {
        components_affected += transition_agent_components_to_unreachable(&state, *agent_id).await;
    }

    // 2. Clear agent_id from components (don't delete components)
    // 3-6. Delete related records and agents
    crate::repository::agents::bulk_delete_agent_records(&mut tx, &valid_ids).await?;

    let delete_result = valid_ids.len();

    tx.commit().await?;

    // 5. Notify gateways to remove these agents from their registry
    for agent_id in &valid_ids {
        state
            .ws_hub
            .block_agent(*agent_id, "Agent deleted by administrator");
    }

    Ok(Json(json!({
        "deleted": delete_result,
        "agent_ids": valid_ids,
        "components_affected": components_affected,
    })))
}

// Helper functions moved to repository::agents
