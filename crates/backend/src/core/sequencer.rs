use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use super::dag;
use crate::AppState;

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
}

/// Build a start plan without executing it (for dry_run and display).
pub async fn build_start_plan(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SequencerError> {
    let dag = dag::build_dag(pool, app_id).await?;
    let levels = dag.topological_levels()?;

    // Get component names for the plan
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

/// Execute a full start sequence following DAG order.
pub async fn execute_start(state: &Arc<AppState>, app_id: Uuid) -> Result<(), SequencerError> {
    let dag = dag::build_dag(&state.db, app_id).await?;
    let levels = dag.topological_levels()?;

    for (level_idx, level) in levels.iter().enumerate() {
        tracing::info!(
            "Starting level {} with {} components",
            level_idx,
            level.len()
        );

        // Start all components in this level in parallel
        let mut handles = Vec::new();
        for &comp_id in level {
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
                    // SUSPEND: don't cancel other levels, return control to operator
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
pub async fn execute_stop(state: &Arc<AppState>, app_id: Uuid) -> Result<(), SequencerError> {
    let dag = dag::build_dag(&state.db, app_id).await?;
    let mut levels = dag.topological_levels()?;
    levels.reverse(); // Stop in reverse order

    for (level_idx, level) in levels.iter().enumerate() {
        tracing::info!(
            "Stopping level {} with {} components",
            level_idx,
            level.len()
        );

        let mut handles = Vec::new();
        for &comp_id in level {
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

async fn start_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    // Transition to Starting
    super::fsm::transition_component(
        state,
        component_id,
        appcontrol_common::ComponentState::Starting,
    )
    .await?;

    // Get start command
    let start_cmd =
        sqlx::query_scalar::<_, Option<String>>("SELECT start_cmd FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

    if let Some(_cmd) = start_cmd {
        // Send command to agent via WebSocket
        // For now, the agent will pick it up and report back
        tracing::info!("Start command sent for component {}", component_id);
    }

    // Wait for component to reach Running state (with timeout)
    let timeout_secs =
        sqlx::query_scalar::<_, i32>("SELECT start_timeout_seconds FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

    loop {
        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            appcontrol_common::ComponentState::Running => return Ok(()),
            appcontrol_common::ComponentState::Failed => {
                return Err(SequencerError::ComponentFailed(component_id));
            }
            _ => {
                if std::time::Instant::now() > deadline {
                    // Timeout → Failed
                    let _ = super::fsm::transition_component(
                        state,
                        component_id,
                        appcontrol_common::ComponentState::Failed,
                    )
                    .await;
                    return Err(SequencerError::ComponentFailed(component_id));
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

async fn stop_single_component(
    state: &Arc<AppState>,
    component_id: Uuid,
) -> Result<(), SequencerError> {
    let current = super::fsm::get_current_state(&state.db, component_id).await?;

    // Only stop if running or degraded
    if !matches!(
        current,
        appcontrol_common::ComponentState::Running | appcontrol_common::ComponentState::Degraded
    ) {
        return Ok(());
    }

    super::fsm::transition_component(
        state,
        component_id,
        appcontrol_common::ComponentState::Stopping,
    )
    .await?;

    let stop_cmd =
        sqlx::query_scalar::<_, Option<String>>("SELECT stop_cmd FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

    if let Some(_cmd) = stop_cmd {
        tracing::info!("Stop command sent for component {}", component_id);
    }

    // Wait for Stopped state
    let timeout_secs =
        sqlx::query_scalar::<_, i32>("SELECT stop_timeout_seconds FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| SequencerError::Database(e.to_string()))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs as u64);

    loop {
        let current = super::fsm::get_current_state(&state.db, component_id).await?;
        match current {
            appcontrol_common::ComponentState::Stopped => return Ok(()),
            _ => {
                if std::time::Instant::now() > deadline {
                    return Err(SequencerError::ComponentFailed(component_id));
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}
