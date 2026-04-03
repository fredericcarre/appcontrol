//! Switchover STOP_SOURCE, SYNC, and START_TARGET phases.

use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::repository::switchover_queries as repo;
use crate::AppState;

use super::{SwitchoverError, get_switchover_details};

/// Execute the STOP_SOURCE phase.
pub(crate) async fn execute_stop_source(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let details = get_switchover_details(&state.db, switchover_id).await?;
    let mode = details["mode"].as_str().unwrap_or("FULL");
    let component_ids: Option<Vec<Uuid>> = details["component_ids"].as_array().map(|arr| {
        arr.iter().filter_map(|v| v.as_str().and_then(|s| s.parse().ok())).collect()
    });

    if mode == "SELECTIVE" {
        if let Some(ids) = &component_ids {
            if !ids.is_empty() {
                let dag = crate::core::dag::build_dag(&state.db, app_id).await
                    .map_err(|e| SwitchoverError::Database(e.to_string()))?;
                let mut impacted_set: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
                for &comp_id in ids { impacted_set.extend(dag.find_all_dependents(comp_id)); }

                tracing::info!(app_id = %app_id, mode = mode, selected_count = ids.len(), impacted_count = impacted_set.len(),
                    "Switchover: stopping selected components and impacted branch");
                crate::core::sequencer::execute_stop_subset(state, app_id, &impacted_set).await?;

                let impacted_ids: Vec<Uuid> = impacted_set.iter().copied().collect();
                let current_details = get_switchover_details(&state.db, switchover_id).await?;
                let mut merged = current_details;
                if let Some(obj) = merged.as_object_mut() {
                    obj.insert("impacted_component_ids".to_string(), serde_json::json!(impacted_ids));
                }
                repo::update_switchover_prepare_details(&state.db, switchover_id, merged)
                    .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

                return Ok(serde_json::json!({
                    "source_stopped": true, "mode": mode,
                    "components_selected": ids.len(), "components_impacted": impacted_set.len(),
                }));
            }
        }
    }

    tracing::info!(app_id = %app_id, mode = mode, "Switchover: stopping all source site components");
    crate::core::sequencer::execute_stop(state, app_id).await?;

    let still_running = repo::count_running_non_optional(&state.db, app_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({"source_stopped": true, "mode": mode, "components_still_running": still_running}))
}

/// Execute the SYNC phase.
pub(crate) async fn execute_sync(
    _state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    tracing::info!(app_id = %app_id, switchover_id = %switchover_id,
        "SYNC: skipping integrity checks (source is stopped), proceeding to START_TARGET");
    Ok(serde_json::json!({
        "status": "skipped",
        "reason": "Source components are stopped - integrity checks not applicable during switchover",
    }))
}

/// Execute the START_TARGET phase.
pub(crate) async fn execute_start_target(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let details = get_switchover_details(&state.db, switchover_id).await?;
    let target_site_id: Uuid = details["target_site_id"]
        .as_str().and_then(|s| s.parse().ok())
        .ok_or_else(|| SwitchoverError::Database("Missing target_site_id".to_string()))?;
    let mode = details["mode"].as_str().unwrap_or("FULL");
    let component_ids: Option<Vec<Uuid>> = details["component_ids"].as_array().map(|arr| {
        arr.iter().filter_map(|v| v.as_str().and_then(|s| s.parse().ok())).collect()
    });
    let initiated_by = details["initiated_by"].as_str().and_then(|s| s.parse::<Uuid>().ok()).unwrap_or(Uuid::nil());

    // Find target binding profile
    let target_profile = repo::find_target_profile(&state.db, app_id, target_site_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?
        .ok_or_else(|| SwitchoverError::ValidationFailed(format!("No binding profile found for target site {}", target_site_id)))?;
    let (target_profile_id, target_profile_name) = target_profile;

    // Get current active profile
    let current_profile = repo::get_active_profile(&state.db, app_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
    let source_profile_name = current_profile.as_ref().map(|(_, name)| name.clone()).unwrap_or_else(|| "none".to_string());

    if mode == "FULL" {
        repo::deactivate_all_profiles(&state.db, app_id)
            .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
        repo::activate_profile(&state.db, target_profile_id)
            .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
        repo::update_app_site(&state.db, app_id, target_site_id)
            .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
    }

    // Get and apply mappings
    let mappings = repo::get_profile_mappings(&state.db, target_profile_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let selected_component_ids: Option<std::collections::HashSet<Uuid>> = component_ids.as_ref().map(|ids| ids.iter().copied().collect());

    let mut swapped = 0;
    let mut swapped_component_ids = Vec::new();

    for (comp_name, new_agent_id) in &mappings {
        let comp_info = repo::get_component_for_switchover(&state.db, app_id, comp_name)
            .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if let Some((comp_id, cur_agent, cur_check, cur_start, cur_stop)) = comp_info {
            if mode == "SELECTIVE" {
                if let Some(ref selected) = selected_component_ids {
                    if !selected.contains(&comp_id) { continue; }
                } else { continue; }
            }

            let before = serde_json::json!({"agent_id": cur_agent, "check_cmd": cur_check, "start_cmd": cur_start, "stop_cmd": cur_stop, "profile": source_profile_name});

            let cmd_overrides = repo::get_site_cmd_overrides(&state.db, comp_id, target_site_id)
                .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
            let (check_override, start_override, stop_override) = cmd_overrides.unwrap_or((None, None, None));

            repo::update_component_for_switchover(&state.db, comp_id, new_agent_id, &check_override, &start_override, &stop_override)
                .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

            let after = serde_json::json!({"agent_id": new_agent_id, "check_cmd": check_override.as_ref().or(cur_check.as_ref()), "start_cmd": start_override.as_ref().or(cur_start.as_ref()), "stop_cmd": stop_override.as_ref().or(cur_stop.as_ref()), "profile": target_profile_name});

            let _ = repo::record_switchover_config_version(&state.db, comp_id, initiated_by, before, after).await;

            swapped += 1;
            swapped_component_ids.push(comp_id);
        }
    }

    crate::websocket::push_config_to_affected_agents(state, Some(app_id), None, None).await;

    // Start components
    if mode == "SELECTIVE" {
        let impacted_ids: Option<Vec<Uuid>> = details["impacted_component_ids"].as_array().map(|arr| {
            arr.iter().filter_map(|v| v.as_str().and_then(|s| s.parse().ok())).collect()
        });
        if let Some(ids) = impacted_ids {
            if !ids.is_empty() {
                let impacted_set: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
                crate::core::sequencer::execute_start_subset(state, app_id, &impacted_set).await?;
            }
        } else if !swapped_component_ids.is_empty() {
            let component_set: std::collections::HashSet<Uuid> = swapped_component_ids.iter().map(|id| id.into_inner()).collect();
            crate::core::sequencer::execute_start_subset(state, app_id, &component_set).await?;
        }
    } else {
        crate::core::sequencer::execute_start(state, app_id).await?;
    }

    Ok(serde_json::json!({
        "target_site_id": target_site_id, "source_profile": source_profile_name,
        "target_profile": target_profile_name, "mode": mode,
        "components_swapped": swapped, "started": true,
    }))
}
