use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use super::dag::{self, Dag};
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
    #[error("Component failed: {0}")]
    ComponentFailed(Uuid),
    #[error("No agent assigned to component {0}")]
    NoAgent(Uuid),
}

/// Build a start plan without executing it (for dry_run and display).
pub async fn build_start_plan(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SequencerError> {
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
pub async fn execute_start(state: &Arc<AppState>, app_id: Uuid) -> Result<(), SequencerError> {
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

        tracing::info!(
            "Starting level {} — {} components in parallel",
            level_idx,
            to_start.len()
        );

        // Start all components in this level in parallel
        let mut handles = Vec::new();
        for comp_id in to_start {
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

    Ok(())
}

/// Execute a full stop sequence (reverse DAG order).
/// Only stops components that are RUNNING or DEGRADED.
pub async fn execute_stop(state: &Arc<AppState>, app_id: Uuid) -> Result<(), SequencerError> {
    let dag = dag::build_dag(&state.db, app_id).await?;
    let mut levels = dag.topological_levels()?;
    levels.reverse(); // Stop in reverse order: children first

    for (level_idx, level) in levels.iter().enumerate() {
        let mut to_stop = Vec::new();

        for &comp_id in level {
            let current = super::fsm::get_current_state(&state.db, comp_id).await?;
            if matches!(current, ComponentState::Running | ComponentState::Degraded) {
                to_stop.push(comp_id);
            }
        }

        if to_stop.is_empty() {
            continue;
        }

        tracing::info!(
            "Stopping level {} — {} components in parallel",
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
                    tracing::error!("Component stop failed: {}", e);
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

/// Start a single component: transition to Starting, send start_cmd to agent, wait for Running.
pub async fn start_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    // Transition to Starting
    super::fsm::transition_component(state, component_id, ComponentState::Starting).await?;

    // Get component info: start_cmd, timeout, agent_id
    let row = sqlx::query_as::<_, (Option<String>, i32, Option<Uuid>)>(
        "SELECT start_cmd, start_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    let (start_cmd, timeout_secs, agent_id) = row;
    let agent_id = agent_id.ok_or(SequencerError::NoAgent(component_id))?;

    // Send start command to the specific agent via its gateway
    if let Some(cmd) = start_cmd {
        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: timeout_secs as u32,
            exec_mode: "detached".to_string(),
        };

        // Record dispatch in command_executions for audit trail
        record_command_dispatch(&state.db, request_id, component_id, agent_id, "start").await;

        state.ws_hub.send_to_agent(agent_id, message);
        tracing::info!(
            component_id = %component_id,
            agent_id = %agent_id,
            request_id = %request_id,
            "Start command dispatched to agent (detached)"
        );
    }

    // Wait for component to reach Running state (agent's health check will confirm)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

    loop {
        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            ComponentState::Running => return Ok(()),
            ComponentState::Failed => {
                return Err(SequencerError::ComponentFailed(component_id));
            }
            _ => {
                if std::time::Instant::now() > deadline {
                    let _ = super::fsm::transition_component(
                        state,
                        component_id,
                        ComponentState::Failed,
                    )
                    .await;
                    return Err(SequencerError::ComponentFailed(component_id));
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// Stop a single component: transition to Stopping, send stop_cmd to agent, wait for Stopped.
pub async fn stop_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    let current = super::fsm::get_current_state(&state.db, component_id).await?;

    // Only stop if running or degraded
    if !matches!(current, ComponentState::Running | ComponentState::Degraded) {
        return Ok(());
    }

    super::fsm::transition_component(state, component_id, ComponentState::Stopping).await?;

    let row = sqlx::query_as::<_, (Option<String>, i32, Option<Uuid>)>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    let (stop_cmd, timeout_secs, agent_id) = row;
    let agent_id = agent_id.ok_or(SequencerError::NoAgent(component_id))?;

    // Send stop command to the specific agent via its gateway
    if let Some(cmd) = stop_cmd {
        let request_id = Uuid::new_v4();
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: timeout_secs as u32,
            exec_mode: "detached".to_string(),
        };

        // Record dispatch in command_executions for audit trail
        record_command_dispatch(&state.db, request_id, component_id, agent_id, "stop").await;

        state.ws_hub.send_to_agent(agent_id, message);
        tracing::info!(
            component_id = %component_id,
            agent_id = %agent_id,
            request_id = %request_id,
            "Stop command dispatched to agent"
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
                    return Err(SequencerError::ComponentFailed(component_id));
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
    pool: &sqlx::PgPool,
    request_id: Uuid,
    component_id: Uuid,
    agent_id: Uuid,
    command_type: &str,
) {
    record_command_dispatch(pool, request_id, component_id, agent_id, command_type).await;
}

/// Record a dispatched command in the command_executions table.
async fn record_command_dispatch(
    pool: &sqlx::PgPool,
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
    pool: &sqlx::PgPool,
    request_id: Uuid,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) {
    let status = if exit_code == 0 {
        "completed"
    } else {
        "failed"
    };
    if let Err(e) = sqlx::query(
        "UPDATE command_executions
         SET exit_code = $2, stdout = $3, stderr = $4, status = $5, completed_at = now()
         WHERE request_id = $1",
    )
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

/// Execute a start sequence on a subset of components within an application's DAG.
///
/// Builds a sub-DAG containing only the specified component IDs, computes
/// topological levels, and starts them in order. Components already RUNNING
/// are skipped. Used for "start up to component" and "start with dependencies".
pub async fn execute_start_subset(
    state: &Arc<AppState>,
    app_id: Uuid,
    component_ids: &HashSet<Uuid>,
) -> Result<(), SequencerError> {
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

    let row = sqlx::query_as::<_, (Option<String>, i32, Option<Uuid>)>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    let (stop_cmd, timeout_secs, agent_id) = row;
    let agent_id = agent_id.ok_or(SequencerError::NoAgent(component_id))?;

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
        state.ws_hub.send_to_agent(agent_id, message);

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
            return Err(SequencerError::ComponentFailed(component_id));
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
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_one(&state.db)
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

    let row = sqlx::query_as::<_, (Option<String>, i32, Option<Uuid>)>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(component_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SequencerError::Database(e.to_string()))?;

    let (stop_cmd, timeout_secs, agent_id) = row;
    let agent_id = agent_id.ok_or(SequencerError::NoAgent(component_id))?;

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

        state.ws_hub.send_to_agent(agent_id, message);
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
                    return Err(SequencerError::ComponentFailed(component_id));
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
