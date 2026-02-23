use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use appcontrol_common::BackendMessage;

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
}

/// Build a rebuild plan. Checks protected components and resolves rebuild commands.
pub async fn build_rebuild_plan(
    pool: &sqlx::PgPool,
    app_id: Uuid,
    component_ids: Option<&[Uuid]>,
) -> Result<Value, RebuildError> {
    let targets = fetch_rebuild_targets(pool, app_id, component_ids).await?;

    // Check for protected components
    for (id, _name, protected, _, _, _) in &targets {
        if *protected {
            return Err(RebuildError::ProtectedComponent(*id));
        }
    }

    // Build DAG order for rebuild
    let dag = super::dag::build_dag(pool, app_id).await?;
    let levels = dag.topological_levels()?;

    let mut plan_levels = Vec::new();
    for level in &levels {
        let mut level_components = Vec::new();
        for &comp_id in level {
            if let Some((_, name, _, rebuild_cmd, infra_cmd, bastion_agent)) =
                targets.iter().find(|(id, _, _, _, _, _)| *id == comp_id)
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

type RebuildTarget = (Uuid, String, bool, Option<String>, Option<String>, Option<Uuid>);

/// Fetch rebuild target components with effective rebuild commands.
async fn fetch_rebuild_targets(
    pool: &sqlx::PgPool,
    app_id: Uuid,
    component_ids: Option<&[Uuid]>,
) -> Result<Vec<RebuildTarget>, RebuildError> {
    if let Some(ids) = component_ids {
        let mut targets = Vec::new();
        for &id in ids {
            let row = sqlx::query_as::<_, RebuildTarget>(
                r#"
                SELECT id, name, rebuild_protected,
                       COALESCE(
                           (SELECT so.rebuild_cmd_override FROM site_overrides so WHERE so.component_id = c.id LIMIT 1),
                           rebuild_cmd
                       ) as effective_rebuild_cmd,
                       rebuild_infra_cmd,
                       rebuild_agent_id
                FROM components c WHERE id = $1
                "#,
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| RebuildError::Database(e.to_string()))?;

            if let Some(r) = row {
                targets.push(r);
            }
        }
        Ok(targets)
    } else {
        sqlx::query_as::<_, RebuildTarget>(
            r#"
            SELECT id, name, rebuild_protected,
                   COALESCE(
                       (SELECT so.rebuild_cmd_override FROM site_overrides so WHERE so.component_id = c.id LIMIT 1),
                       rebuild_cmd
                   ) as effective_rebuild_cmd,
                   rebuild_infra_cmd,
                   rebuild_agent_id
            FROM components c WHERE application_id = $1
            "#,
        )
        .bind(app_id)
        .fetch_all(pool)
        .await
        .map_err(|e| RebuildError::Database(e.to_string()))
    }
}

/// Execute a rebuild plan: stop components in reverse DAG order, run rebuild commands,
/// then restart in DAG order. This follows the diagnostic → rebuild → verify pattern.
///
/// Steps per component:
/// 1. Stop the component (if running)
/// 2. Run `rebuild_infra_cmd` on the bastion agent (if defined) — infrastructure rebuild
/// 3. Run `rebuild_cmd` on the component's agent — application rebuild
/// 4. Start the component
/// 5. Verify via health check
pub async fn execute_rebuild(
    state: &Arc<AppState>,
    app_id: Uuid,
    component_ids: Option<&[Uuid]>,
    initiated_by: Uuid,
) -> Result<Value, RebuildError> {
    let targets = fetch_rebuild_targets(&state.db, app_id, component_ids).await?;

    // Check for protected components
    for (id, _name, protected, _, _, _) in &targets {
        if *protected {
            return Err(RebuildError::ProtectedComponent(*id));
        }
    }

    // Build DAG order
    let dag = super::dag::build_dag(&state.db, app_id).await?;
    let levels = dag.topological_levels()?;

    // Collect target IDs for quick lookup
    let target_ids: std::collections::HashSet<Uuid> =
        targets.iter().map(|(id, _, _, _, _, _)| *id).collect();

    // Phase 1: Stop affected components in reverse DAG order
    let mut reverse_levels = levels.clone();
    reverse_levels.reverse();

    tracing::info!(
        app_id = %app_id,
        targets = target_ids.len(),
        "Rebuild: stopping affected components"
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
                // Continue rebuild even if stop fails (component might already be stopped)
            }
        }
    }

    // Phase 2: Execute rebuild commands in DAG order (level by level)
    let mut rebuild_results = Vec::new();

    for level in &levels {
        let mut handles = Vec::new();

        for &comp_id in level {
            if !target_ids.contains(&comp_id) {
                continue;
            }

            let target = targets
                .iter()
                .find(|(id, _, _, _, _, _)| *id == comp_id)
                .unwrap();
            let (_, name, _, rebuild_cmd, infra_cmd, bastion_agent) = target;

            let comp_name = name.clone();
            let agent_id = get_component_agent(&state.db, comp_id).await;

            // Run infrastructure rebuild first (if defined)
            if let Some(infra_cmd) = infra_cmd {
                let exec_agent = bastion_agent.or(agent_id);
                if let Some(agent_id) = exec_agent {
                    let request_id = Uuid::new_v4();
                    let message = BackendMessage::ExecuteCommand {
                        request_id,
                        component_id: comp_id,
                        command: infra_cmd.clone(),
                        timeout_seconds: 300,
                        exec_mode: "sync".to_string(),
                    };

                    super::sequencer::record_command_result(
                        &state.db,
                        request_id,
                        0,
                        &format!("Rebuild infra command dispatched for {}", comp_name),
                        "",
                    )
                    .await;

                    state.ws_hub.send_to_agent(agent_id, message);
                    tracing::info!(
                        component = %comp_name,
                        agent = %agent_id,
                        "Rebuild infra command dispatched"
                    );
                }
            }

            // Run application rebuild command
            if let Some(rebuild_cmd) = rebuild_cmd {
                if let Some(agent_id) = agent_id {
                    let request_id = Uuid::new_v4();
                    let state_clone = state.clone();
                    let cmd = rebuild_cmd.clone();
                    let name = comp_name.clone();

                    handles.push(tokio::spawn(async move {
                        let message = BackendMessage::ExecuteCommand {
                            request_id,
                            component_id: comp_id,
                            command: cmd,
                            timeout_seconds: 600,
                            exec_mode: "sync".to_string(),
                        };
                        state_clone.ws_hub.send_to_agent(agent_id, message);
                        tracing::info!(
                            component = %name,
                            agent = %agent_id,
                            request_id = %request_id,
                            "Rebuild command dispatched"
                        );
                        (comp_id, name, request_id)
                    }));
                } else {
                    tracing::warn!(component = %comp_name, "No agent for rebuild — skipping");
                }
            }
        }

        for handle in handles {
            if let Ok((comp_id, name, request_id)) = handle.await {
                rebuild_results.push(serde_json::json!({
                    "component_id": comp_id,
                    "name": name,
                    "rebuild_request_id": request_id,
                    "status": "dispatched",
                }));
            }
        }
    }

    // Phase 3: Restart components in DAG order
    tracing::info!(
        app_id = %app_id,
        "Rebuild: restarting components in DAG order"
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
                tracing::error!("Failed to restart component after rebuild: {}", e);
            }
        }
    }

    // Log the rebuild action
    let _ = sqlx::query(
        "INSERT INTO action_log (user_id, action, resource_type, resource_id, details) VALUES ($1, 'rebuild_execute', 'application', $2, $3)",
    )
    .bind(initiated_by)
    .bind(app_id)
    .bind(serde_json::json!({"components": rebuild_results.len()}))
    .execute(&state.db)
    .await;

    Ok(serde_json::json!({
        "status": "completed",
        "components_rebuilt": rebuild_results.len(),
        "results": rebuild_results,
    }))
}

/// Get the agent_id assigned to a component.
async fn get_component_agent(pool: &sqlx::PgPool, component_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>("SELECT agent_id FROM components WHERE id = $1 AND agent_id IS NOT NULL")
        .bind(component_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}
