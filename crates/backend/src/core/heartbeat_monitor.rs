//! Background task that monitors agent heartbeats and transitions components
//! to UNREACHABLE when their agent has been silent for too long.
//!
//! This distinguishes "check failed" (FAILED — agent ran the check, it returned error)
//! from "agent unavailable" (UNREACHABLE — no heartbeat, we don't know the real state).

use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "postgres")]
use uuid::Uuid;

use crate::AppState;
use crate::db::DbUuid;
#[cfg(feature = "postgres")]
use appcontrol_common::ComponentState;

/// Row returned when querying for stale agents and their components.
#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct StaleComponent {
    component_id: DbUuid,
    component_name: String,
    agent_id: DbUuid,
    application_id: DbUuid,
    app_name: String,
    /// Whether the agent is blocked (is_active = false) vs stale heartbeat
    agent_blocked: bool,
}

/// Start the heartbeat monitor background task.
/// Runs every `check_interval` seconds, queries for agents whose last_heartbeat_at
/// exceeds the organization's configured timeout, and transitions their components
/// to UNREACHABLE. Also monitors gateway heartbeats and marks them disconnected.
/// When agents reconnect with UNREACHABLE components, triggers resync.
///
/// NOTE: Currently only implemented for PostgreSQL. SQLite support is pending.
#[cfg(feature = "postgres")]
pub async fn run_heartbeat_monitor(state: Arc<AppState>, check_interval: Duration) {
    let mut interval = tokio::time::interval(check_interval);

    loop {
        interval.tick().await;

        // Check stale gateways first (they affect agent connectivity)
        if let Err(e) = check_stale_gateways(&state).await {
            tracing::error!("Gateway heartbeat monitor error: {}", e);
        }

        if let Err(e) = check_stale_agents(&state).await {
            tracing::error!("Heartbeat monitor error: {}", e);
        }

        // Resync UNREACHABLE components when agent is active
        if let Err(e) = resync_unreachable_components(&state).await {
            tracing::error!("Resync unreachable components error: {}", e);
        }
    }
}

/// SQLite stub: heartbeat monitoring not yet implemented for SQLite.
/// Components won't transition to UNREACHABLE automatically.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn run_heartbeat_monitor(_state: Arc<AppState>, check_interval: Duration) {
    // Log once at startup that this feature is not available
    tracing::warn!(
        "Heartbeat monitor disabled: not yet implemented for SQLite backend. \
         Components will not automatically transition to UNREACHABLE on agent timeout."
    );

    // Sleep forever to keep the task alive but do nothing
    let mut interval = tokio::time::interval(check_interval);
    loop {
        interval.tick().await;
        // No-op for SQLite
    }
}

/// Gateway heartbeat timeout in seconds (2 minutes).
/// Gateways should send heartbeats every 60 seconds, so 2 minutes means we missed 2+.
#[cfg(feature = "postgres")]
const GATEWAY_HEARTBEAT_TIMEOUT_SECS: i64 = 120;

/// Check for gateways that have missed heartbeats and mark them as suspended.
#[cfg(feature = "postgres")]
async fn check_stale_gateways(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    // Find gateways with stale heartbeats that are still marked as 'active'
    // Mark them as 'suspended' (the valid status for unavailable gateways)
    let stale_result = sqlx::query(
        r#"
        UPDATE gateways
        SET status = 'suspended'
        WHERE status = 'active'
          AND last_heartbeat_at IS NOT NULL
          AND last_heartbeat_at < now() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(GATEWAY_HEARTBEAT_TIMEOUT_SECS)
    .execute(&state.db)
    .await?;
    let stale_count = stale_result.rows_affected();

    if stale_count > 0 {
        tracing::warn!(
            count = stale_count,
            timeout_secs = GATEWAY_HEARTBEAT_TIMEOUT_SECS,
            "Marked stale gateways as suspended (no heartbeat)"
        );
    }

    // Also update gateways that reconnect (have recent heartbeat but are marked suspended)
    let reconnected_result = sqlx::query(
        r#"
        UPDATE gateways
        SET status = 'active'
        WHERE status = 'suspended'
          AND last_heartbeat_at IS NOT NULL
          AND last_heartbeat_at >= now() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(GATEWAY_HEARTBEAT_TIMEOUT_SECS)
    .execute(&state.db)
    .await?;
    let reconnected_count = reconnected_result.rows_affected();

    if reconnected_count > 0 {
        tracing::info!(
            count = reconnected_count,
            "Gateways reconnected (heartbeat resumed)"
        );
    }

    Ok(())
}

/// Check for agents that have missed their heartbeat timeout and transition
/// their components to UNREACHABLE.
#[cfg(feature = "postgres")]
async fn check_stale_agents(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    // Find components whose agent has exceeded the org-level heartbeat timeout
    // and that are NOT already in UNREACHABLE, STOPPED, or STOPPING state.
    let stale_components = sqlx::query_as::<_, StaleComponent>(
        r#"
        SELECT c.id AS component_id, c.name AS component_name, c.agent_id, c.application_id,
               app.name AS app_name, NOT a.is_active AS agent_blocked
        FROM components c
        JOIN agents a ON a.id = c.agent_id
        JOIN applications app ON app.id = c.application_id
        JOIN organizations o ON o.id = a.organization_id
        LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE c.agent_id IS NOT NULL
          AND (
            -- Case 1: Active agent with stale heartbeat (timeout exceeded)
            (a.is_active = true
             AND a.last_heartbeat_at IS NOT NULL
             AND a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval)
            OR
            -- Case 2: Blocked/inactive agent (components should be UNREACHABLE)
            (a.is_active = false)
            OR
            -- Case 3: Gateway is blocked/inactive (all its agents' components should be UNREACHABLE)
            (g.id IS NOT NULL AND g.is_active = false)
          )
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    if stale_components.is_empty() {
        return Ok(());
    }

    // Group by agent for logging
    let mut agent_ids: Vec<DbUuid> = stale_components.iter().map(|c| c.agent_id).collect();
    agent_ids.sort();
    agent_ids.dedup();

    tracing::warn!(
        stale_agents = agent_ids.len(),
        stale_components = stale_components.len(),
        "Detected stale agents — transitioning components to UNREACHABLE"
    );

    for comp in &stale_components {
        // Get current state from latest state_transition
        let current = crate::core::fsm::get_current_state(&state.db, comp.component_id).await;
        let current_state = match current {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Don't transition if already UNREACHABLE, STOPPED, or STOPPING
        match current_state {
            ComponentState::Unreachable | ComponentState::Stopped | ComponentState::Stopping => {
                continue;
            }
            _ => {}
        }

        // Transition to UNREACHABLE with appropriate trigger
        let trigger = if comp.agent_blocked {
            "agent_blocked"
        } else {
            "heartbeat_timeout"
        };
        if let Err(e) = transition_to_unreachable(state, comp, current_state, trigger).await {
            tracing::warn!(
                component_id = %comp.component_id,
                "Failed to transition to UNREACHABLE: {}", e
            );
        }
    }

    // NOTE: We do NOT set is_active = false here.
    // is_active is controlled only by explicit admin actions (block/unblock).
    // The heartbeat_monitor only transitions component states to UNREACHABLE.

    Ok(())
}

#[cfg(feature = "postgres")]
/// Transition a single component to UNREACHABLE, recording the previous state
/// in the details for recovery when the agent reconnects.
async fn transition_to_unreachable(
    state: &Arc<AppState>,
    comp: &StaleComponent,
    current_state: ComponentState,
    trigger: &str,
) -> Result<(), crate::core::fsm::FsmError> {
    // Insert state transition (append-only)
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
        VALUES ($1, $2, 'UNREACHABLE', $4,
                jsonb_build_object('previous_state', $2, 'agent_id', $3::text))
        "#,
    )
    .bind(comp.component_id)
    .bind(current_state.to_string())
    .bind(comp.agent_id.to_string())
    .bind(trigger)
    .execute(&state.db)
    .await
    .map_err(|e| crate::core::fsm::FsmError::Database(e.to_string()))?;

    // Update cached current_state on the component
    sqlx::query("UPDATE components SET current_state = 'UNREACHABLE' WHERE id = $1")
        .bind(comp.component_id)
        .execute(&state.db)
        .await
        .map_err(|e| crate::core::fsm::FsmError::Database(e.to_string()))?;

    metrics::counter!(
        "state_transitions_total",
        "from" => current_state.to_string(),
        "to" => "UNREACHABLE".to_string()
    )
    .increment(1);

    // Push WebSocket event
    state.ws_hub.broadcast(
        comp.application_id,
        appcontrol_common::WsEvent::StateChange {
            component_id: *comp.component_id,
            app_id: *comp.application_id,
            component_name: Some(comp.component_name.clone()),
            app_name: Some(comp.app_name.clone()),
            from: current_state,
            to: ComponentState::Unreachable,
            at: chrono::Utc::now(),
        },
    );

    tracing::info!(
        component_id = %comp.component_id,
        from = %current_state,
        agent_id = %comp.agent_id,
        trigger = %trigger,
        "Component transitioned to UNREACHABLE"
    );

    Ok(())
}

/// Agent with UNREACHABLE components that has a recent heartbeat.
#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct AgentToResync {
    agent_id: DbUuid,
    unreachable_count: i64,
}

#[cfg(feature = "postgres")]
/// Detect agents that are active (recent heartbeat) but have UNREACHABLE components.
/// This happens when an agent reconnects after a timeout period.
/// We send RunChecksNow to trigger immediate health checks and resync state.
async fn resync_unreachable_components(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    // Find agents with:
    // 1. Recent heartbeat (within timeout)
    // 2. At least one component in UNREACHABLE state
    // 3. Gateway is active
    let agents_to_resync = sqlx::query_as::<_, AgentToResync>(
        r#"
        SELECT a.id AS agent_id, COUNT(c.id) AS unreachable_count
        FROM agents a
        JOIN organizations o ON o.id = a.organization_id
        JOIN components c ON c.agent_id = a.id
        LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE a.is_active = true
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at >= now() - (o.heartbeat_timeout_seconds || ' seconds')::interval
          AND c.current_state = 'UNREACHABLE'
          AND (g.id IS NULL OR g.is_active = true)
          AND (g.id IS NULL OR g.status = 'active')
        GROUP BY a.id
        HAVING COUNT(c.id) > 0
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    if agents_to_resync.is_empty() {
        return Ok(());
    }

    tracing::info!(
        agents_count = agents_to_resync.len(),
        "Detected active agents with UNREACHABLE components — triggering resync"
    );

    for agent in &agents_to_resync {
        tracing::info!(
            agent_id = %agent.agent_id,
            unreachable_count = agent.unreachable_count,
            "Sending RunChecksNow to resync UNREACHABLE components"
        );

        // Use the websocket module's send_run_checks_now function
        crate::websocket::send_run_checks_now(state, *agent.agent_id);
    }

    Ok(())
}

#[cfg(test)]
#[cfg(feature = "postgres")]
mod tests {
    use super::*;

    #[test]
    fn test_stale_component_struct() {
        // Basic struct construction test
        let comp = StaleComponent {
            component_id: DbUuid::new_v4(),
            component_name: "test-component".to_string(),
            agent_id: DbUuid::new_v4(),
            application_id: DbUuid::new_v4(),
            app_name: "test-app".to_string(),
            agent_blocked: false,
        };
        assert_ne!(comp.component_id, comp.agent_id);
        assert!(!comp.agent_blocked);
        assert_eq!(comp.component_name, "test-component");
        assert_eq!(comp.app_name, "test-app");
    }
}
