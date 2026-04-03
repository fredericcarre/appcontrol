use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::DbUuid;
use crate::AppState;
use appcontrol_common::BackendMessage;

/// Default timeout for rebuild commands (5 minutes).
const REBUILD_CMD_TIMEOUT_SECS: u64 = 300;
/// Default timeout for infra rebuild commands (10 minutes).
const INFRA_CMD_TIMEOUT_SECS: u64 = 600;
/// Polling interval when waiting for command completion.
const POLL_INTERVAL_SECS: u64 = 2;

#[derive(Debug, thiserror::Error)]
pub enum RebuildError {
    #[error("Component {0} is rebuild-protected")]
    ProtectedComponent(Uuid),
    #[error("Database error: {0}")]
    Database(String),
    #[error("DAG error: {0}")]
    Dag(#[from] super::dag::DagError),
    #[error("No rebuild command for component {0}")]
    NoRebuildCommand(Uuid),
    #[error("No agent assigned to component {0}")]
    NoAgent(Uuid),
    #[error("Rebuild failed for component {0}: {1}")]
    ExecutionFailed(Uuid, String),
    #[error("Rebuild suspended: command timed out for component {0}")]
    Timeout(Uuid),
}

/// Build a rebuild plan. Checks protected components and resolves rebuild commands.
pub async fn build_rebuild_plan(
    pool: &crate::db::DbPool,
    app_id: Uuid,
    component_ids: Option<&[Uuid]>,
) -> Result<Value, RebuildError> {
    let targets = fetch_rebuild_targets(pool, app_id, component_ids).await?;

    // Check for protected components
    for (id, _name, protected, _, _, _) in &targets {
        if *protected {
            return Err(RebuildError::ProtectedComponent(id.into_inner()));
        }
    }

    // Build DAG order for rebuild
    let dag = super::dag::build_dag(pool, app_id).await?;
    let levels = dag.topological_levels()?;

    let mut plan_levels = Vec::new();
    for level in &levels {
        let mut level_components = Vec::new();
        for &comp_id in level {
            if let Some((_, name, _, rebuild_cmd, infra_cmd, bastion_agent)) = targets
                .iter()
                .find(|(id, _, _, _, _, _)| id.into_inner() == comp_id)
            {
                level_components.push(serde_json::json!({
                    "component_id": comp_id,
                    "name": name,
                    "rebuild_cmd": rebuild_cmd,
                    "rebuild_infra_cmd": infra_cmd,
                    "rebuild_agent_id": bastion_agent,
                }));
            }
        }
        if !level_components.is_empty() {
            plan_levels.push(level_components);
        }
    }

    Ok(serde_json::json!({
        "levels": plan_levels,
        "total_components": targets.len(),
    }))
}

type RebuildTarget = crate::repository::core_queries::RebuildTarget;

/// Fetch rebuild target components with effective rebuild commands.
async fn fetch_rebuild_targets(
    pool: &crate::db::DbPool,
    app_id: Uuid,
    component_ids: Option<&[Uuid]>,
) -> Result<Vec<RebuildTarget>, RebuildError> {
    if let Some(ids) = component_ids {
        let mut targets = Vec::new();
        for &id in ids {
            let row = crate::repository::core_queries::fetch_rebuild_target_by_id(pool, id)
                .await
                .map_err(|e| RebuildError::Database(e.to_string()))?;
            if let Some(r) = row {
                targets.push(r);
            }
        }
        Ok(targets)
    } else {
        crate::repository::core_queries::fetch_rebuild_targets_for_app(pool, app_id)
            .await
            .map_err(|e| RebuildError::Database(e.to_string()))
    }
}

/// Execute a rebuild plan: stop components in reverse DAG order, run rebuild commands,
/// wait for each to complete, then restart in DAG order.
///
/// Steps per component:
/// 1. Stop the component (if running)
/// 2. Run `rebuild_infra_cmd` on the bastion agent (if defined) — wait for completion
/// 3. Run `rebuild_cmd` on the component's agent — wait for completion
/// 4. Start the component
/// 5. Verify via health check
///
/// On failure: SUSPEND — log the error, do NOT proceed to restart phase.
pub async fn execute_rebuild(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
    component_ids: Option<&[Uuid]>,
    initiated_by: Uuid,
) -> Result<Value, RebuildError> {
    let app_id: Uuid = app_id.into();

    let targets = fetch_rebuild_targets(&state.db, app_id, component_ids).await?;

    // Check for protected components
    for (id, _name, protected, _, _, _) in &targets {
        if *protected {
            return Err(RebuildError::ProtectedComponent(id.into_inner()));
        }
    }

    // Log action BEFORE execution (Critical Rule #3: log before execute)
    let _ = crate::repository::core_queries::insert_rebuild_action_log(
        &state.db, initiated_by, app_id, targets.len(),
    ).await;

    // Build DAG order
    let dag = super::dag::build_dag(&state.db, app_id).await?;
    let levels = dag.topological_levels()?;

    // Collect target IDs for quick lookup
    let target_ids: std::collections::HashSet<Uuid> = targets
        .iter()
        .map(|(id, _, _, _, _, _)| id.into_inner())
        .collect();

    // Phase 1: Stop affected components in reverse DAG order
    let mut reverse_levels = levels.clone();
    reverse_levels.reverse();

    tracing::info!(
        app_id = %app_id,
        targets = target_ids.len(),
        "Rebuild phase 1: stopping affected components"
    );

    for level in &reverse_levels {
        let mut handles = Vec::new();
        for &comp_id in level {
            if !target_ids.contains(&comp_id) {
                continue;
            }
            let state_clone = state.clone();
            handles.push(tokio::spawn(async move {
                super::sequencer::stop_single_component(&state_clone, comp_id).await
            }));
        }
        for handle in handles {
            if let Ok(Err(e)) = handle.await {
                tracing::warn!("Failed to stop component during rebuild: {}", e);
                // Continue — component might already be stopped
            }
        }
    }

    // Phase 2: Execute rebuild commands in DAG order (level by level)
    // WAIT for each command to complete before proceeding.
    let mut rebuild_results = Vec::new();

    tracing::info!(app_id = %app_id, "Rebuild phase 2: executing rebuild commands");

    for level in &levels {
        for &comp_id in level {
            if !target_ids.contains(&comp_id) {
                continue;
            }

            let target = targets
                .iter()
                .find(|(id, _, _, _, _, _)| id.into_inner() == comp_id)
                .unwrap();
            let (_, name, _, rebuild_cmd, infra_cmd, bastion_agent) = target;

            let comp_name = name.clone();
            let agent_id = get_component_agent(&state.db, comp_id).await;

            // Run infrastructure rebuild first (if defined) — WAIT for completion
            if let Some(infra_cmd) = infra_cmd {
                let exec_agent = bastion_agent
                    .map(|b| b.into_inner())
                    .or(agent_id.map(|id| id.into_inner()));
                if let Some(exec_agent_id) = exec_agent {
                    let request_id = Uuid::new_v4();
                    let message = BackendMessage::ExecuteCommand {
                        request_id,
                        component_id: comp_id,
                        command: infra_cmd.clone(),
                        timeout_seconds: INFRA_CMD_TIMEOUT_SECS as u32,
                        exec_mode: "sync".to_string(),
                    };

                    // Record dispatch
                    super::sequencer::record_command_dispatch_public(
                        &state.db,
                        request_id,
                        comp_id,
                        exec_agent_id,
                        "rebuild_infra",
                    )
                    .await;

                    state.ws_hub.send_to_agent(exec_agent_id, message);
                    tracing::info!(
                        component = %comp_name,
                        agent = %exec_agent_id,
                        request_id = %request_id,
                        "Rebuild infra command dispatched — waiting for completion"
                    );

                    // Wait for completion
                    let result =
                        wait_for_command_completion(&state.db, request_id, INFRA_CMD_TIMEOUT_SECS)
                            .await;

                    match result {
                        CommandCompletion::Success => {
                            tracing::info!(component = %comp_name, "Infra rebuild completed successfully");
                        }
                        CommandCompletion::Failed(msg) => {
                            tracing::error!(component = %comp_name, error = %msg, "Infra rebuild FAILED — suspending rebuild");
                            return Err(RebuildError::ExecutionFailed(
                                comp_id,
                                format!("Infra rebuild failed: {}", msg),
                            ));
                        }
                        CommandCompletion::Timeout => {
                            tracing::error!(component = %comp_name, "Infra rebuild TIMED OUT — suspending rebuild");
                            return Err(RebuildError::Timeout(comp_id));
                        }
                    }
                }
            }

            // Run application rebuild command — WAIT for completion
            if let Some(rebuild_cmd) = rebuild_cmd {
                if let Some(agent_id) = agent_id.map(|id| id.into_inner()) {
                    let request_id = Uuid::new_v4();
                    let message = BackendMessage::ExecuteCommand {
                        request_id,
                        component_id: comp_id,
                        command: rebuild_cmd.clone(),
                        timeout_seconds: REBUILD_CMD_TIMEOUT_SECS as u32,
                        exec_mode: "sync".to_string(),
                    };

                    super::sequencer::record_command_dispatch_public(
                        &state.db,
                        request_id,
                        comp_id,
                        agent_id,
                        "rebuild_app",
                    )
                    .await;

                    state.ws_hub.send_to_agent(agent_id, message);
                    tracing::info!(
                        component = %comp_name,
                        agent = %agent_id,
                        request_id = %request_id,
                        "Rebuild command dispatched — waiting for completion"
                    );

                    let result = wait_for_command_completion(
                        &state.db,
                        request_id,
                        REBUILD_CMD_TIMEOUT_SECS,
                    )
                    .await;

                    match result {
                        CommandCompletion::Success => {
                            rebuild_results.push(serde_json::json!({
                                "component_id": comp_id,
                                "name": comp_name,
                                "rebuild_request_id": request_id,
                                "status": "completed",
                            }));
                            tracing::info!(component = %comp_name, "App rebuild completed successfully");
                        }
                        CommandCompletion::Failed(msg) => {
                            tracing::error!(component = %comp_name, error = %msg, "App rebuild FAILED — suspending rebuild");
                            return Err(RebuildError::ExecutionFailed(
                                comp_id,
                                format!("App rebuild failed: {}", msg),
                            ));
                        }
                        CommandCompletion::Timeout => {
                            tracing::error!(component = %comp_name, "App rebuild TIMED OUT — suspending rebuild");
                            return Err(RebuildError::Timeout(comp_id));
                        }
                    }
                } else {
                    tracing::warn!(component = %comp_name, "No agent for rebuild — skipping");
                }
            }
        }
    }

    // Phase 3: Restart components in DAG order (only if ALL rebuilds succeeded)
    tracing::info!(
        app_id = %app_id,
        "Rebuild phase 3: restarting components in DAG order"
    );

    for level in &levels {
        let mut handles = Vec::new();
        for &comp_id in level {
            if !target_ids.contains(&comp_id) {
                continue;
            }
            let state_clone = state.clone();
            handles.push(tokio::spawn(async move {
                super::sequencer::start_single_component(&state_clone, comp_id).await
            }));
        }
        for handle in handles {
            if let Ok(Err(e)) = handle.await {
                tracing::error!(
                    "Failed to restart component after rebuild — suspending: {}",
                    e
                );
                return Err(RebuildError::ExecutionFailed(
                    Uuid::nil(),
                    format!("Restart failed after rebuild: {}", e),
                ));
            }
        }
    }

    Ok(serde_json::json!({
        "status": "completed",
        "components_rebuilt": rebuild_results.len(),
        "results": rebuild_results,
    }))
}

/// Get the agent_id assigned to a component.
async fn get_component_agent(pool: &crate::db::DbPool, component_id: Uuid) -> Option<DbUuid> {
    crate::repository::core_queries::get_component_agent_id(pool, component_id).await
}

/// Result of waiting for a command to complete.
enum CommandCompletion {
    Success,
    Failed(String),
    Timeout,
}

/// Poll the command_executions table until the command completes or times out.
async fn wait_for_command_completion(
    pool: &crate::db::DbPool,
    request_id: Uuid,
    timeout_secs: u64,
) -> CommandCompletion {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        let result = crate::repository::core_queries::get_command_execution_status(pool, request_id).await;

        match result {
            Ok(Some((status, exit_code, stderr))) => {
                match status.as_str() {
                    "completed" => return CommandCompletion::Success,
                    "failed" => {
                        let msg = stderr
                            .unwrap_or_else(|| format!("exit code {}", exit_code.unwrap_or(-1)));
                        return CommandCompletion::Failed(msg);
                    }
                    _ => {
                        // Still dispatched/running — keep polling
                    }
                }
            }
            Ok(None) => {
                // Not yet tracked — keep polling
            }
            Err(e) => {
                tracing::warn!(request_id = %request_id, "Error polling command status: {}", e);
            }
        }

        if std::time::Instant::now() > deadline {
            return CommandCompletion::Timeout;
        }

        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}
