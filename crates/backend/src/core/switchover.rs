use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, thiserror::Error)]
pub enum SwitchoverError {
    #[error("No active switchover for application")]
    NoActiveSwitchover,
    #[error("Invalid phase transition")]
    InvalidPhase,
    #[error("Database error: {0}")]
    Database(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Sequencer error: {0}")]
    Sequencer(String),
}

impl From<super::sequencer::SequencerError> for SwitchoverError {
    fn from(e: super::sequencer::SequencerError) -> Self {
        SwitchoverError::Sequencer(e.to_string())
    }
}

/// Start a new switchover process (6 phases: PREPARE → VALIDATE → STOP_SOURCE → SYNC → START_TARGET → COMMIT).
pub async fn start_switchover(
    pool: &sqlx::PgPool,
    app_id: Uuid,
    target_site_id: Uuid,
    mode: &str,
    component_ids: Option<Vec<Uuid>>,
    initiated_by: Uuid,
) -> Result<Uuid, SwitchoverError> {
    let switchover_id = Uuid::new_v4();

    // Insert into switchover_log (append-only)
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, 'PREPARE', 'in_progress',
                $3::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .bind(serde_json::json!({
        "target_site_id": target_site_id,
        "mode": mode,
        "component_ids": component_ids,
        "initiated_by": initiated_by,
    }))
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(switchover_id)
}

/// Execute the VALIDATE phase: verify target site agents are reachable and components have valid configs.
async fn execute_validate(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let mut issues = Vec::new();

    // 1. Verify the target site exists and is a DR site
    let target_info = get_switchover_details(&state.db, switchover_id).await?;
    let target_site_id: Uuid = target_info["target_site_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SwitchoverError::ValidationFailed("Missing target_site_id".to_string()))?;

    let site_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE id = $1 AND is_active = true)",
    )
    .bind(target_site_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    if !site_exists {
        issues.push("Target site does not exist or is inactive".to_string());
    }

    // 2. Verify all components have site overrides for the target site (agent + commands)
    let components = sqlx::query_as::<_, (Uuid, String, Option<Uuid>)>(
        "SELECT id, name, agent_id FROM components WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    for (comp_id, comp_name, _) in &components {
        let has_override = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM site_overrides WHERE component_id = $1 AND site_id = $2)",
        )
        .bind(comp_id)
        .bind(target_site_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if !has_override {
            issues.push(format!(
                "Component '{}' has no site override for target site",
                comp_name
            ));
        }
    }

    // 3. Verify agents for the target site are connected (check heartbeats)
    let target_agents = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT DISTINCT so.agent_id_override, a.hostname
        FROM site_overrides so
        JOIN agents a ON a.id = so.agent_id_override
        JOIN components c ON c.id = so.component_id
        WHERE c.application_id = $1 AND so.site_id = $2 AND so.agent_id_override IS NOT NULL
        "#,
    )
    .bind(app_id)
    .bind(target_site_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    for (agent_id, hostname) in &target_agents {
        let last_heartbeat = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
            "SELECT last_heartbeat_at FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        match last_heartbeat {
            Some(hb) if (chrono::Utc::now() - hb).num_seconds() > 120 => {
                issues.push(format!(
                    "Agent '{}' ({}) last heartbeat was {}s ago (stale)",
                    hostname,
                    agent_id,
                    (chrono::Utc::now() - hb).num_seconds()
                ));
            }
            None => {
                issues.push(format!(
                    "Agent '{}' ({}) has never sent a heartbeat",
                    hostname, agent_id
                ));
            }
            _ => {} // Agent is alive
        }
    }

    let validation_result = serde_json::json!({
        "components_checked": components.len(),
        "target_agents_checked": target_agents.len(),
        "issues": issues,
        "valid": issues.is_empty(),
    });

    if !issues.is_empty() {
        return Err(SwitchoverError::ValidationFailed(
            serde_json::to_string(&issues).unwrap_or_default(),
        ));
    }

    Ok(validation_result)
}

/// Execute the STOP_SOURCE phase: stop all components on the source site using the sequencer.
async fn execute_stop_source(
    state: &Arc<AppState>,
    app_id: Uuid,
) -> Result<Value, SwitchoverError> {
    tracing::info!(app_id = %app_id, "Switchover: stopping source site components");

    super::sequencer::execute_stop(state, app_id).await?;

    // Verify all non-optional components are stopped
    let still_running = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM components
        WHERE application_id = $1 AND is_optional = false AND current_state NOT IN ('STOPPED', 'UNKNOWN')
        "#,
    )
    .bind(app_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "source_stopped": true,
        "components_still_running": still_running,
    }))
}

/// Execute the SYNC phase: verify data consistency between source and target.
/// Runs integrity checks on source-side components and WAITS for results.
/// Fails the phase if any integrity check returns a non-zero exit code.
async fn execute_sync(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let details = get_switchover_details(&state.db, switchover_id).await?;
    let target_site_id: Uuid = details["target_site_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SwitchoverError::Database("Missing target_site_id".to_string()))?;

    // Run integrity checks on components that have integrity_check_cmd
    let integrity_components = sqlx::query_as::<_, (Uuid, String, String, Option<Uuid>)>(
        r#"
        SELECT c.id, c.name, c.integrity_check_cmd, c.agent_id
        FROM components c
        WHERE c.application_id = $1 AND c.integrity_check_cmd IS NOT NULL
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    // Dispatch all integrity checks and collect request_ids
    let mut dispatched: Vec<(Uuid, String, Uuid)> = Vec::new(); // (comp_id, name, request_id)
    for (comp_id, name, cmd, agent_id) in &integrity_components {
        if let Some(agent_id) = agent_id {
            let request_id = Uuid::new_v4();
            let message = appcontrol_common::BackendMessage::ExecuteCommand {
                request_id,
                component_id: *comp_id,
                command: cmd.clone(),
                timeout_seconds: 120,
                exec_mode: "sync".to_string(),
            };

            super::sequencer::record_command_dispatch_public(
                &state.db,
                request_id,
                *comp_id,
                *agent_id,
                "integrity_check",
            )
            .await;

            state.ws_hub.send_to_agent(*agent_id, message);
            dispatched.push((*comp_id, name.clone(), request_id));
        }
    }

    if dispatched.is_empty() {
        return Ok(serde_json::json!({
            "target_site_id": target_site_id,
            "integrity_checks": 0,
            "status": "no_checks_configured",
        }));
    }

    tracing::info!(
        app_id = %app_id,
        checks = dispatched.len(),
        "SYNC: waiting for integrity check results"
    );

    // Wait for ALL integrity checks to complete (timeout: 120s per check)
    let timeout_secs: u64 = 120;
    let poll_interval = std::time::Duration::from_secs(2);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    let mut check_results = Vec::new();
    let mut failures = Vec::new();

    for (_comp_id, name, request_id) in &dispatched {
        let result = loop {
            let row = sqlx::query_as::<_, (String, Option<i16>, Option<String>)>(
                "SELECT status, exit_code, stderr FROM command_executions WHERE request_id = $1",
            )
            .bind(request_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            match row {
                Some((status, exit_code, _stderr)) if status == "completed" => {
                    break serde_json::json!({
                        "component": name,
                        "status": "passed",
                        "exit_code": exit_code.unwrap_or(0),
                    });
                }
                Some((status, exit_code, stderr)) if status == "failed" => {
                    let msg =
                        stderr.unwrap_or_else(|| format!("exit code {}", exit_code.unwrap_or(-1)));
                    failures.push(format!("{}: {}", name, msg));
                    break serde_json::json!({
                        "component": name,
                        "status": "failed",
                        "exit_code": exit_code.unwrap_or(-1),
                        "error": msg,
                    });
                }
                _ => {
                    if std::time::Instant::now() > deadline {
                        failures.push(format!("{}: timed out", name));
                        break serde_json::json!({
                            "component": name,
                            "status": "timeout",
                        });
                    }
                    tokio::time::sleep(poll_interval).await;
                }
            }
        };
        check_results.push(result);
    }

    if !failures.is_empty() {
        return Err(SwitchoverError::ValidationFailed(format!(
            "Integrity check failures: {}",
            failures.join("; ")
        )));
    }

    Ok(serde_json::json!({
        "target_site_id": target_site_id,
        "integrity_checks_completed": check_results.len(),
        "all_passed": true,
        "checks": check_results,
    }))
}

/// Execute the START_TARGET phase: swap agent assignments and start components on the target site.
async fn execute_start_target(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let details = get_switchover_details(&state.db, switchover_id).await?;
    let target_site_id: Uuid = details["target_site_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SwitchoverError::Database("Missing target_site_id".to_string()))?;

    // Swap component configurations to use target site overrides
    let overrides = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<String>, Option<String>, Option<String>)>(
        r#"
        SELECT component_id, agent_id_override, check_cmd_override, start_cmd_override, stop_cmd_override
        FROM site_overrides
        WHERE site_id = $1 AND component_id IN (SELECT id FROM components WHERE application_id = $2)
        "#,
    )
    .bind(target_site_id)
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let mut swapped = 0;
    for (comp_id, agent_override, check_override, start_override, stop_override) in &overrides {
        // Save current config as a snapshot before swapping (config_versions)
        let current =
            sqlx::query_as::<_, (Option<Uuid>, Option<String>, Option<String>, Option<String>)>(
                "SELECT agent_id, check_cmd, start_cmd, stop_cmd FROM components WHERE id = $1",
            )
            .bind(comp_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if let Some((cur_agent, cur_check, cur_start, cur_stop)) = current {
            let before = serde_json::json!({
                "agent_id": cur_agent,
                "check_cmd": cur_check,
                "start_cmd": cur_start,
                "stop_cmd": cur_stop,
            });

            // Apply target site overrides with COALESCE (non-null overrides only)
            sqlx::query(
                r#"
                UPDATE components SET
                    agent_id = COALESCE($2, agent_id),
                    check_cmd = COALESCE($3, check_cmd),
                    start_cmd = COALESCE($4, start_cmd),
                    stop_cmd = COALESCE($5, stop_cmd),
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(comp_id)
            .bind(agent_override)
            .bind(check_override)
            .bind(start_override)
            .bind(stop_override)
            .execute(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            let after = serde_json::json!({
                "agent_id": agent_override.or(cur_agent),
                "check_cmd": check_override.as_ref().or(cur_check.as_ref()),
                "start_cmd": start_override.as_ref().or(cur_start.as_ref()),
                "stop_cmd": stop_override.as_ref().or(cur_stop.as_ref()),
            });

            // Record config snapshot (append-only)
            let _ = sqlx::query(
                r#"
                INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot)
                VALUES ('component_switchover', $1, $2, $3, $4)
                "#,
            )
            .bind(comp_id)
            .bind(details["initiated_by"].as_str().and_then(|s| s.parse::<Uuid>().ok()).unwrap_or(Uuid::nil()))
            .bind(before)
            .bind(after)
            .execute(&state.db)
            .await;

            swapped += 1;
        }
    }

    // Update application's site_id to the target site
    sqlx::query("UPDATE applications SET site_id = $2, updated_at = now() WHERE id = $1")
        .bind(app_id)
        .bind(target_site_id)
        .execute(&state.db)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    // Now start all components via the sequencer (they'll use the new agent assignments)
    tracing::info!(app_id = %app_id, "Switchover: starting components on target site");
    super::sequencer::execute_start(state, app_id).await?;

    Ok(serde_json::json!({
        "target_site_id": target_site_id,
        "components_swapped": swapped,
        "started": true,
    }))
}

/// Advance to the next phase with real orchestration.
pub async fn advance_phase(state: &Arc<AppState>, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let pool = &state.db;

    // Get current phase
    let current = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"
        SELECT switchover_id, phase, status
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, current_phase, _status) = current;

    let next_phase = match current_phase.as_str() {
        "PREPARE" => "VALIDATE",
        "VALIDATE" => "STOP_SOURCE",
        "STOP_SOURCE" => "SYNC",
        "SYNC" => "START_TARGET",
        "START_TARGET" => "COMMIT",
        _ => return Err(SwitchoverError::InvalidPhase),
    };

    // Execute real orchestration for each phase
    let phase_result = match next_phase {
        "VALIDATE" => execute_validate(state, app_id, switchover_id).await,
        "STOP_SOURCE" => execute_stop_source(state, app_id).await,
        "SYNC" => execute_sync(state, app_id, switchover_id).await,
        "START_TARGET" => execute_start_target(state, app_id, switchover_id).await,
        "COMMIT" => Ok(serde_json::json!({"finalized": true})),
        _ => Ok(serde_json::json!({})),
    };

    match phase_result {
        Ok(details) => {
            // Mark current phase as completed
            sqlx::query(
                r#"
                INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES (gen_random_uuid(), $1, $2, $3, 'completed', $4::jsonb)
                "#,
            )
            .bind(switchover_id)
            .bind(app_id)
            .bind(&current_phase)
            .bind(&details)
            .execute(pool)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            // Start next phase
            sqlx::query(
                r#"
                INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES (gen_random_uuid(), $1, $2, $3, 'in_progress', '{}'::jsonb)
                "#,
            )
            .bind(switchover_id)
            .bind(app_id)
            .bind(next_phase)
            .execute(pool)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            // Send notification
            let db = state.db.clone();
            let event = super::notifications::NotificationEvent::Switchover {
                app_id,
                switchover_id,
                phase: next_phase.to_string(),
                status: "in_progress".to_string(),
            };
            tokio::spawn(async move {
                let _ = super::notifications::dispatch_event(&db, app_id, event).await;
            });

            Ok(serde_json::json!({
                "switchover_id": switchover_id,
                "previous_phase": current_phase,
                "current_phase": next_phase,
                "status": "in_progress",
                "details": details,
            }))
        }
        Err(e) => {
            // Mark phase as failed
            let error_details = serde_json::json!({"error": e.to_string()});
            let _ = sqlx::query(
                r#"
                INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES (gen_random_uuid(), $1, $2, $3, 'failed', $4::jsonb)
                "#,
            )
            .bind(switchover_id)
            .bind(app_id)
            .bind(&current_phase)
            .bind(&error_details)
            .execute(pool)
            .await;

            Err(e)
        }
    }
}

/// Rollback the switchover.
pub async fn rollback(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let current = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT switchover_id, phase
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;

    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, 'ROLLBACK', 'completed',
                $3::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .bind(serde_json::json!({"rolled_back_from": phase}))
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "status": "rolled_back",
        "rolled_back_from": phase,
    }))
}

/// Commit the switchover (final phase).
pub async fn commit(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let current = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT switchover_id, phase
        FROM switchover_log
        WHERE application_id = $1 AND status = 'in_progress'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;

    if phase != "COMMIT" {
        return Err(SwitchoverError::InvalidPhase);
    }

    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, 'COMMIT', 'completed', '{}'::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "status": "committed",
    }))
}

/// Get switchover status.
pub async fn get_status(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let logs = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT switchover_id, phase, status, created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .bind(app_id)
    .fetch_all(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    if logs.is_empty() {
        return Ok(serde_json::json!({"status": "no_switchover"}));
    }

    let (switchover_id, current_phase, current_status, _) = &logs[0];

    let phases: Vec<Value> = logs.iter().map(|(_, phase, status, at)| {
        serde_json::json!({"phase": phase, "status": status, "at": at})
    }).collect();

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "current_phase": current_phase,
        "current_status": current_status,
        "history": phases,
    }))
}

/// Retrieve the details JSON from the PREPARE phase entry for a switchover.
async fn get_switchover_details(
    pool: &sqlx::PgPool,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    sqlx::query_scalar::<_, Value>(
        r#"
        SELECT details FROM switchover_log
        WHERE switchover_id = $1 AND phase = 'PREPARE'
        ORDER BY created_at ASC
        LIMIT 1
        "#,
    )
    .bind(switchover_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)
}
