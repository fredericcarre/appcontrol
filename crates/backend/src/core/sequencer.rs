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
        state.ws_hub.send_to_agent(agent_id, message);
        tracing::info!(
            component_id = %component_id,
            agent_id = %agent_id,
            request_id = %request_id,
            "Start command dispatched to agent (detached)"
        );
    }

    // Wait for component to reach Running state (agent's health check will confirm)
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

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
        state.ws_hub.send_to_agent(agent_id, message);
        tracing::info!(
            component_id = %component_id,
            agent_id = %agent_id,
            request_id = %request_id,
            "Stop command dispatched to agent"
        );
    }

    // Wait for Stopped state
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

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
