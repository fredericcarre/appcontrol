use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: Uuid,
    pub hostname: String,
    pub organization_id: Uuid,
    pub gateway_id: Option<Uuid>,
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
    pub id: Uuid,
    pub hostname: String,
    pub organization_id: Uuid,
    pub gateway_id: Option<Uuid>,
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
    let agents = sqlx::query_as::<_, AgentListRow>(
        r#"
        SELECT a.id, a.hostname, a.organization_id, a.gateway_id, a.labels, a.ip_addresses,
               a.version, a.os_name, a.os_version, a.cpu_arch, a.cpu_cores,
               a.total_memory_mb, a.disk_total_gb,
               a.last_heartbeat_at, a.is_active, a.created_at,
               g.name as gateway_name, g.zone as gateway_zone
        FROM agents a
        LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1
        ORDER BY a.hostname
        "#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
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
    let agent = sqlx::query_as::<_, AgentRow>(
        r#"
        SELECT id, hostname, organization_id, gateway_id, labels, ip_addresses, version, last_heartbeat_at, is_active, created_at
        FROM agents
        WHERE id = $1 AND organization_id = $2
        "#,
    )
    .bind(id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(agent)))
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
    let agent: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT hostname, gateway_id FROM agents WHERE id = $1 AND organization_id = $2",
    )
    .bind(agent_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    let (hostname, gateway_id) = match agent {
        Some(a) => a,
        None => return Err(ApiError::NotFound),
    };

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

    let mut tx = state.db.begin().await?;

    // 1. Suspend the agent and clear gateway association
    sqlx::query("UPDATE agents SET is_active = false, gateway_id = NULL WHERE id = $1")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    // 2. Log the block event in certificate_events if table exists
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           SELECT $1, 'blocked', certificate_fingerprint, certificate_cn
           FROM agents WHERE id = $1"#,
    )
    .bind(agent_id)
    .execute(&mut *tx)
    .await
    .ok(); // Don't fail if table doesn't exist yet

    // 3. Transition all components of this agent to UNREACHABLE
    let components_affected = transition_agent_components_to_unreachable(&state, agent_id).await;

    tx.commit().await?;

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
    let agent: Option<(String,)> =
        sqlx::query_as("SELECT hostname FROM agents WHERE id = $1 AND organization_id = $2")
            .bind(agent_id)
            .bind(user.organization_id)
            .fetch_optional(&state.db)
            .await?;

    let hostname = match agent {
        Some((h,)) => h,
        None => return Err(ApiError::NotFound),
    };

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
    sqlx::query("UPDATE agents SET is_active = true WHERE id = $1")
        .bind(agent_id)
        .execute(&state.db)
        .await?;

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
    let agent_exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM agents WHERE id = $1 AND organization_id = $2")
            .bind(agent_id)
            .bind(user.organization_id)
            .fetch_optional(&state.db)
            .await?;

    if agent_exists.is_none() {
        return Err(ApiError::NotFound);
    }

    // Clamp minutes to valid range
    let minutes = params.minutes.clamp(1, 1440);

    let metrics = sqlx::query_as::<_, MetricPoint>(
        r#"
        SELECT cpu_pct, memory_pct, disk_used_pct, created_at
        FROM agent_metrics
        WHERE agent_id = $1 AND created_at > now() - ($2 || ' minutes')::interval
        ORDER BY created_at ASC
        "#,
    )
    .bind(agent_id)
    .bind(minutes)
    .fetch_all(&state.db)
    .await?;

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
        id: Uuid,
        name: String,
        application_id: Uuid,
        app_name: String,
    }

    let components: Vec<ComponentInfo> = sqlx::query_as(
        r#"
        SELECT c.id, c.name, c.application_id, a.name AS app_name
        FROM components c
        JOIN applications a ON c.application_id = a.id
        WHERE c.agent_id = $1
        "#,
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut affected = 0;

    for comp in &components {
        // Get current state
        let current_state = match crate::core::fsm::get_current_state(&state.db, comp.id).await {
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
        let result = sqlx::query(
            r#"
            INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
            VALUES ($1, $2, 'UNREACHABLE', 'agent_blocked',
                    jsonb_build_object('previous_state', $2, 'agent_id', $3::text))
            "#,
        )
        .bind(comp.id)
        .bind(current_state.to_string())
        .bind(agent_id.to_string())
        .execute(&state.db)
        .await;

        if result.is_ok() {
            affected += 1;

            // Push WebSocket event
            state.ws_hub.broadcast(
                comp.application_id,
                appcontrol_common::WsEvent::StateChange {
                    component_id: comp.id,
                    app_id: comp.application_id,
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
    let valid_agents: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, hostname FROM agents WHERE id = ANY($1) AND organization_id = $2",
    )
    .bind(&body.agent_ids)
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

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
    sqlx::query("UPDATE components SET agent_id = NULL WHERE agent_id = ANY($1)")
        .bind(&valid_ids)
        .execute(&mut *tx)
        .await?;

    // 3. Delete discovery reports for these agents
    sqlx::query("DELETE FROM discovery_reports WHERE agent_id = ANY($1)")
        .bind(&valid_ids)
        .execute(&mut *tx)
        .await?;

    // 4. Delete the agents
    let delete_result = sqlx::query("DELETE FROM agents WHERE id = ANY($1)")
        .bind(&valid_ids)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    // 5. Notify gateways to remove these agents from their registry
    for agent_id in &valid_ids {
        state
            .ws_hub
            .block_agent(*agent_id, "Agent deleted by administrator");
    }

    Ok(Json(json!({
        "deleted": delete_result.rows_affected(),
        "agent_ids": valid_ids,
        "components_affected": components_affected,
    })))
}
