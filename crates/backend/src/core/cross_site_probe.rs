//! Cross-site probe — detects components running on the passive/wrong site.
//!
//! When a DR binding profile exists for an application, this background task
//! periodically runs the check_cmd on the passive site's agent to detect if
//! the component is unexpectedly running there.
//!
//! This handles the case where someone starts a process manually outside
//! AppControl — the system detects it and alerts the operator.

use crate::db::DbUuid;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::AppState;
use appcontrol_common::{BackendMessage, WsEvent};

/// Interval between cross-site probe cycles (5 minutes).
#[allow(dead_code)]
const PROBE_INTERVAL_SECS: u64 = 300;

/// Row representing a component that should be probed on its passive site.
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct ProbeTarget {
    component_id: DbUuid,
    component_name: String,
    application_id: DbUuid,
    app_name: String,
    /// The check command to run
    check_cmd: String,
    /// The agent currently assigned (active site)
    active_agent_id: DbUuid,
    /// The passive agent from the binding profile mapping
    passive_agent_id: DbUuid,
    /// Active site name (for display)
    active_site_name: String,
    /// Active site ID
    active_site_id: DbUuid,
    /// Passive site name (for display)
    passive_site_name: String,
    /// Passive site ID
    passive_site_id: DbUuid,
}

/// Start the cross-site probe background task.
pub async fn run_cross_site_probe(state: Arc<AppState>, interval: Duration) {
    let mut tick = tokio::time::interval(interval);
    // Skip the first immediate tick
    tick.tick().await;

    loop {
        tick.tick().await;

        if let Err(e) = probe_passive_sites(&state).await {
            tracing::error!("Cross-site probe error: {}", e);
        }
    }
}

/// Main probe logic: find components with DR profiles and check them on passive agents.
async fn probe_passive_sites(state: &Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let targets = get_probe_targets(&state.db).await?;

    if targets.is_empty() {
        return Ok(());
    }

    tracing::debug!("Cross-site probe: checking {} components", targets.len());

    for target in &targets {
        let passive_agent_id: Uuid = target.passive_agent_id.into_inner();

        // Check if the passive agent is connected
        if !state.ws_hub.is_agent_connected(passive_agent_id) {
            tracing::trace!(
                "Cross-site probe: passive agent {} not connected, skipping {}",
                passive_agent_id,
                target.component_name
            );
            continue;
        }

        // Send the check command to the passive agent
        let request_id = Uuid::new_v4();
        let sent = state.ws_hub.send_to_agent(
            passive_agent_id,
            BackendMessage::ExecuteCommand {
                request_id,
                component_id: target.component_id.into_inner(),
                command: target.check_cmd.clone(),
                timeout_seconds: 30,
                exec_mode: "sync".to_string(),
            },
        );

        if !sent {
            continue;
        }

        // Wait for the result with a timeout
        let result = wait_for_command_result(state, request_id, Duration::from_secs(35)).await;

        let component_id: Uuid = target.component_id.into_inner();
        let app_id: Uuid = target.application_id.into_inner();
        let passive_site_id: Uuid = target.passive_site_id.into_inner();

        match result {
            Some(0) => {
                // Component is running on passive site — this is a warning!
                tracing::warn!(
                    "Cross-site probe: {} ({}) detected RUNNING on passive site {}",
                    target.component_name,
                    component_id,
                    target.passive_site_name
                );

                update_passive_status(&state.db, component_id, Some(passive_site_id), "active")
                    .await?;

                // Broadcast alert to frontend
                state.ws_hub.broadcast(
                    app_id,
                    WsEvent::CrossSiteAlert {
                        component_id,
                        app_id,
                        component_name: target.component_name.clone(),
                        app_name: target.app_name.clone(),
                        expected_site: target.active_site_name.clone(),
                        detected_site: target.passive_site_name.clone(),
                        status: "active".to_string(),
                        at: chrono::Utc::now(),
                    },
                );
            }
            Some(_) => {
                // Component is NOT running on passive site — normal state
                // Only update if it was previously flagged
                clear_passive_status_if_active(&state.db, component_id).await?;
            }
            None => {
                // Timeout or error — don't change status
                tracing::debug!(
                    "Cross-site probe: timeout checking {} on passive agent",
                    target.component_name
                );
            }
        }
    }

    Ok(())
}

/// Wait for a command result by polling the check_events or a simple timeout.
/// We use a simple approach: send command and wait for the Ack + result via a channel.
async fn wait_for_command_result(
    state: &Arc<AppState>,
    request_id: Uuid,
    timeout: Duration,
) -> Option<i32> {
    // Use a simple polling approach: check if the result has been received
    // by looking at the pending_probe_results map
    let (tx, rx) = tokio::sync::oneshot::channel::<i32>();

    // Register the callback
    state
        .probe_results
        .insert(request_id, ProbeCallback(Some(tx)));

    // Wait with timeout
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(exit_code)) => {
            state.probe_results.remove(&request_id);
            Some(exit_code)
        }
        _ => {
            state.probe_results.remove(&request_id);
            None
        }
    }
}

/// Callback holder for probe command results.
pub struct ProbeCallback(pub Option<tokio::sync::oneshot::Sender<i32>>);

/// Called when a CommandResult is received that matches a probe request.
pub fn handle_probe_result(state: &AppState, request_id: Uuid, exit_code: i32) -> bool {
    if let Some((_, mut callback)) = state.probe_results.remove(&request_id) {
        if let Some(tx) = callback.0.take() {
            let _ = tx.send(exit_code);
        }
        true
    } else {
        false
    }
}

/// Query: find all components that have a DR binding profile with a different agent.
async fn get_probe_targets(pool: &crate::db::DbPool) -> Result<Vec<ProbeTarget>, sqlx::Error> {
    sqlx::query_as::<_, ProbeTarget>(
        r#"
        SELECT
            c.id AS component_id,
            c.name AS component_name,
            c.application_id,
            a.name AS app_name,
            COALESCE(so.check_cmd_override, c.check_cmd) AS check_cmd,
            c.agent_id AS active_agent_id,
            bpm.agent_id AS passive_agent_id,
            s_active.name AS active_site_name,
            s_active.id AS active_site_id,
            s_passive.name AS passive_site_name,
            s_passive.id AS passive_site_id
        FROM components c
        JOIN applications a ON a.id = c.application_id
        -- Get the active binding profile for this app
        JOIN binding_profiles bp ON bp.application_id = c.application_id AND bp.is_active = false
        -- Get the mapping for this component in the inactive (DR) profile
        JOIN binding_profile_mappings bpm ON bpm.profile_id = bp.id AND bpm.component_name = c.name
        -- Active agent's site
        JOIN agents ag_active ON ag_active.id = c.agent_id
        JOIN gateways gw_active ON gw_active.id = ag_active.gateway_id
        JOIN sites s_active ON s_active.id = gw_active.site_id
        -- Passive agent's site
        JOIN agents ag_passive ON ag_passive.id = bpm.agent_id
        JOIN gateways gw_passive ON gw_passive.id = ag_passive.gateway_id
        JOIN sites s_passive ON s_passive.id = gw_passive.site_id
        -- Only check overrides for the passive site
        LEFT JOIN site_overrides so ON so.component_id = c.id AND so.site_id = s_passive.id
        WHERE c.agent_id IS NOT NULL
          AND c.check_cmd IS NOT NULL
          AND c.agent_id != bpm.agent_id
          AND c.current_state IN ('RUNNING', 'DEGRADED')
        ORDER BY a.name, c.name
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Update the passive site status for a component.
async fn update_passive_status(
    pool: &crate::db::DbPool,
    component_id: Uuid,
    detected_site_id: Option<Uuid>,
    status: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now();
    sqlx::query(
        r#"UPDATE components
           SET detected_site_id = $2,
               passive_site_status = $3,
               passive_check_at = $4
           WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(detected_site_id.map(crate::db::bind_id))
    .bind(status)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Clear passive status only if it was previously 'active' (to avoid unnecessary writes).
async fn clear_passive_status_if_active(
    pool: &crate::db::DbPool,
    component_id: Uuid,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now();
    sqlx::query(
        r#"UPDATE components
           SET passive_site_status = 'inactive',
               detected_site_id = NULL,
               passive_check_at = $2
           WHERE id = $1
             AND passive_site_status = 'active'"#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}
