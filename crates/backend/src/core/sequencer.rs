use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use super::dag::{self, Dag};
use crate::AppState;
use crate::db::DbUuid;
use appcontrol_common::{BackendMessage, ComponentState};

#[derive(Debug, thiserror::Error)]
pub enum SequencerError {
    #[error("DAG error: {0}")]
    Dag(#[from] dag::DagError),
    #[error("FSM error: {0}")]
    Fsm(#[from] super::fsm::FsmError),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Component failed: {name} ({id})")]
    ComponentFailed { id: Uuid, name: String },
    #[error("No agent assigned to component {name} ({id})")]
    NoAgent { id: Uuid, name: String },
    #[error("Gateway unavailable for agent {agent_id} — cannot send command to {name}")]
    GatewayUnavailable { agent_id: Uuid, name: String },
    #[error("Operation cancelled")]
    Cancelled,
}

/// Helper to get a component's display name (display_name or name fallback)
async fn get_component_name(pool: &crate::db::DbPool, component_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(display_name, name) FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| component_id.to_string())
}

/// Build a start plan without executing it (for dry_run and display).
pub async fn build_start_plan(
    pool: &crate::db::DbPool,
    app_id: Uuid,
) -> Result<Value, SequencerError> {
    let dag = dag::build_dag(pool, app_id).await?;
    let levels = dag.topological_levels()?;

    let mut plan_levels = Vec::new();
    for level in &levels {
        let mut level_info = Vec::new();
        for &comp_id in level {
            let name = sqlx::query_scalar::<_, String>("SELECT name FROM components WHERE id = $1")
                .bind(comp_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| SequencerError::Database(e.to_string()))?
                .unwrap_or_else(|| comp_id.to_string());

            level_info.push(serde_json::json!({
                "component_id": comp_id,
                "name": name,
            }));
        }
        plan_levels.push(level_info);
    }

    Ok(serde_json::json!({ "levels": plan_levels, "total_levels": levels.len() }))
}

/// Smart start: walk the DAG, skip RUNNING, handle FAILED (pink branch), start STOPPED.
///
/// AppControl v1 logic:
/// - Components already RUNNING → skip
/// - Component FAILED → stop its dependents first (reverse order), then restart the branch
/// - Component STOPPED/UNKNOWN → start normally
/// - Components at the same level start in parallel
/// - For application-type components, starts the referenced app first
pub async fn execute_start(state: &Arc<AppState>, app_id: impl Into<Uuid>) -> Result<(), SequencerError> {
    let app_id: Uuid = app_id.into();

    // Use internal function with visited set to prevent infinite recursion on cyclic app references
    let mut visited = HashSet::new();
    execute_start_internal(state, app_id, &mut visited).await
}

/// Internal start function with cycle detection via visited set.
async fn execute_start_internal(
    state: &Arc<AppState>,
    app_id: Uuid,
    visited: &mut HashSet<Uuid>,
) -> Result<(), SequencerError> {
    // Cycle detection: if we've already visited this app, skip it
    if !visited.insert(app_id) {
        tracing::warn!(
            app_id = %app_id,
            "Skipping already-visited application (cycle detected in app references)"
        );
        return Ok(());
    }

    // NOTE: Referenced apps (application-type components) are started when their turn
    // comes in the DAG order, not upfront. This ensures proper dependency ordering:
    // e.g., if backend-api depends on postgres-db, postgres-db starts first, then
    // when backend-api's turn comes, its referenced app is started.

    // Start regular components in this app following DAG order
    let dag = dag::build_dag(&state.db, app_id).await?;
    let levels = dag.topological_levels()?;

    // Build reverse adjacency: who depends on whom (parent → children)
    let dependents = build_dependents_map(&dag);

    for (level_idx, level) in levels.iter().enumerate() {
        let mut to_start = Vec::new();

        for &comp_id in level {
            let current = super::fsm::get_current_state(&state.db, comp_id).await?;

            match current {
                ComponentState::Running => {
                    // Already running, skip
                    tracing::debug!(component_id = %comp_id, "Already RUNNING, skipping");
                }
                ComponentState::Failed => {
                    // Pink branch root: stop dependents first, then restart
                    tracing::info!(
                        component_id = %comp_id,
                        level = level_idx,
                        "FAILED component detected — stopping dependents (pink branch)"
                    );
                    let branch_deps = find_all_dependents(&dependents, comp_id);
                    stop_branch_dependents(state, &dag, &branch_deps).await?;
                    to_start.push(comp_id);
                }
                ComponentState::Degraded => {
                    // Degraded: restart the component
                    to_start.push(comp_id);
                }
                _ => {
                    // STOPPED, UNKNOWN, STARTING, etc. → start
                    to_start.push(comp_id);
                }
            }
        }

        if to_start.is_empty() {
            continue;
        }

        // Separate application-type components (must be handled sequentially due to recursion)
        // from regular components (can be parallelized)
        let mut app_type_components = Vec::new();
        let mut regular_components = Vec::new();

        for comp_id in to_start {
            let ref_app_id: Option<Uuid> =
                sqlx::query_scalar("SELECT referenced_app_id FROM components WHERE id = $1")
                    .bind(comp_id)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(|e| SequencerError::Database(e.to_string()))?
                    .flatten();

            if let Some(id) = ref_app_id {
                app_type_components.push((comp_id, id));
            } else {
                regular_components.push(comp_id);
            }
        }

        // First, handle application-type components sequentially (they recursively start other apps)
        for (comp_id, ref_app_id) in app_type_components {
            tracing::info!(
                component_id = %comp_id,
                referenced_app_id = %ref_app_id,
                level = level_idx,
                "Starting referenced application (application-type component)"
            );
            // Recursive call - this is in the main async context, not spawned
            Box::pin(execute_start_internal(state, ref_app_id, visited)).await?;

            // Wait for the referenced app to be fully started
            // Use the SUM of all start_timeout_seconds from the referenced app's components
            // (since they start in DAG order, worst case is sequential starting)
            let timeout_secs: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(start_timeout_seconds), 120) FROM components
                 WHERE application_id = $1 AND start_cmd IS NOT NULL",
            )
            .bind(ref_app_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(120);

            tracing::debug!(
                component_id = %comp_id,
                referenced_app_id = %ref_app_id,
                timeout_secs = timeout_secs,
                "Waiting for referenced app to start (timeout based on sum of component timeouts)"
            );

            let deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);
            loop {
                // Check aggregate state of referenced app's components
                #[derive(sqlx::FromRow)]
                struct StateCount {
                    total: i64,
                    running: i64,
                    degraded: i64,
                    failed: i64,
                }
                let counts: StateCount = sqlx::query_as(
                    "SELECT
                        COUNT(*) as total,
                        COUNT(*) FILTER (WHERE current_state = 'RUNNING') as running,
                        COUNT(*) FILTER (WHERE current_state = 'DEGRADED') as degraded,
                        COUNT(*) FILTER (WHERE current_state = 'FAILED') as failed
                     FROM components
                     WHERE application_id = $1",
                )
                .bind(ref_app_id)
                .fetch_one(&state.db)
                .await
                .unwrap_or(StateCount {
                    total: 0,
                    running: 0,
                    degraded: 0,
                    failed: 0,
                });

                // Aggregate state logic:
                // - FAILED if any component is FAILED
                // - RUNNING if all components are RUNNING
                // - DEGRADED if all are RUNNING or DEGRADED (at least one DEGRADED)
                // - Otherwise still starting
                if counts.failed > 0 {
                    let _name = get_component_name(&state.db, comp_id).await;
                    tracing::error!(
                        component_id = %comp_id,
                        referenced_app_id = %ref_app_id,
                        failed_count = counts.failed,
                        "Referenced app has failed components"
                    );
                    let _ = super::sequencer::start_single_component(state, comp_id).await;
                }

                if counts.running == counts.total {
                    tracing::info!(
                        component_id = %comp_id,
                        referenced_app_id = %ref_app_id,
                        "Application-type component: referenced app fully RUNNING"
                    );
                    break;
                }

                if counts.running + counts.degraded == counts.total && counts.degraded > 0 {
                    tracing::warn!(
                        component_id = %comp_id,
                        degraded_count = counts.degraded,
                        "Application-type component: referenced app DEGRADED (proceeding with warning)"
                    );
                    break;
                }

                if std::time::Instant::now() > deadline {
                    let name = get_component_name(&state.db, comp_id).await;
                    tracing::error!(
                        component_id = %comp_id,
                        referenced_app_id = %ref_app_id,
                        running = counts.running,
                        degraded = counts.degraded,
                        total = counts.total,
                        "Timeout waiting for referenced app to start"
                    );
                    return Err(SequencerError::ComponentFailed { id: comp_id, name });
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }

        // Then, start regular components in parallel
        if !regular_components.is_empty() {
            tracing::info!(
                "Starting level {} — {} regular components in parallel",
                level_idx,
                regular_components.len()
            );

            let mut handles = Vec::new();
            for comp_id in regular_components {
                let state_clone = state.clone();
                handles.push(tokio::spawn(async move {
                    start_single_component(&state_clone, comp_id).await
                }));
            }

            // Wait for all to complete
            for handle in handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::error!("Component start failed: {}", e);
                        return Err(e);
                    }
                    Err(e) => {
                        tracing::error!("Task join error: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Execute a full stop sequence (reverse DAG order).
/// Only stops components that are RUNNING or DEGRADED.
/// For application-type components, propagates stop to the referenced app.
pub async fn execute_stop(state: &Arc<AppState>, app_id: impl Into<Uuid>) -> Result<(), SequencerError> {
    let app_id: Uuid = app_id.into();

    // Use internal function with visited set to prevent infinite recursion on cyclic app references
    let mut visited = HashSet::new();
    execute_stop_internal(state, app_id, &mut visited).await
}

/// Internal stop function with cycle detection via visited set.
async fn execute_stop_internal(
    state: &Arc<AppState>,
    app_id: Uuid,
    visited: &mut HashSet<Uuid>,
) -> Result<(), SequencerError> {
    // Cycle detection: if we've already visited this app, skip it
    if !visited.insert(app_id) {
        tracing::warn!(
            app_id = %app_id,
            "Skipping already-visited application (cycle detected in app references)"
        );
        return Ok(());
    }

    // NOTE: Referenced apps (application-type components) are stopped when their turn
    // comes in the reverse DAG order. This ensures proper dependency ordering:
    // e.g., stop frontend first, then api-gateway, then when backend-api's turn comes,
    // its referenced app is stopped.

    // Stop regular components in this app following reverse DAG order
    let dag = dag::build_dag(&state.db, app_id).await?;
    let mut levels = dag.topological_levels()?;
    levels.reverse(); // Stop in reverse order: children first

    tracing::warn!(
        app_id = %app_id,
        level_count = levels.len(),
        "execute_stop_internal: processing {} levels",
        levels.len()
    );

    for (level_idx, level) in levels.iter().enumerate() {
        tracing::warn!(
            app_id = %app_id,
            level_idx = level_idx,
            component_count = level.len(),
            "Processing stop level"
        );

        // Separate application-type components from regular components
        // and check if they need to be stopped
        let mut app_type_components = Vec::new();
        let mut regular_components = Vec::new();

        for &comp_id in level {
            // Check if this is an application-type component
            let ref_app_id: Option<Uuid> =
                sqlx::query_scalar("SELECT referenced_app_id FROM components WHERE id = $1")
                    .bind(comp_id)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(|e| SequencerError::Database(e.to_string()))?
                    .flatten();

            if let Some(ref_id) = ref_app_id {
                // For app-type components, check if the REFERENCED APP has running components
                // (the component's own state may be stale)
                // Exclude components without stop_cmd since they can't be stopped
                let running_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM components
                     WHERE application_id = $1
                     AND current_state IN ('RUNNING', 'DEGRADED', 'STARTING', 'STOPPING')
                     AND stop_cmd IS NOT NULL",
                )
                .bind(ref_id)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);

                tracing::warn!(
                    component_id = %comp_id,
                    referenced_app_id = %ref_id,
                    running_count = running_count,
                    "App-type component check: referenced app has {} running components",
                    running_count
                );

                if running_count > 0 {
                    app_type_components.push((comp_id, ref_id));
                }
            } else {
                // Regular component: check its own state
                let current = super::fsm::get_current_state(&state.db, comp_id).await?;
                if matches!(current, ComponentState::Running | ComponentState::Degraded) {
                    regular_components.push(comp_id);
                }
            }
        }

        tracing::warn!(
            level_idx = level_idx,
            app_type_count = app_type_components.len(),
            regular_count = regular_components.len(),
            "Stop level summary"
        );

        if app_type_components.is_empty() && regular_components.is_empty() {
            continue;
        }

        // First, stop regular components in parallel
        if !regular_components.is_empty() {
            tracing::info!(
                "Stopping level {} — {} regular components in parallel",
                level_idx,
                regular_components.len()
            );

            let mut handles = Vec::new();
            for comp_id in regular_components {
                let state_clone = state.clone();
                handles.push(tokio::spawn(async move {
                    stop_single_component(&state_clone, comp_id).await
                }));
            }

            for handle in handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::error!("Component stop failed: {}", e);
                        return Err(e);
                    }
                    Err(e) => {
                        tracing::error!("Task join error: {}", e);
                    }
                }
            }
        }

        // Then, handle application-type components sequentially (they recursively stop other apps)
        for (comp_id, ref_app_id) in app_type_components {
            tracing::info!(
                component_id = %comp_id,
                referenced_app_id = %ref_app_id,
                level = level_idx,
                "Stopping referenced application (application-type component)"
            );
            // Recursive call - this is in the main async context, not spawned
            Box::pin(execute_stop_internal(state, ref_app_id, visited)).await?;

            // Wait for the referenced app to be fully stopped
            // Use the SUM of all stop_timeout_seconds from the referenced app's components
            // (since they stop in DAG order, worst case is sequential stopping)
            let timeout_secs: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(stop_timeout_seconds), 60) FROM components
                 WHERE application_id = $1 AND stop_cmd IS NOT NULL",
            )
            .bind(ref_app_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(60);

            tracing::debug!(
                component_id = %comp_id,
                referenced_app_id = %ref_app_id,
                timeout_secs = timeout_secs,
                "Waiting for referenced app to stop (timeout based on sum of component timeouts)"
            );

            let deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);
            loop {
                // Check aggregate state of referenced app's components
                // Exclude components without stop_cmd since they can't be stopped
                let running_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM components
                     WHERE application_id = $1
                     AND current_state IN ('RUNNING', 'DEGRADED', 'STARTING', 'STOPPING')
                     AND stop_cmd IS NOT NULL",
                )
                .bind(ref_app_id)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);

                if running_count == 0 {
                    tracing::info!(
                        component_id = %comp_id,
                        referenced_app_id = %ref_app_id,
                        "Application-type component: referenced app fully stopped"
                    );
                    break;
                }

                if std::time::Instant::now() > deadline {
                    let name = get_component_name(&state.db, comp_id).await;
                    tracing::error!(
                        component_id = %comp_id,
                        referenced_app_id = %ref_app_id,
                        running_count = running_count,
                        timeout_secs = timeout_secs,
                        "Timeout waiting for referenced app to stop"
                    );
                    return Err(SequencerError::ComponentFailed { id: comp_id, name });
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }

    Ok(())
}

/// Start a single component: transition to Starting, send start_cmd to agent, wait for Running.
/// For application-type components (with referenced_app_id), they are skipped here because
/// their status is derived from the referenced app. The actual start of the referenced app
/// is handled at the API level (components.rs:start_component).
pub async fn start_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    // Get component info: start_cmd, timeout, agent_id, and referenced_app_id
    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        start_cmd: Option<String>,
        start_timeout_seconds: i32,
        agent_id: Option<crate::db::DbUuid>,
        referenced_app_id: Option<crate::db::DbUuid>,
    }

    let info = sqlx::query_as::<_, ComponentInfo>(
        "SELECT start_cmd, start_timeout_seconds, agent_id, referenced_app_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    // Application-type components are handled in execute_start_internal directly
    // (they recursively start their referenced app). Here we just skip them.
    if info.referenced_app_id.is_some() {
        tracing::debug!(
            component_id = %component_id,
            "Skipping application-type component (handled by execute_start_internal)"
        );
        return Ok(());
    }

    // Components without a start command are skipped - nothing to do
    let start_cmd = match &info.start_cmd {
        Some(cmd) if !cmd.trim().is_empty() => cmd.clone(),
        _ => {
            tracing::debug!(
                component_id = %component_id,
                "Skipping component without start_cmd"
            );
            return Ok(());
        }
    };

    // Check agent BEFORE transitioning to avoid stuck states
    let agent_id: DbUuid = match info.agent_id {
        Some(id) => DbUuid::from(id.into_inner()),
        None => {
            tracing::debug!(
                component_id = %component_id,
                "Skipping component without agent_id"
            );
            return Ok(());
        }
    };

    // Transition to Starting for normal components
    super::fsm::transition_component(state, component_id, ComponentState::Starting).await?;
    let timeout_secs = info.start_timeout_seconds;

    // Send start command to the specific agent via its gateway
    let request_id = Uuid::new_v4();
    let message = BackendMessage::ExecuteCommand {
        request_id,
        component_id,
        command: start_cmd,
        timeout_seconds: timeout_secs as u32,
        exec_mode: "detached".to_string(),
    };

    // Record dispatch in command_executions for audit trail
    record_command_dispatch(&state.db, request_id, component_id, *agent_id, "start").await;

    // Send command to agent - fail explicitly if gateway unavailable
    if !state.ws_hub.send_to_agent(agent_id, message) {
        // Revert to previous state since command couldn't be sent
        let _ = super::fsm::transition_component(state, component_id, ComponentState::Failed).await;
        let name = get_component_name(&state.db, component_id).await;
        return Err(SequencerError::GatewayUnavailable { agent_id: *agent_id, name });
    }

    tracing::info!(
        component_id = %component_id,
        agent_id = %agent_id,
        request_id = %request_id,
        "Start command dispatched to agent (detached)"
    );

    // Trigger immediate health check so we don't wait for the next scheduled check
    let check_request_id = Uuid::new_v4();
    let _ = state.ws_hub.send_to_agent(
        agent_id,
        BackendMessage::RunChecksNow {
            request_id: check_request_id,
        },
    ); // Ignore failure for health check trigger - not critical
    tracing::debug!(
        agent_id = %agent_id,
        request_id = %check_request_id,
        "Triggered immediate health checks after start command"
    );

    // Get the app_id for checking cancellation
    let app_id: Option<DbUuid> =
        sqlx::query_scalar("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    // Wait for component to reach Running state (agent's health check will confirm)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);
    let mut last_check_request = std::time::Instant::now();
    const CHECK_REQUEST_INTERVAL_SECS: u64 = 3; // Request checks more frequently during start

    loop {
        // Check for cancellation
        if let Some(aid) = app_id {
            if state.operation_lock.is_cancelled_async(aid).await {
                tracing::info!(component_id = %component_id, "Start operation cancelled");
                return Err(SequencerError::Cancelled);
            }
        }

        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            ComponentState::Running => return Ok(()),
            ComponentState::Failed => {
                let name = get_component_name(&state.db, component_id).await;
                return Err(SequencerError::ComponentFailed {
                    id: component_id,
                    name,
                });
            }
            _ => {
                if std::time::Instant::now() > deadline {
                    let _ = super::fsm::transition_component(
                        state,
                        component_id,
                        ComponentState::Failed,
                    )
                    .await;
                    let name = get_component_name(&state.db, component_id).await;
                    return Err(SequencerError::ComponentFailed {
                        id: component_id,
                        name,
                    });
                }

                // Request health check every few seconds to detect start faster
                if last_check_request.elapsed().as_secs() >= CHECK_REQUEST_INTERVAL_SECS {
                    let _ = state.ws_hub.send_to_agent(
                        agent_id,
                        BackendMessage::RunChecksNow {
                            request_id: *DbUuid::new_v4(),
                        },
                    );
                    last_check_request = std::time::Instant::now();
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// Stop a single component: transition to Stopping, send stop_cmd to agent, wait for Stopped.
/// For application-type components (with referenced_app_id), they are skipped here because
/// their status is derived from the referenced app. The actual stop of the referenced app
/// is handled at the API level (components.rs:stop_component).
pub async fn stop_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    // Get component info: stop_cmd, timeout, agent_id, referenced_app_id, and application_id
    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        stop_cmd: Option<String>,
        stop_timeout_seconds: i32,
        agent_id: Option<crate::db::DbUuid>,
        referenced_app_id: Option<crate::db::DbUuid>,
        application_id: DbUuid,
    }

    let info = sqlx::query_as::<_, ComponentInfo>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id, referenced_app_id, application_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    // Application-type components are handled in execute_stop_internal directly
    // (they recursively stop their referenced app). Here we just skip them.
    if info.referenced_app_id.is_some() {
        tracing::debug!(
            component_id = %component_id,
            "Skipping application-type component (handled by execute_stop_internal)"
        );
        return Ok(());
    }

    // Components without a stop command are skipped - nothing to do
    let stop_cmd = match &info.stop_cmd {
        Some(cmd) if !cmd.trim().is_empty() => cmd.clone(),
        _ => {
            tracing::debug!(
                component_id = %component_id,
                "Skipping component without stop_cmd"
            );
            return Ok(());
        }
    };

    // Check agent BEFORE transitioning to avoid stuck states
    let agent_id: DbUuid = match info.agent_id {
        Some(id) => DbUuid::from(id.into_inner()),
        None => {
            tracing::debug!(
                component_id = %component_id,
                "Skipping component without agent_id"
            );
            return Ok(());
        }
    };

    let current = super::fsm::get_current_state(&state.db, component_id).await?;

    // Only stop if running or degraded
    if !matches!(current, ComponentState::Running | ComponentState::Degraded) {
        return Ok(());
    }

    super::fsm::transition_component(state, component_id, ComponentState::Stopping).await?;
    let timeout_secs = info.stop_timeout_seconds;

    // Send stop command to the specific agent via its gateway
    let request_id = Uuid::new_v4();
    let message = BackendMessage::ExecuteCommand {
        request_id,
        component_id,
        command: stop_cmd,
        timeout_seconds: timeout_secs as u32,
        exec_mode: "detached".to_string(),
    };

    // Record dispatch in command_executions for audit trail
    record_command_dispatch(&state.db, request_id, component_id, *agent_id, "stop").await;

    // Send command to agent - fail explicitly if gateway unavailable
    if !state.ws_hub.send_to_agent(agent_id, message) {
        // Revert to previous state since command couldn't be sent
        let _ = super::fsm::transition_component(state, component_id, ComponentState::Failed).await;
        let name = get_component_name(&state.db, component_id).await;
        return Err(SequencerError::GatewayUnavailable { agent_id: *agent_id, name });
    }

    tracing::info!(
        component_id = %component_id,
        agent_id = %agent_id,
        request_id = %request_id,
        "Stop command dispatched to agent"
    );

    // Trigger immediate health check so we don't wait for the next scheduled check
    let check_request_id = Uuid::new_v4();
    let _ = state.ws_hub.send_to_agent(
        agent_id,
        BackendMessage::RunChecksNow {
            request_id: check_request_id,
        },
    ); // Ignore failure for health check trigger - not critical
    tracing::debug!(
        agent_id = %agent_id,
        request_id = %check_request_id,
        "Triggered immediate health checks after stop command"
    );

    // Wait for Stopped state
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);
    let app_id = info.application_id;
    let mut last_check_request = std::time::Instant::now();
    const CHECK_REQUEST_INTERVAL_SECS: u64 = 3; // Request checks more frequently during stop

    loop {
        // Check for cancellation
        if state.operation_lock.is_cancelled_async(app_id).await {
            tracing::info!(component_id = %component_id, "Stop operation cancelled");
            return Err(SequencerError::Cancelled);
        }

        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            ComponentState::Stopped => return Ok(()),
            _ => {
                if std::time::Instant::now() > deadline {
                    let name = get_component_name(&state.db, component_id).await;
                    return Err(SequencerError::ComponentFailed {
                        id: component_id,
                        name,
                    });
                }

                // Request health check every few seconds to detect stop faster
                if last_check_request.elapsed().as_secs() >= CHECK_REQUEST_INTERVAL_SECS {
                    let _ = state.ws_hub.send_to_agent(
                        agent_id,
                        BackendMessage::RunChecksNow {
                            request_id: *DbUuid::new_v4(),
                        },
                    );
                    last_check_request = std::time::Instant::now();
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command execution tracking (audit trail)
// ---------------------------------------------------------------------------

/// Record a dispatched command in the command_executions table (public variant for rebuild/switchover).
pub async fn record_command_dispatch_public(
    pool: &crate::db::DbPool,
    request_id: Uuid,
    component_id: Uuid,
    agent_id: Uuid,
    command_type: &str,
) {
    record_command_dispatch(pool, request_id, component_id, agent_id, command_type).await;
}

/// Record a dispatched command in the command_executions table.
async fn record_command_dispatch(
    pool: &crate::db::DbPool,
    request_id: Uuid,
    component_id: Uuid,
    agent_id: Uuid,
    command_type: &str,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO command_executions (request_id, component_id, agent_id, command_type, status)
         VALUES ($1, $2, $3, $4, 'dispatched')
         ON CONFLICT (request_id) DO NOTHING",
    )
    .bind(request_id)
    .bind(component_id)
    .bind(agent_id)
    .bind(command_type)
    .execute(pool)
    .await
    {
        tracing::warn!(
            request_id = %request_id,
            "Failed to record command dispatch: {}", e
        );
    }
}

/// Record a command result in the command_executions table.
pub async fn record_command_result(
    pool: &crate::db::DbPool,
    request_id: impl Into<Uuid>,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) {
    let request_id: Uuid = request_id.into();

    let status = if exit_code == 0 {
        "completed"
    } else {
        "failed"
    };
    if let Err(e) = sqlx::query(&format!(
        "UPDATE command_executions \
             SET exit_code = $2, stdout = $3, stderr = $4, status = $5, completed_at = {} \
             WHERE request_id = $1",
        crate::db::sql::now()
    ))
    .bind(request_id)
    .bind(exit_code as i16)
    .bind(stdout)
    .bind(stderr)
    .bind(status)
    .execute(pool)
    .await
    {
        tracing::warn!(
            request_id = %request_id,
            "Failed to record command result: {}", e
        );
    }
}

/// Execute a stop sequence on a subset of components within an application's DAG.
///
/// Builds a sub-DAG containing only the specified component IDs, computes
/// topological levels in reverse order, and stops them. Components already
/// STOPPED are skipped. Used for selective switchover.
pub async fn execute_stop_subset(
    state: &Arc<AppState>,
    app_id: Uuid,
    component_ids: &HashSet<Uuid>,
) -> Result<(), SequencerError> {
    let full_dag = dag::build_dag(&state.db, app_id).await?;
    let sub = full_dag.sub_dag(component_ids);
    let mut levels = sub.topological_levels()?;
    levels.reverse(); // Stop in reverse order: dependents first

    for (level_idx, level) in levels.iter().enumerate() {
        let mut to_stop = Vec::new();

        for &comp_id in level {
            let current = super::fsm::get_current_state(&state.db, comp_id).await?;

            match current {
                ComponentState::Running | ComponentState::Degraded => {
                    to_stop.push(comp_id);
                }
                _ => {
                    tracing::debug!(
                        component_id = %comp_id,
                        state = ?current,
                        "Already stopped or not running, skipping"
                    );
                }
            }
        }

        if to_stop.is_empty() {
            continue;
        }

        tracing::info!(
            "Stopping subset level {} — {} components in parallel",
            level_idx,
            to_stop.len()
        );

        let mut handles = Vec::new();
        for comp_id in to_stop {
            let state_clone = state.clone();
            handles.push(tokio::spawn(async move {
                stop_single_component(&state_clone, comp_id).await
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::error!("Component stop failed in subset: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    tracing::error!("Task join error: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Execute a start sequence on a subset of components within an application's DAG.
///
/// Builds a sub-DAG containing only the specified component IDs, computes
/// topological levels, and starts them in order. Components already RUNNING
/// are skipped. Used for "start up to component" and "start with dependencies".
pub async fn execute_start_subset(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
    component_ids: &HashSet<Uuid>,
) -> Result<(), SequencerError> {
    let app_id: Uuid = app_id.into();

    let full_dag = dag::build_dag(&state.db, app_id).await?;
    let sub = full_dag.sub_dag(component_ids);
    let levels = sub.topological_levels()?;

    let dependents = build_dependents_map(&full_dag);

    for (level_idx, level) in levels.iter().enumerate() {
        let mut to_start = Vec::new();

        for &comp_id in level {
            let current = super::fsm::get_current_state(&state.db, comp_id).await?;

            match current {
                ComponentState::Running => {
                    tracing::debug!(
                        component_id = %comp_id,
                        "Already RUNNING, skipping"
                    );
                }
                ComponentState::Failed => {
                    tracing::info!(
                        component_id = %comp_id,
                        level = level_idx,
                        "FAILED component in subset — stopping dependents first"
                    );
                    let branch_deps = find_all_dependents(&dependents, comp_id);
                    // Only stop dependents that are in our subset
                    let subset_deps: HashSet<Uuid> =
                        branch_deps.intersection(component_ids).copied().collect();
                    stop_branch_dependents(state, &full_dag, &subset_deps).await?;
                    to_start.push(comp_id);
                }
                _ => {
                    to_start.push(comp_id);
                }
            }
        }

        if to_start.is_empty() {
            continue;
        }

        tracing::info!(
            "Starting subset level {} — {} components in parallel",
            level_idx,
            to_start.len()
        );

        let mut handles = Vec::new();
        for comp_id in to_start {
            let state_clone = state.clone();
            handles.push(tokio::spawn(async move {
                start_single_component(&state_clone, comp_id).await
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::error!("Component start failed in subset: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    tracing::error!("Task join error: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Dispatch a stop command to a component without waiting for completion.
/// Transitions to STOPPING and sends the stop_cmd, but returns immediately.
async fn dispatch_stop(state: &Arc<AppState>, component_id: Uuid) -> Result<(), SequencerError> {
    let current = super::fsm::get_current_state(&state.db, component_id).await?;

    // Only stop if running or degraded
    if !matches!(current, ComponentState::Running | ComponentState::Degraded) {
        tracing::debug!(component_id = %component_id, state = ?current, "Skipping stop - not running");
        return Ok(());
    }

    super::fsm::transition_component(state, component_id, ComponentState::Stopping).await?;

    let row = sqlx::query_as::<_, (Option<String>, i32, Option<crate::db::DbUuid>)>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    let (stop_cmd, timeout_secs, raw_agent_id) = row;
    let agent_id: Uuid = match raw_agent_id {
        Some(id) => id.into_inner(),
        None => {
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::NoAgent {
                id: component_id,
                name,
            });
        }
    };

    if let Some(cmd) = stop_cmd {
        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: timeout_secs as u32,
            exec_mode: "detached".to_string(),
        };

        record_command_dispatch(&state.db, request_id, component_id, agent_id, "stop").await;

        if !state.ws_hub.send_to_agent(agent_id, message) {
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::GatewayUnavailable { agent_id, name });
        }

        tracing::info!(
            component_id = %component_id,
            agent_id = %agent_id,
            request_id = %request_id,
            "Stop command dispatched"
        );
    }

    Ok(())
}

/// Wait for a component to reach STOPPED state with a given timeout.
async fn wait_for_stopped(
    state: &Arc<AppState>,
    component_id: Uuid,
    timeout_secs: u64,
) -> Result<(), SequencerError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            ComponentState::Stopped => return Ok(()),
            ComponentState::Stopping => {
                // Still stopping, keep waiting
            }
            _ => {
                // Unexpected state
                tracing::warn!(component_id = %component_id, state = ?current, "Unexpected state while waiting for STOPPED");
            }
        }

        if std::time::Instant::now() > deadline {
            tracing::warn!(component_id = %component_id, "Timeout waiting for STOPPED");
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::ComponentFailed {
                id: component_id,
                name,
            });
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Stop a component and all its dependents in correct DAG order.
///
/// This is the proper way to stop a component: first stop all components that
/// depend on it (in reverse topological order), then stop the target component.
/// For example, if PostgreSQL is depended upon by UserService and OrderService,
/// those services will be stopped first before PostgreSQL is stopped.
///
/// This function waits for each level to be fully STOPPED before proceeding
/// to the next level. The timeout per component is based on its check_interval_seconds
/// plus its stop_timeout_seconds to ensure the health check has time to detect the stop.
pub async fn stop_with_dependents(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    // Get the application ID for this component
    let app_id: Uuid =
        sqlx::query_scalar::<_, crate::db::DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?
            .into_inner();

    // Build the DAG for the application
    let dag = super::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| SequencerError::Database(e.to_string()))?;

    // Find all components that depend on this one (transitively)
    let dependents = dag.find_all_dependents(component_id);

    tracing::info!(
        component_id = %component_id,
        dependent_count = dependents.len(),
        "Stopping component with dependents"
    );

    // Build a sub-DAG containing only the target component and its dependents
    let mut subset = dependents.clone();
    subset.insert(component_id);
    let sub_dag = dag.sub_dag(&subset);

    // Get topological levels and reverse them for stop order
    let levels = sub_dag
        .topological_levels()
        .map_err(|e| SequencerError::Database(e.to_string()))?;

    // Stop in reverse order (dependents first, then dependencies)
    for level in levels.into_iter().rev() {
        // Only stop components that are actually running
        let running_in_level: Vec<Uuid> = {
            let mut running = Vec::new();
            for comp_id in level {
                let current = super::fsm::get_current_state(&state.db, comp_id).await?;
                if matches!(current, ComponentState::Running | ComponentState::Degraded) {
                    running.push(comp_id);
                }
            }
            running
        };

        if running_in_level.is_empty() {
            continue;
        }

        tracing::info!(
            target_component = %component_id,
            level_size = running_in_level.len(),
            components = ?running_in_level,
            "Stopping components in level (reverse DAG order)"
        );

        // First, dispatch stop commands for all components in this level in parallel
        let mut dispatch_tasks = Vec::new();
        for comp_id in &running_in_level {
            let state_clone = state.clone();
            let comp_id = *comp_id;
            dispatch_tasks.push(tokio::spawn(async move {
                dispatch_stop(&state_clone, comp_id).await
            }));
        }

        // Wait for all stop commands to be dispatched
        for task in dispatch_tasks {
            if let Err(e) = task.await {
                tracing::error!("Dispatch task join error: {}", e);
            }
        }

        // Then, wait for all components in this level to reach STOPPED
        // Use a generous timeout: check_interval (30s default) + stop_timeout + buffer
        let wait_timeout_secs = 90; // 30s check interval + 60s buffer

        let mut wait_tasks = Vec::new();
        for comp_id in running_in_level {
            let state_clone = state.clone();
            wait_tasks.push(tokio::spawn(async move {
                wait_for_stopped(&state_clone, comp_id, wait_timeout_secs).await
            }));
        }

        // Wait for all components to stop
        let mut all_stopped = true;
        for task in wait_tasks {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::error!("Wait for stop failed: {}", e);
                    all_stopped = false;
                    // Continue with other components
                }
                Err(e) => {
                    tracing::error!("Wait task join error: {}", e);
                    all_stopped = false;
                }
            }
        }

        if !all_stopped {
            tracing::warn!("Some components failed to stop, continuing with next level anyway");
        }
    }

    Ok(())
}

/// Force-stop a single component: bypass FSM rules, send stop_cmd to agent.
///
/// Used for production incidents where you need to kill a component immediately
/// without respecting DAG dependencies or FSM state machine rules.
pub async fn force_stop_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    // Force transition to Stopping regardless of current state
    super::fsm::force_transition_component(state, component_id, ComponentState::Stopping).await?;

    let row = sqlx::query_as::<_, (Option<String>, i32, Option<crate::db::DbUuid>)>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    let (stop_cmd, timeout_secs, raw_agent_id) = row;
    let agent_id: Uuid = match raw_agent_id {
        Some(id) => id.into_inner(),
        None => {
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::NoAgent {
                id: component_id,
                name,
            });
        }
    };

    if let Some(cmd) = stop_cmd {
        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: timeout_secs as u32,
            exec_mode: "detached".to_string(),
        };

        record_command_dispatch(&state.db, request_id, component_id, agent_id, "force_stop").await;

        if !state.ws_hub.send_to_agent(agent_id, message) {
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::GatewayUnavailable { agent_id, name });
        }

        tracing::warn!(
            component_id = %component_id,
            agent_id = %agent_id,
            request_id = %request_id,
            "FORCE STOP command dispatched to agent (bypassing dependencies)"
        );
    }

    // Wait for Stopped state
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

    loop {
        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            ComponentState::Stopped => return Ok(()),
            _ => {
                if std::time::Instant::now() > deadline {
                    // Force to FAILED on timeout
                    let _ = super::fsm::force_transition_component(
                        state,
                        component_id,
                        ComponentState::Failed,
                    )
                    .await;
                    let name = get_component_name(&state.db, component_id).await;
                    return Err(SequencerError::ComponentFailed {
                        id: component_id,
                        name,
                    });
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pink branch helpers
// ---------------------------------------------------------------------------

type DependentsMap = std::collections::HashMap<Uuid, HashSet<Uuid>>;

/// Build reverse adjacency: for each component, which components depend on it.
fn build_dependents_map(dag: &Dag) -> DependentsMap {
    let mut dependents: DependentsMap = std::collections::HashMap::new();
    for (&node, deps) in &dag.adjacency {
        for &dep in deps {
            dependents.entry(dep).or_default().insert(node);
        }
    }
    dependents
}

/// BFS from a root component through its dependents (children, grandchildren, ...).
fn find_all_dependents(dependents: &DependentsMap, root: Uuid) -> HashSet<Uuid> {
    let mut affected = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root);

    while let Some(current) = queue.pop_front() {
        if let Some(deps) = dependents.get(&current) {
            for &dep in deps {
                if affected.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }
    }

    affected // does NOT include root itself
}

/// Stop the dependents of a failed component in reverse DAG order.
/// Only stops those that are RUNNING or DEGRADED.
async fn stop_branch_dependents(
    state: &Arc<AppState>,
    dag: &Dag,
    dependents: &HashSet<Uuid>,
) -> Result<(), SequencerError> {
    if dependents.is_empty() {
        return Ok(());
    }

    // Build a sub-DAG with just the dependent components
    let levels = dag.topological_levels()?;
    let mut reversed_levels: Vec<Vec<Uuid>> = levels
        .into_iter()
        .map(|level| {
            level
                .into_iter()
                .filter(|id| dependents.contains(id))
                .collect::<Vec<_>>()
        })
        .filter(|level| !level.is_empty())
        .collect();
    reversed_levels.reverse(); // Stop children first

    for level in reversed_levels {
        let mut handles = Vec::new();
        for comp_id in level {
            let state_clone = state.clone();
            handles.push(tokio::spawn(async move {
                stop_single_component(&state_clone, comp_id).await
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::error!("Failed to stop branch dependent: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    tracing::error!("Task join error: {}", e);
                }
            }
        }
    }

    Ok(())
}
