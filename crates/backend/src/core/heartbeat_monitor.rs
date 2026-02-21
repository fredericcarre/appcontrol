//! Background task that monitors agent heartbeats and transitions components
//! to UNREACHABLE when their agent has been silent for too long.
//!
//! This distinguishes "check failed" (FAILED — agent ran the check, it returned error)
//! from "agent unavailable" (UNREACHABLE — no heartbeat, we don't know the real state).

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::AppState;
use appcontrol_common::ComponentState;

/// Row returned when querying for stale agents and their components.
#[derive(Debug, sqlx::FromRow)]
struct StaleComponent {
    component_id: Uuid,
    agent_id: Uuid,
    application_id: Uuid,
}

/// Start the heartbeat monitor background task.
/// Runs every `check_interval` seconds, queries for agents whose last_heartbeat_at
/// exceeds the organization's configured timeout, and transitions their components
/// to UNREACHABLE.
pub async fn run_heartbeat_monitor(state: Arc<AppState>, check_interval: Duration) {
    let mut interval = tokio::time::interval(check_interval);

    loop {
        interval.tick().await;

        if let Err(e) = check_stale_agents(&state).await {
            tracing::error!("Heartbeat monitor error: {}", e);
        }
    }
}

/// Check for agents that have missed their heartbeat timeout and transition
/// their components to UNREACHABLE.
async fn check_stale_agents(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    // Find components whose agent has exceeded the org-level heartbeat timeout
    // and that are NOT already in UNREACHABLE, STOPPED, or STOPPING state.
    let stale_components = sqlx::query_as::<_, StaleComponent>(
        r#"
        SELECT c.id AS component_id, c.agent_id, c.application_id
        FROM components c
        JOIN agents a ON a.id = c.agent_id
        JOIN applications app ON app.id = c.application_id
        JOIN organizations o ON o.id = a.organization_id
        WHERE a.is_active = true
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval
          AND c.agent_id IS NOT NULL
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    if stale_components.is_empty() {
        return Ok(());
    }

    // Group by agent for logging
    let mut agent_ids: Vec<Uuid> = stale_components.iter().map(|c| c.agent_id).collect();
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

        // Transition to UNREACHABLE with 'heartbeat_timeout' trigger
        if let Err(e) = transition_to_unreachable(state, comp, current_state).await {
            tracing::warn!(
                component_id = %comp.component_id,
                "Failed to transition to UNREACHABLE: {}", e
            );
        }
    }

    // Mark stale agents as inactive
    for agent_id in &agent_ids {
        let _ = sqlx::query("UPDATE agents SET is_active = false WHERE id = $1")
            .bind(agent_id)
            .execute(&state.db)
            .await;
    }

    Ok(())
}

/// Transition a single component to UNREACHABLE, recording the previous state
/// in the details for recovery when the agent reconnects.
async fn transition_to_unreachable(
    state: &Arc<AppState>,
    comp: &StaleComponent,
    current_state: ComponentState,
) -> Result<(), crate::core::fsm::FsmError> {
    // Insert state transition (append-only) with heartbeat_timeout trigger
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
        VALUES ($1, $2, 'UNREACHABLE', 'heartbeat_timeout',
                jsonb_build_object('previous_state', $2, 'agent_id', $3::text))
        "#,
    )
    .bind(comp.component_id)
    .bind(current_state.to_string())
    .bind(comp.agent_id.to_string())
    .execute(&state.db)
    .await
    .map_err(|e| crate::core::fsm::FsmError::Database(e.to_string()))?;

    // Push WebSocket event
    state.ws_hub.broadcast(
        comp.application_id,
        appcontrol_common::WsEvent::StateChange {
            component_id: comp.component_id,
            app_id: comp.application_id,
            from: current_state,
            to: ComponentState::Unreachable,
            at: chrono::Utc::now(),
        },
    );

    tracing::info!(
        component_id = %comp.component_id,
        from = %current_state,
        agent_id = %comp.agent_id,
        "Component transitioned to UNREACHABLE (heartbeat timeout)"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stale_component_struct() {
        // Basic struct construction test
        let comp = StaleComponent {
            component_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            application_id: Uuid::new_v4(),
        };
        assert_ne!(comp.component_id, comp.agent_id);
    }
}
