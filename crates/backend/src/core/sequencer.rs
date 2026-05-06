use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use super::dag::{self, Dag};
use crate::db::DbUuid;
use crate::AppState;
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
    crate::repository::core_queries::get_component_display_name(pool, component_id).await
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
            let name = crate::repository::core_queries::get_component_name_by_id(pool, comp_id)
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
pub async fn execute_start(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
) -> Result<(), SequencerError> {
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
            let ref_app_id = crate::repository::core_queries::get_component_referenced_app_id(
                &state.db, comp_id,
            )
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

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
            let timeout_secs =
                crate::repository::core_queries::get_app_start_timeout_sum(&state.db, ref_app_id)
                    .await;

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
                let counts =
                    crate::repository::core_queries::get_app_state_counts(&state.db, ref_app_id)
                        .await;

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
pub async fn execute_stop(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
) -> Result<(), SequencerError> {
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
            let ref_app_id = crate::repository::core_queries::get_component_referenced_app_id(
                &state.db, comp_id,
            )
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

            if let Some(ref_id) = ref_app_id {
                // For app-type components, check if the REFERENCED APP has running components
                let running_count =
                    crate::repository::core_queries::count_running_components_in_app(
                        &state.db, ref_id,
                    )
                    .await;

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
            let timeout_secs =
                crate::repository::core_queries::get_app_stop_timeout_sum(&state.db, ref_app_id)
                    .await;

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
                let running_count =
                    crate::repository::core_queries::count_running_components_in_app(
                        &state.db, ref_app_id,
                    )
                    .await;

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
    let info = crate::repository::core_queries::get_start_component_info(&state.db, component_id)
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

    // Manual task: pause the DAG until an operator validates via the API.
    // Open (or reuse) a pending manual_task_validations row, transition the
    // component to STARTING, then poll. Validated → RUNNING. Skipped →
    // RUNNING (the operator decided to advance without claiming success,
    // typically for "this step doesn't apply this time"). Failed → FAILED
    // (kills the level the same way a regular component failure does).
    if info.component_type.as_deref() == Some("manual_task") {
        let app_id = info.application_id.ok_or_else(|| {
            SequencerError::Database("manual_task component has no application_id".into())
        })?;
        return run_manual_task_component(state, component_id, app_id, info.start_timeout_seconds)
            .await;
    }

    // Fan-out cluster: dispatch start to every enabled member instead of the
    // parent, then wait for the aggregate state to become Running. The parent's
    // own start_cmd is treated as a per-member fallback when a member has no
    // override of its own.
    if info.cluster_mode.as_deref() == Some("fan_out") {
        return start_fan_out_component(
            state,
            component_id,
            info.start_cmd.as_deref(),
            info.start_native.as_ref(),
            info.start_timeout_seconds,
            info.cluster_concurrency_mode.as_deref(),
            info.cluster_batch_size,
        )
        .await;
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
    let agent_id: Uuid = match info.agent_id {
        Some(id) => id,
        None => {
            tracing::debug!(
                component_id = %component_id,
                "Skipping component without agent_id"
            );
            return Ok(());
        }
    };

    // Skip if already running (no need to start again)
    let current = super::fsm::get_current_state(&state.db, component_id).await?;
    if current == ComponentState::Running {
        tracing::debug!(component_id = %component_id, "Component already RUNNING, skipping start");
        return Ok(());
    }

    // Transition to Starting for normal components
    super::fsm::transition_component(state, component_id, ComponentState::Starting).await?;
    let timeout_secs = info.start_timeout_seconds;

    // Send start command to the specific agent via its gateway. When the
    // component has a native start spec it overrides the shell `start_cmd`
    // (the agent ignores `command` if `native` is set).
    let request_id = Uuid::new_v4();
    let message = BackendMessage::ExecuteCommand {
        request_id,
        component_id,
        command: start_cmd,
        timeout_seconds: timeout_secs as u32,
        exec_mode: "detached".to_string(),
        cluster_member_id: None,
        native: info.start_native.clone(),
    };

    // Record dispatch in command_executions for audit trail
    crate::repository::core_queries::record_command_dispatch(
        &state.db,
        request_id,
        component_id,
        agent_id,
        "start",
    )
    .await;

    // Send command to agent - fail explicitly if gateway unavailable
    if !state.ws_hub.send_to_agent(agent_id, message) {
        // Revert to previous state since command couldn't be sent
        let _ = super::fsm::transition_component(state, component_id, ComponentState::Failed).await;
        let name = get_component_name(&state.db, component_id).await;
        return Err(SequencerError::GatewayUnavailable { agent_id, name });
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

    wait_for_running(state, component_id, timeout_secs, Some(agent_id)).await
}

/// Block until `component_id` reaches RUNNING (or DEGRADED → ok), FAILED, or
/// timeout. While polling, optionally ping `agent_id` with RunChecksNow every
/// few seconds so the next health check confirms the new state quickly. The
/// fan-out path passes `None` because each member runs its own scheduled
/// checks via the snapshot the agent already has.
async fn wait_for_running(
    state: &Arc<AppState>,
    component_id: Uuid,
    timeout_secs: i32,
    ping_agent: Option<Uuid>,
) -> Result<(), SequencerError> {
    let app_id: Option<Uuid> =
        crate::repository::core_queries::get_component_app_id_uuid(&state.db, component_id)
            .await
            .ok();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);
    let mut last_check_request = std::time::Instant::now();
    const CHECK_REQUEST_INTERVAL_SECS: u64 = 3;

    loop {
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

                if let Some(agent) = ping_agent {
                    if last_check_request.elapsed().as_secs() >= CHECK_REQUEST_INTERVAL_SECS {
                        let _ = state.ws_hub.send_to_agent(
                            agent,
                            BackendMessage::RunChecksNow {
                                request_id: *DbUuid::new_v4(),
                            },
                        );
                        last_check_request = std::time::Instant::now();
                    }
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
    let info = crate::repository::core_queries::get_stop_component_info(&state.db, component_id)
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

    // Fan-out cluster: symmetric to start — dispatch stop to every enabled
    // member, then wait for the aggregate state to become Stopped.
    if info.cluster_mode.as_deref() == Some("fan_out") {
        return stop_fan_out_component(
            state,
            component_id,
            info.application_id,
            info.stop_cmd.as_deref(),
            info.stop_native.as_ref(),
            info.stop_timeout_seconds,
            info.cluster_concurrency_mode.as_deref(),
            info.cluster_batch_size,
        )
        .await;
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
    let agent_id: Uuid = match info.agent_id {
        Some(id) => id,
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

    // Send stop command to the specific agent via its gateway. Native spec
    // overrides the shell `stop_cmd` when set (agent honors `native` first).
    let request_id = Uuid::new_v4();
    let message = BackendMessage::ExecuteCommand {
        request_id,
        component_id,
        command: stop_cmd,
        timeout_seconds: timeout_secs as u32,
        exec_mode: "detached".to_string(),
        cluster_member_id: None,
        native: info.stop_native.clone(),
    };

    // Record dispatch in command_executions for audit trail
    crate::repository::core_queries::record_command_dispatch(
        &state.db,
        request_id,
        component_id,
        agent_id,
        "stop",
    )
    .await;

    // Send command to agent - fail explicitly if gateway unavailable
    if !state.ws_hub.send_to_agent(agent_id, message) {
        // Revert to previous state since command couldn't be sent
        let _ = super::fsm::transition_component(state, component_id, ComponentState::Failed).await;
        let name = get_component_name(&state.db, component_id).await;
        return Err(SequencerError::GatewayUnavailable { agent_id, name });
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
                    // Stop timeout expired. If still in STOPPING, force-transition to STOPPED.
                    // The stop command was sent; if the process is somehow still alive,
                    // the next health check cycle will detect it and transition accordingly.
                    if current == ComponentState::Stopping {
                        tracing::warn!(
                            component_id = %component_id,
                            "Stop timeout expired while STOPPING — forcing transition to STOPPED"
                        );
                        let _ = super::fsm::transition_component(
                            state,
                            component_id,
                            ComponentState::Stopped,
                        )
                        .await;
                        return Ok(());
                    }
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
    crate::repository::core_queries::record_command_dispatch(
        pool,
        request_id,
        component_id,
        agent_id,
        command_type,
    )
    .await;
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
    crate::repository::core_queries::record_command_result(
        pool, request_id, exit_code, stdout, stderr,
    )
    .await;
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

    let dinfo = crate::repository::core_queries::get_dispatch_stop_info(&state.db, component_id)
        .await
        .map_err(|e| SequencerError::Database(e.to_string()))?;

    let agent_id: Uuid = match dinfo.agent_id {
        Some(id) => id,
        None => {
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::NoAgent {
                id: component_id,
                name,
            });
        }
    };

    if let Some(cmd) = dinfo.stop_cmd {
        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: dinfo.stop_timeout_seconds as u32,
            exec_mode: "detached".to_string(),
            cluster_member_id: None,
            native: None,
        };

        crate::repository::core_queries::record_command_dispatch(
            &state.db,
            request_id,
            component_id,
            agent_id,
            "stop",
        )
        .await;

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
            // Timeout waiting for STOPPED — force transition if stuck in STOPPING
            let current = super::fsm::get_current_state(&state.db, component_id).await?;
            if current == ComponentState::Stopping {
                tracing::warn!(
                    component_id = %component_id,
                    "wait_for_stopped: timeout while STOPPING — forcing transition to STOPPED"
                );
                let _ =
                    super::fsm::transition_component(state, component_id, ComponentState::Stopped)
                        .await;
                return Ok(());
            }
            tracing::warn!(component_id = %component_id, state = ?current, "Timeout waiting for STOPPED");
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
        crate::repository::core_queries::get_component_app_id_uuid(&state.db, component_id)
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

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

    let dinfo = crate::repository::core_queries::get_dispatch_stop_info(&state.db, component_id)
        .await
        .map_err(|e| SequencerError::Database(e.to_string()))?;

    let agent_id: Uuid = match dinfo.agent_id {
        Some(id) => id,
        None => {
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::NoAgent {
                id: component_id,
                name,
            });
        }
    };

    let timeout_secs = dinfo.stop_timeout_seconds;

    if let Some(cmd) = dinfo.stop_cmd {
        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: timeout_secs as u32,
            exec_mode: "detached".to_string(),
            cluster_member_id: None,
            native: None,
        };

        crate::repository::core_queries::record_command_dispatch(
            &state.db,
            request_id,
            component_id,
            agent_id,
            "force_stop",
        )
        .await;

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

// ============================================================================
// Fan-out cluster helpers
// ============================================================================
//
// When the DAG sequencer hits a component whose `cluster_mode = 'fan_out'`,
// the parent's own start_cmd is no longer the workload — the workload is the
// set of enabled members, each with its own agent and (optionally) its own
// command override. We dispatch the start/stop to each member, then wait for
// the parent's aggregate state to settle (the FSM aggregates member check
// results into the parent's state via `apply_member_check_result` +
// `derive_component_state`).
//
// Behaviour:
// * No enabled members → log + treat as no-op (no transition). The component
//   stays in whatever state its own check_cmd reports, so demos that haven't
//   added any members yet don't break.
// * Member has neither override nor parent fallback → skip + warn (won't
//   prevent other members from starting).
// * Gateway unavailable for a member during start → fail the whole fan-out
//   (consistent with how `start_single_component` treats a missing gateway).
// * Stop is forgiving: a missing member gateway is logged but doesn't abort.

/// Default batch size when the operator picks 'batched' but leaves
/// `cluster_batch_size` NULL. Tuned to be small enough that a downstream
/// service (DB, auth, LB) doesn't get stomped on by a 200-node tier coming
/// up at once, but large enough that bring-up of a typical 6-30-node tier
/// completes in a reasonable wall-clock time.
const DEFAULT_FAN_OUT_BATCH_SIZE: usize = 10;

fn resolve_batch_size(mode: Option<&str>, configured: Option<i32>, total: usize) -> Option<usize> {
    if mode == Some("batched") {
        let n = configured
            .filter(|n| *n > 0)
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_FAN_OUT_BATCH_SIZE)
            .min(total.max(1));
        Some(n)
    } else {
        None
    }
}

async fn start_fan_out_component(
    state: &Arc<AppState>,
    component_id: Uuid,
    parent_start_cmd: Option<&str>,
    parent_start_native: Option<&appcontrol_common::types::NativeCommand>,
    start_timeout_seconds: i32,
    concurrency_mode: Option<&str>,
    batch_size: Option<i32>,
) -> Result<(), SequencerError> {
    let current = super::fsm::get_current_state(&state.db, component_id).await?;
    if current == ComponentState::Running {
        tracing::debug!(component_id = %component_id, "Fan-out parent already RUNNING, skipping start");
        return Ok(());
    }

    let members = state
        .cluster_member_repo
        .list_by_component(component_id)
        .await
        .map_err(|e| SequencerError::Database(e.to_string()))?;

    let enabled: Vec<_> = members
        .into_iter()
        .filter(|m| m.member.is_enabled)
        .collect();
    if enabled.is_empty() {
        tracing::warn!(
            component_id = %component_id,
            "Fan-out component has no enabled members — nothing to start"
        );
        return Ok(());
    }

    super::fsm::transition_component(state, component_id, ComponentState::Starting).await?;

    let parent_cmd = parent_start_cmd.map(str::trim).filter(|s| !s.is_empty());
    let batch = resolve_batch_size(concurrency_mode, batch_size, enabled.len());

    let mut dispatched = 0usize;
    for (idx, m) in enabled.iter().enumerate() {
        // Native dispatch wins over shell, same precedence as the agent's
        // scheduler: explicit per-member override → templated parent native.
        let native = m
            .member
            .start_native_override
            .clone()
            .or_else(|| {
                parent_start_native.map(|n| {
                    n.templated_for_member(
                        &m.member.hostname,
                        m.member.install_path.as_deref(),
                    )
                })
            });

        let cmd = m
            .member
            .start_cmd_override
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| parent_cmd.map(|s| s.to_string()));

        // Skip only if BOTH paths are empty. With native we don't need a
        // shell command — the agent ignores `command` when `native` is set.
        if native.is_none() && cmd.is_none() {
            tracing::warn!(
                component_id = %component_id,
                member_id = %m.member.id,
                hostname = %m.member.hostname,
                "Skipping member with neither start_native, start_cmd_override nor parent start"
            );
            continue;
        }
        let command = cmd.unwrap_or_default();

        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command,
            timeout_seconds: start_timeout_seconds as u32,
            exec_mode: "detached".to_string(),
            cluster_member_id: Some(m.member.id),
            native,
        };

        crate::repository::core_queries::record_command_dispatch(
            &state.db,
            request_id,
            component_id,
            m.member.agent_id,
            "start",
        )
        .await;

        if !state.ws_hub.send_to_agent(m.member.agent_id, message) {
            let _ =
                super::fsm::transition_component(state, component_id, ComponentState::Failed).await;
            let name = get_component_name(&state.db, component_id).await;
            return Err(SequencerError::GatewayUnavailable {
                agent_id: m.member.agent_id,
                name,
            });
        }

        // Mark the member as STARTING so when its next health check returns
        // exit 0 the FSM transitions Starting → Running cleanly, and on a
        // failed check during the start window the member stays in Starting.
        let _ = state
            .cluster_member_repo
            .upsert_state(m.member.id, "STARTING", 0, 0, None)
            .await;
        dispatched += 1;

        // Batched concurrency: every `batch` dispatches, wait for the
        // members started so far to reach RUNNING (or the start window to
        // expire) before kicking the next batch. We poll the aggregate
        // parent state — which `apply_member_check_result` keeps in sync —
        // rather than tracking each member individually, so a tier that
        // converges well below 100% on its policy (e.g. threshold_pct 80)
        // still flows past the gate.
        if let Some(b) = batch {
            let started_count = idx + 1;
            let last_in_batch = started_count % b == 0;
            let any_left = idx + 1 < enabled.len();
            if last_in_batch && any_left {
                tracing::info!(
                    component_id = %component_id,
                    batch_size = b,
                    started = started_count,
                    total = enabled.len(),
                    "Batched fan-out start: waiting for current batch before next"
                );
                wait_until_aggregate_satisfied(
                    state,
                    component_id,
                    start_timeout_seconds,
                    "RUNNING",
                )
                .await?;
            }
        }
    }

    tracing::info!(
        component_id = %component_id,
        members = dispatched,
        concurrency = ?concurrency_mode,
        batch_size = ?batch,
        "Fan-out start dispatched to members — waiting for aggregate to reach RUNNING"
    );

    // Reuse the per-component wait loop. The parent's current_state is updated
    // by apply_member_check_result whenever a member sends a check result, so
    // wait_for_running converges naturally on the aggregated policy.
    wait_for_running(state, component_id, start_timeout_seconds, None).await
}

/// Block until the parent's aggregated state matches `target_state`, used
/// between batches in `start_fan_out_component` / `stop_fan_out_component`.
/// Honors operation cancellation. Logs and returns Ok on timeout — the next
/// batch still gets a chance.
async fn wait_until_aggregate_satisfied(
    state: &Arc<AppState>,
    component_id: Uuid,
    timeout_secs: i32,
    target_state: &str,
) -> Result<(), SequencerError> {
    let app_id =
        crate::repository::core_queries::get_component_app_id_uuid(&state.db, component_id)
            .await
            .ok();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

    loop {
        if let Some(aid) = app_id {
            if state.operation_lock.is_cancelled_async(aid).await {
                return Err(SequencerError::Cancelled);
            }
        }
        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        if current.to_string().eq_ignore_ascii_case(target_state)
            || current == ComponentState::Failed
            || current == ComponentState::Stopped
        {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            tracing::warn!(
                component_id = %component_id,
                target = %target_state,
                "Batched fan-out: gate timed out, proceeding with next batch anyway"
            );
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn stop_fan_out_component(
    state: &Arc<AppState>,
    component_id: Uuid,
    _application_id: Uuid,
    parent_stop_cmd: Option<&str>,
    parent_stop_native: Option<&appcontrol_common::types::NativeCommand>,
    stop_timeout_seconds: i32,
    concurrency_mode: Option<&str>,
    batch_size: Option<i32>,
) -> Result<(), SequencerError> {
    let current = super::fsm::get_current_state(&state.db, component_id).await?;
    if !matches!(current, ComponentState::Running | ComponentState::Degraded) {
        return Ok(());
    }

    let members = state
        .cluster_member_repo
        .list_by_component(component_id)
        .await
        .map_err(|e| SequencerError::Database(e.to_string()))?;

    let enabled: Vec<_> = members
        .into_iter()
        .filter(|m| m.member.is_enabled)
        .collect();
    if enabled.is_empty() {
        return Ok(());
    }

    super::fsm::transition_component(state, component_id, ComponentState::Stopping).await?;

    let parent_cmd = parent_stop_cmd.map(str::trim).filter(|s| !s.is_empty());
    let batch = resolve_batch_size(concurrency_mode, batch_size, enabled.len());

    for (idx, m) in enabled.iter().enumerate() {
        let native = m
            .member
            .stop_native_override
            .clone()
            .or_else(|| {
                parent_stop_native.map(|n| {
                    n.templated_for_member(
                        &m.member.hostname,
                        m.member.install_path.as_deref(),
                    )
                })
            });

        let cmd = m
            .member
            .stop_cmd_override
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| parent_cmd.map(|s| s.to_string()));

        if native.is_none() && cmd.is_none() {
            tracing::warn!(
                component_id = %component_id,
                member_id = %m.member.id,
                "Skipping member with neither stop_native, stop_cmd_override nor parent stop"
            );
            continue;
        }
        let command = cmd.unwrap_or_default();

        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command,
            timeout_seconds: stop_timeout_seconds as u32,
            exec_mode: "detached".to_string(),
            cluster_member_id: Some(m.member.id),
            native,
        };

        crate::repository::core_queries::record_command_dispatch(
            &state.db,
            request_id,
            component_id,
            m.member.agent_id,
            "stop",
        )
        .await;

        if !state.ws_hub.send_to_agent(m.member.agent_id, message) {
            tracing::warn!(
                component_id = %component_id,
                member_id = %m.member.id,
                agent_id = %m.member.agent_id,
                "Gateway unavailable for member during fan-out stop — continuing"
            );
            continue;
        }

        // Mark the member as STOPPING. Without this, the next scheduled
        // health check would arrive while the member is still RUNNING and
        // map exit 1 → DEGRADED (not STOPPED) — the parent's aggregation
        // would then never converge on STOPPED and the operator would think
        // "Stop did nothing". Setting STOPPING upfront makes
        // `next_state_from_check(Stopping, non_zero) = Stopped` fire as
        // soon as the agent's first post-stop check lands.
        let _ = state
            .cluster_member_repo
            .upsert_state(m.member.id, "STOPPING", 0, 0, None)
            .await;

        // Symmetric to start: throttle through batches of size `b` so a
        // 200-node tier doesn't fire 200 stop_cmds at the same instant.
        if let Some(b) = batch {
            let stopped_count = idx + 1;
            let last_in_batch = stopped_count % b == 0;
            let any_left = idx + 1 < enabled.len();
            if last_in_batch && any_left {
                tracing::info!(
                    component_id = %component_id,
                    batch_size = b,
                    stopped = stopped_count,
                    total = enabled.len(),
                    "Batched fan-out stop: waiting for current batch before next"
                );
                wait_until_aggregate_satisfied(
                    state,
                    component_id,
                    stop_timeout_seconds,
                    "STOPPED",
                )
                .await?;
            }
        }
    }

    wait_for_stopped(state, component_id, stop_timeout_seconds as u64).await
}

// ============================================================================
// Manual task helpers
// ============================================================================
//
// Manual tasks are DAG checkpoints for actions the operator does outside
// AppControl (clicks in F5, pages on-call, asks the DBA, …). The sequencer
// pauses on them: opens a `manual_task_validations` pending row, polls
// until the operator submits POST /components/:id/manual-task/validate,
// then advances or fails the FSM accordingly.
//
// The poll cadence (2 s) is faster than the per-component start poll
// (1 s + RunChecksNow at 3 s) because validation latency is human, not
// machine — operators paste a ticket number, then click. The dominant
// bound is operator typing speed, not network.

async fn run_manual_task_component(
    state: &Arc<AppState>,
    component_id: Uuid,
    application_id: Uuid,
    start_timeout_seconds: i32,
) -> Result<(), SequencerError> {
    let current = super::fsm::get_current_state(&state.db, component_id).await?;
    if current == ComponentState::Running {
        tracing::debug!(component_id = %component_id, "Manual task already RUNNING — skipping");
        return Ok(());
    }
    super::fsm::transition_component(state, component_id, ComponentState::Starting).await?;

    // Reuse the pending operation lock's user_id when available so the
    // audit log shows who triggered the start. Falls back to nil — the
    // close_pending call later attributes the validation to the operator
    // who clicked Validate, not to the sequencer.
    let started_by = state
        .operation_lock
        .get_active(application_id)
        .await
        .ok()
        .flatten()
        .map(|op| op.user_id)
        .unwrap_or(Uuid::nil());

    let _validation_id = crate::repository::manual_tasks::open_pending(
        &state.db,
        component_id,
        application_id,
        started_by,
    )
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    tracing::info!(
        component_id = %component_id,
        application_id = %application_id,
        "Manual task pending — waiting for operator validation"
    );

    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(start_timeout_seconds as u64);
    loop {
        if state
            .operation_lock
            .is_cancelled_async(application_id)
            .await
        {
            return Err(SequencerError::Cancelled);
        }

        let status =
            crate::repository::manual_tasks::latest_pending_status(&state.db, component_id)
                .await
                .map_err(|e| SequencerError::Database(e.to_string()))?
                .unwrap_or_else(|| "pending".into());

        match status.as_str() {
            "validated" | "skipped" => {
                super::fsm::transition_component(state, component_id, ComponentState::Running)
                    .await?;
                return Ok(());
            }
            "failed" => {
                super::fsm::transition_component(state, component_id, ComponentState::Failed)
                    .await?;
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
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}
