use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbJson, DbUuid};
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
    pool: &crate::db::DbPool,
    app_id: Uuid,
    target_site_id: Uuid,
    mode: &str,
    component_ids: Option<Vec<Uuid>>,
    initiated_by: Uuid,
) -> Result<Uuid, SwitchoverError> {
    let switchover_id = Uuid::new_v4();

    // Insert into switchover_log (append-only)
    let row_id = DbUuid::new_v4();
    let details_json = DbJson::from(serde_json::json!({
        "target_site_id": target_site_id,
        "mode": mode,
        "component_ids": component_ids,
        "initiated_by": initiated_by,
    }));
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, 'PREPARE', 'in_progress', $4)
        "#,
    )
    .bind(row_id)
    .bind(DbUuid::from(switchover_id))
    .bind(DbUuid::from(app_id))
    .bind(&details_json)
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(switchover_id)
}

/// Execute the VALIDATE phase: verify target binding profile exists and agents are reachable.
async fn execute_validate(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let mut issues = Vec::new();

    // 1. Verify the target site exists and is active
    let target_info = get_switchover_details(&state.db, switchover_id).await?;
    let target_site_id: Uuid = target_info["target_site_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SwitchoverError::ValidationFailed("Missing target_site_id".to_string()))?;

    let site_info =
        sqlx::query_as::<_, (String, bool)>("SELECT name, is_active FROM sites WHERE id = $1")
            .bind(DbUuid::from(target_site_id))
            .fetch_optional(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    match site_info {
        None => issues.push("Target site does not exist".to_string()),
        Some((_, false)) => issues.push("Target site is inactive".to_string()),
        _ => {}
    }

    // 2. Verify a binding profile exists for the target site
    #[cfg(feature = "postgres")]
    let profile_sql: &str = r#"
        SELECT bp.id, bp.name,
               (SELECT COUNT(*) FROM binding_profile_mappings WHERE profile_id = bp.id) as mapping_count
        FROM binding_profiles bp
        WHERE bp.application_id = $1
          AND EXISTS (
            SELECT 1 FROM unnest(bp.gateway_ids) AS gw_id
            JOIN gateways g ON g.id = gw_id
            WHERE g.site_id = $2
          )
        LIMIT 1
        "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let profile_sql: &str = r#"
        SELECT bp.id, bp.name,
               (SELECT COUNT(*) FROM binding_profile_mappings WHERE profile_id = bp.id) as mapping_count
        FROM binding_profiles bp
        WHERE bp.application_id = $1
          AND EXISTS (
            SELECT 1 FROM json_each(bp.gateway_ids) AS gw
            JOIN gateways g ON g.id = gw.value
            WHERE g.site_id = $2
          )
        LIMIT 1
        "#;
    let target_profile = sqlx::query_as::<_, (DbUuid, String, i64)>(profile_sql)
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(target_site_id))
    .fetch_optional(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let (target_profile_id, target_profile_name) = match target_profile {
        Some((id, name, count)) => {
            if count == 0 {
                issues.push(format!(
                    "Binding profile '{}' has no component mappings",
                    name
                ));
            }
            (id, name)
        }
        None => {
            issues.push(format!(
                "No binding profile found for target site (site_id={})",
                target_site_id
            ));
            return Err(SwitchoverError::ValidationFailed(
                serde_json::to_string(&issues).unwrap_or_default(),
            ));
        }
    };

    // 3. Get all components and verify they have mappings in the target profile
    let components = sqlx::query_as::<_, (DbUuid, String)>(
        "SELECT id, name FROM components WHERE application_id = $1",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let mut components_with_mapping = 0;
    for (_comp_id, comp_name) in &components {
        let has_mapping = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM binding_profile_mappings WHERE profile_id = $1 AND component_name = $2)",
        )
        .bind(target_profile_id)
        .bind(comp_name)
        .fetch_one(&state.db)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if has_mapping {
            components_with_mapping += 1;
        } else {
            // This is a warning, not an error - component might be intentionally disabled on DR
            tracing::warn!(
                app_id = %app_id,
                component = %comp_name,
                profile = %target_profile_name,
                "Component has no mapping in target profile (will be skipped)"
            );
        }
    }

    if components_with_mapping == 0 {
        issues.push("No components have mappings in the target profile".to_string());
    }

    // 4. Verify agents in the target profile are connected (check heartbeats)
    let target_agents = sqlx::query_as::<_, (DbUuid, String)>(
        r#"
        SELECT DISTINCT bpm.agent_id, a.hostname
        FROM binding_profile_mappings bpm
        JOIN agents a ON a.id = bpm.agent_id
        WHERE bpm.profile_id = $1
        "#,
    )
    .bind(&target_profile_id)
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
        "target_profile": target_profile_name,
        "components_total": components.len(),
        "components_with_mapping": components_with_mapping,
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

/// Execute the STOP_SOURCE phase: stop components on the source site using the sequencer.
/// For SELECTIVE mode, stops the specified components AND their dependents (impacted branch).
async fn execute_stop_source(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let details = get_switchover_details(&state.db, switchover_id).await?;
    let mode = details["mode"].as_str().unwrap_or("FULL");
    let component_ids: Option<Vec<Uuid>> = details["component_ids"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
            .collect()
    });

    // Use selective stop for SELECTIVE mode, full stop for FULL mode
    if mode == "SELECTIVE" {
        if let Some(ids) = &component_ids {
            if !ids.is_empty() {
                // Build the DAG to find dependents (impacted branch)
                let dag = super::dag::build_dag(&state.db, app_id)
                    .await
                    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

                // Find all dependents of the selected components
                let mut impacted_set: std::collections::HashSet<Uuid> =
                    ids.iter().copied().collect();
                for &comp_id in ids {
                    let dependents = dag.find_all_dependents(comp_id);
                    impacted_set.extend(dependents);
                }

                tracing::info!(
                    app_id = %app_id,
                    mode = mode,
                    selected_count = ids.len(),
                    impacted_count = impacted_set.len(),
                    "Switchover: stopping selected components and impacted branch"
                );

                super::sequencer::execute_stop_subset(state, app_id, &impacted_set).await?;

                // Store the impacted set for START_TARGET phase
                // Read current details, merge, write back (works for both PG and SQLite)
                let impacted_ids: Vec<Uuid> = impacted_set.iter().copied().collect();
                let current_details = get_switchover_details(&state.db, *switchover_id).await?;
                let mut merged = current_details;
                if let Some(obj) = merged.as_object_mut() {
                    obj.insert("impacted_component_ids".to_string(), serde_json::json!(impacted_ids));
                }
                sqlx::query(
                    r#"
                    UPDATE switchover_log
                    SET details = $2
                    WHERE switchover_id = $1 AND phase = 'PREPARE'
                    "#,
                )
                .bind(DbUuid::from(*switchover_id))
                .bind(DbJson::from(merged))
                .execute(&state.db)
                .await
                .map_err(|e| SwitchoverError::Database(e.to_string()))?;

                return Ok(serde_json::json!({
                    "source_stopped": true,
                    "mode": mode,
                    "components_selected": ids.len(),
                    "components_impacted": impacted_set.len(),
                }));
            }
        }
    }

    // FULL mode: stop all components
    tracing::info!(
        app_id = %app_id,
        mode = mode,
        "Switchover: stopping all source site components"
    );
    super::sequencer::execute_stop(state, app_id).await?;

    // Verify all non-optional components are stopped
    #[cfg(feature = "postgres")]
    let running_sql: &str = "SELECT COUNT(*) FROM components \
        WHERE application_id = $1 AND is_optional = false AND current_state NOT IN ('STOPPED', 'UNKNOWN')";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let running_sql: &str = "SELECT COUNT(*) FROM components \
        WHERE application_id = $1 AND is_optional = 0 AND current_state NOT IN ('STOPPED', 'UNKNOWN')";

    let still_running = sqlx::query_scalar::<_, i64>(running_sql)
    .bind(DbUuid::from(app_id))
    .fetch_one(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "source_stopped": true,
        "mode": mode,
        "components_still_running": still_running,
    }))
}

/// Execute the SYNC phase: placeholder for data synchronization verification.
/// During switchover, the source site is already stopped, so we skip runtime integrity checks.
/// This phase can be extended to verify data replication status, backup completion, etc.
async fn execute_sync(
    _state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    // Note: During switchover, the source components are already stopped (STOP_SOURCE phase).
    // Running integrity_check_cmd on stopped components doesn't make sense.
    //
    // In a production scenario, this phase could:
    // 1. Verify database replication lag is zero
    // 2. Check backup/snapshot completion
    // 3. Verify file sync status
    // 4. Check message queue drain status
    //
    // For now, we just log and proceed. The actual data sync verification
    // should be implemented based on the specific infrastructure.

    tracing::info!(
        app_id = %app_id,
        switchover_id = %switchover_id,
        "SYNC: skipping integrity checks (source is stopped), proceeding to START_TARGET"
    );

    Ok(serde_json::json!({
        "status": "skipped",
        "reason": "Source components are stopped - integrity checks not applicable during switchover",
        "note": "Extend this phase to verify data replication/sync status if needed"
    }))
}

/// Execute the START_TARGET phase: activate target binding profile and start components.
/// For SELECTIVE mode, only updates and starts the specified components.
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

    let mode = details["mode"].as_str().unwrap_or("FULL");

    // Debug: log raw details
    tracing::info!(
        details_raw = %details,
        "DEBUG: Raw switchover details for START_TARGET"
    );

    let component_ids: Option<Vec<Uuid>> = details["component_ids"].as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
            .collect()
    });

    let initiated_by = details["initiated_by"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok())
        .unwrap_or(Uuid::nil());

    tracing::info!(
        app_id = %app_id,
        mode = mode,
        component_count = component_ids.as_ref().map(|c| c.len()),
        "Switchover: starting target site components"
    );

    // 1. Find the target binding profile (profile whose gateways belong to target site)
    #[cfg(feature = "postgres")]
    let target_profile_sql: &str = r#"
        SELECT bp.id, bp.name
        FROM binding_profiles bp
        WHERE bp.application_id = $1
          AND EXISTS (
            SELECT 1 FROM unnest(bp.gateway_ids) AS gw_id
            JOIN gateways g ON g.id = gw_id
            WHERE g.site_id = $2
          )
        LIMIT 1
        "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let target_profile_sql: &str = r#"
        SELECT bp.id, bp.name
        FROM binding_profiles bp
        WHERE bp.application_id = $1
          AND EXISTS (
            SELECT 1 FROM json_each(bp.gateway_ids) AS gw
            JOIN gateways g ON g.id = gw.value
            WHERE g.site_id = $2
          )
        LIMIT 1
        "#;
    let target_profile = sqlx::query_as::<_, (DbUuid, String)>(target_profile_sql)
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(target_site_id))
    .fetch_optional(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or_else(|| {
        SwitchoverError::ValidationFailed(format!(
            "No binding profile found for target site {}",
            target_site_id
        ))
    })?;

    let (target_profile_id, target_profile_name) = target_profile;

    // 2. Get current active profile for snapshot
    #[cfg(feature = "postgres")]
    let active_sql: &str = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let active_sql: &str = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = 1";

    let current_profile = sqlx::query_as::<_, (DbUuid, String)>(active_sql)
    .bind(DbUuid::from(app_id))
    .fetch_optional(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let source_profile_name = current_profile
        .as_ref()
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "none".to_string());

    // For SELECTIVE mode, we DON'T change the active profile (partial switchover)
    // For FULL mode, we switch the entire profile
    if mode == "FULL" {
        // Deactivate all profiles for this app
        #[cfg(feature = "postgres")]
        let deactivate_sql: &str = "UPDATE binding_profiles SET is_active = false WHERE application_id = $1";
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let deactivate_sql: &str = "UPDATE binding_profiles SET is_active = 0 WHERE application_id = $1";

        sqlx::query(deactivate_sql)
            .bind(DbUuid::from(app_id))
            .execute(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        // Activate the target profile
        #[cfg(feature = "postgres")]
        let activate_sql: &str = "UPDATE binding_profiles SET is_active = true WHERE id = $1";
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let activate_sql: &str = "UPDATE binding_profiles SET is_active = 1 WHERE id = $1";

        sqlx::query(activate_sql)
            .bind(&target_profile_id)
            .execute(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        tracing::info!(
            app_id = %app_id,
            from_profile = %source_profile_name,
            to_profile = %target_profile_name,
            "Switchover: activated target binding profile (FULL mode)"
        );

        // Update application's site_id to the target site (only for FULL mode)
        sqlx::query(&format!(
            "UPDATE applications SET site_id = $2, updated_at = {} WHERE id = $1",
            crate::db::sql::now()
        ))
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(target_site_id))
        .execute(&state.db)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;
    }

    // 3. Get mappings from the target profile and update components
    let mappings = sqlx::query_as::<_, (String, DbUuid)>(
        "SELECT component_name, agent_id FROM binding_profile_mappings WHERE profile_id = $1",
    )
    .bind(&target_profile_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    // Build set of component IDs to update (all for FULL, selected for SELECTIVE)
    let selected_component_ids: Option<std::collections::HashSet<Uuid>> = component_ids
        .as_ref()
        .map(|ids| ids.iter().copied().collect());

    tracing::info!(
        mode = mode,
        component_ids_count = component_ids.as_ref().map(|c| c.len()),
        selected_set_size = selected_component_ids.as_ref().map(|s| s.len()),
        "DEBUG: Parsed component_ids for selective filtering"
    );

    let mut swapped = 0;
    let mut swapped_component_ids = Vec::new();

    for (comp_name, new_agent_id) in &mappings {
        // Get component id and current agent
        let comp_info = sqlx::query_as::<_, (DbUuid, Option<DbUuid>, Option<String>, Option<String>, Option<String>)>(
            "SELECT id, agent_id, check_cmd, start_cmd, stop_cmd FROM components WHERE application_id = $1 AND name = $2",
        )
        .bind(DbUuid::from(app_id))
        .bind(comp_name)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if let Some((comp_id, cur_agent, cur_check, cur_start, cur_stop)) = comp_info {
            // For SELECTIVE mode, only update selected components
            if mode == "SELECTIVE" {
                if let Some(ref selected) = selected_component_ids {
                    if !selected.contains(&comp_id) {
                        tracing::debug!(
                            comp_id = %comp_id,
                            comp_name = %comp_name,
                            "SELECTIVE: Skipping component (not in selected set)"
                        );
                        continue;
                    }
                } else {
                    // If selected_component_ids is None in SELECTIVE mode, something is wrong
                    tracing::error!(
                        comp_id = %comp_id,
                        "SELECTIVE mode but selected_component_ids is None - skipping ALL updates"
                    );
                    continue;
                }
            }

            // Record config snapshot before change
            let before = serde_json::json!({
                "agent_id": cur_agent,
                "check_cmd": cur_check,
                "start_cmd": cur_start,
                "stop_cmd": cur_stop,
                "profile": source_profile_name,
            });

            // Check for site_overrides (command overrides only)
            let cmd_overrides =
                sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
                    r#"
                SELECT check_cmd_override, start_cmd_override, stop_cmd_override
                FROM site_overrides
                WHERE component_id = $1 AND site_id = $2
                "#,
                )
                .bind(&comp_id)
                .bind(DbUuid::from(target_site_id))
                .fetch_optional(&state.db)
                .await
                .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            let (check_override, start_override, stop_override) =
                cmd_overrides.unwrap_or((None, None, None));

            // Update component with new agent and optional command overrides
            sqlx::query(&format!(
                "UPDATE components SET \
                        agent_id = $2, \
                        check_cmd = COALESCE($3, check_cmd), \
                        start_cmd = COALESCE($4, start_cmd), \
                        stop_cmd = COALESCE($5, stop_cmd), \
                        updated_at = {} \
                    WHERE id = $1",
                crate::db::sql::now()
            ))
            .bind(comp_id)
            .bind(new_agent_id)
            .bind(&check_override)
            .bind(&start_override)
            .bind(&stop_override)
            .execute(&state.db)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            let after = serde_json::json!({
                "agent_id": new_agent_id,
                "check_cmd": check_override.as_ref().or(cur_check.as_ref()),
                "start_cmd": start_override.as_ref().or(cur_start.as_ref()),
                "stop_cmd": stop_override.as_ref().or(cur_stop.as_ref()),
                "profile": target_profile_name,
            });

            // Record config snapshot (append-only)
            let _ = sqlx::query(
                r#"
                INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot)
                VALUES ('component_switchover', $1, $2, $3, $4)
                "#,
            )
            .bind(comp_id)
            .bind(initiated_by)
            .bind(before)
            .bind(after)
            .execute(&state.db)
            .await;

            swapped += 1;
            swapped_component_ids.push(comp_id);
        }
    }

    // 4. Push config updates to all affected agents
    tracing::info!(app_id = %app_id, "Switchover: pushing config updates to affected agents");
    crate::websocket::push_config_to_affected_agents(state, Some(app_id), None, None).await;

    // 5. Start components via the sequencer
    if mode == "SELECTIVE" {
        // Get the impacted component IDs (selected + dependents) stored in STOP_SOURCE phase
        let impacted_ids: Option<Vec<Uuid>> =
            details["impacted_component_ids"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
                    .collect()
            });

        if let Some(ids) = impacted_ids {
            if !ids.is_empty() {
                let impacted_set: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
                tracing::info!(
                    app_id = %app_id,
                    mode = mode,
                    swapped_count = swapped,
                    impacted_count = impacted_set.len(),
                    "Switchover: starting swapped components and impacted branch"
                );
                super::sequencer::execute_start_subset(state, app_id, &impacted_set).await?;
            }
        } else if !swapped_component_ids.is_empty() {
            // Fallback: just start swapped components if impacted_ids not found
            let component_set: std::collections::HashSet<Uuid> = swapped_component_ids
                .iter()
                .map(|id| id.into_inner())
                .collect();
            super::sequencer::execute_start_subset(state, app_id, &component_set).await?;
        }
    } else {
        // FULL mode: Start all components
        tracing::info!(
            app_id = %app_id,
            mode = mode,
            "Switchover: starting all components on target site"
        );
        super::sequencer::execute_start(state, app_id).await?;
    }

    Ok(serde_json::json!({
        "target_site_id": target_site_id,
        "source_profile": source_profile_name,
        "target_profile": target_profile_name,
        "mode": mode,
        "components_swapped": swapped,
        "started": true,
    }))
}

/// Advance to the next phase with real orchestration.
/// The phase that's currently "in_progress" is the one we execute.
pub async fn advance_phase(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let pool = &state.db;

    // Get the phase that's currently in_progress - that's what we need to execute
    let current = sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"
        SELECT switchover_id, phase, status
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

    let (switchover_id, active_phase, _status) = current;

    // Execute the logic for the currently active phase
    let phase_result = match active_phase.as_str() {
        "PREPARE" => {
            // PREPARE is just initialization, nothing to execute
            Ok(serde_json::json!({"status": "prepared"}))
        }
        "VALIDATE" => execute_validate(state, app_id, *switchover_id).await,
        "STOP_SOURCE" => execute_stop_source(state, app_id, *switchover_id).await,
        "SYNC" => execute_sync(state, app_id, *switchover_id).await,
        "START_TARGET" => execute_start_target(state, app_id, *switchover_id).await,
        "COMMIT" => Ok(serde_json::json!({"finalized": true})),
        _ => return Err(SwitchoverError::InvalidPhase),
    };

    // Determine the next phase
    let next_phase = match active_phase.as_str() {
        "PREPARE" => Some("VALIDATE"),
        "VALIDATE" => Some("STOP_SOURCE"),
        "STOP_SOURCE" => Some("SYNC"),
        "SYNC" => Some("START_TARGET"),
        "START_TARGET" => Some("COMMIT"),
        "COMMIT" => None, // No more phases
        _ => None,
    };

    match phase_result {
        Ok(details) => {
            // Mark the active phase as completed with its results
            sqlx::query(
                r#"
                INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES (gen_random_uuid(), $1, $2, $3, 'completed', $4::jsonb)
                "#,
            )
            .bind(switchover_id)
            .bind(app_id)
            .bind(&active_phase)
            .bind(&details)
            .execute(pool)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

            // Mark the next phase as in_progress (if any)
            if let Some(next) = next_phase {
                sqlx::query(
                    r#"
                    INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                    VALUES (gen_random_uuid(), $1, $2, $3, 'in_progress', '{}'::jsonb)
                    "#,
                )
                .bind(switchover_id)
                .bind(app_id)
                .bind(next)
                .execute(pool)
                .await
                .map_err(|e| SwitchoverError::Database(e.to_string()))?;
            }

            // Send notification
            let db = state.db.clone();
            let notification_phase = next_phase
                .map(|s| s.to_string())
                .unwrap_or_else(|| "DONE".to_string());
            let notification_status = if next_phase.is_some() {
                "in_progress"
            } else {
                "completed"
            };
            let event = super::notifications::NotificationEvent::Switchover {
                app_id,
                switchover_id: *switchover_id,
                phase: notification_phase.clone(),
                status: notification_status.to_string(),
            };
            tokio::spawn(async move {
                let _ = super::notifications::dispatch_event(&db, app_id, event).await;
            });

            Ok(serde_json::json!({
                "switchover_id": switchover_id,
                "completed_phase": active_phase,
                "next_phase": next_phase,
                "status": notification_status,
                "details": details,
            }))
        }
        Err(e) => {
            // Mark the active phase as failed
            let error_details = serde_json::json!({"error": e.to_string()});
            let _ = sqlx::query(
                r#"
                INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES (gen_random_uuid(), $1, $2, $3, 'failed', $4::jsonb)
                "#,
            )
            .bind(switchover_id)
            .bind(app_id)
            .bind(&active_phase)
            .bind(&error_details)
            .execute(pool)
            .await;

            Err(e)
        }
    }
}

/// Rollback the switchover.
pub async fn rollback(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let current = sqlx::query_as::<_, (DbUuid, String)>(
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
pub async fn commit(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let current = sqlx::query_as::<_, (DbUuid, String)>(
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
pub async fn get_status(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let logs = sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
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
    pool: &crate::db::DbPool,
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
