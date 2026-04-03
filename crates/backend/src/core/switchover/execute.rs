//! Switchover STOP_SOURCE, SYNC, and START_TARGET phases.

use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbJson, DbUuid};
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
                sqlx::query("UPDATE switchover_log SET details = $2 WHERE switchover_id = $1 AND phase = 'PREPARE'")
                    .bind(DbUuid::from(switchover_id)).bind(DbJson::from(merged))
                    .execute(&state.db).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

                return Ok(serde_json::json!({
                    "source_stopped": true, "mode": mode,
                    "components_selected": ids.len(), "components_impacted": impacted_set.len(),
                }));
            }
        }
    }

    tracing::info!(app_id = %app_id, mode = mode, "Switchover: stopping all source site components");
    crate::core::sequencer::execute_stop(state, app_id).await?;

    #[cfg(feature = "postgres")]
    let running_sql: &str = "SELECT COUNT(*) FROM components WHERE application_id = $1 AND is_optional = false AND current_state NOT IN ('STOPPED', 'UNKNOWN')";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let running_sql: &str = "SELECT COUNT(*) FROM components WHERE application_id = $1 AND is_optional = 0 AND current_state NOT IN ('STOPPED', 'UNKNOWN')";

    let still_running = sqlx::query_scalar::<_, i64>(running_sql)
        .bind(DbUuid::from(app_id)).fetch_one(&state.db).await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

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
    #[cfg(feature = "postgres")]
    let target_profile_sql: &str = r#"SELECT bp.id, bp.name FROM binding_profiles bp WHERE bp.application_id = $1
        AND EXISTS (SELECT 1 FROM unnest(bp.gateway_ids) AS gw_id JOIN gateways g ON g.id = gw_id WHERE g.site_id = $2) LIMIT 1"#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let target_profile_sql: &str = r#"SELECT bp.id, bp.name FROM binding_profiles bp WHERE bp.application_id = $1
        AND EXISTS (SELECT 1 FROM json_each(bp.gateway_ids) AS gw JOIN gateways g ON g.id = gw.value WHERE g.site_id = $2) LIMIT 1"#;

    let target_profile = sqlx::query_as::<_, (DbUuid, String)>(target_profile_sql)
        .bind(DbUuid::from(app_id)).bind(DbUuid::from(target_site_id))
        .fetch_optional(&state.db).await.map_err(|e| SwitchoverError::Database(e.to_string()))?
        .ok_or_else(|| SwitchoverError::ValidationFailed(format!("No binding profile found for target site {}", target_site_id)))?;
    let (target_profile_id, target_profile_name) = target_profile;

    // Get current active profile
    #[cfg(feature = "postgres")]
    let active_sql: &str = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let active_sql: &str = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = 1";
    let current_profile = sqlx::query_as::<_, (DbUuid, String)>(active_sql)
        .bind(DbUuid::from(app_id)).fetch_optional(&state.db).await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;
    let source_profile_name = current_profile.as_ref().map(|(_, name)| name.clone()).unwrap_or_else(|| "none".to_string());

    if mode == "FULL" {
        #[cfg(feature = "postgres")]
        let deactivate_sql: &str = "UPDATE binding_profiles SET is_active = false WHERE application_id = $1";
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let deactivate_sql: &str = "UPDATE binding_profiles SET is_active = 0 WHERE application_id = $1";
        sqlx::query(deactivate_sql).bind(DbUuid::from(app_id)).execute(&state.db).await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        #[cfg(feature = "postgres")]
        let activate_sql: &str = "UPDATE binding_profiles SET is_active = true WHERE id = $1";
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let activate_sql: &str = "UPDATE binding_profiles SET is_active = 1 WHERE id = $1";
        sqlx::query(activate_sql).bind(target_profile_id).execute(&state.db).await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        sqlx::query(&format!("UPDATE applications SET site_id = $2, updated_at = {} WHERE id = $1", crate::db::sql::now()))
            .bind(DbUuid::from(app_id)).bind(DbUuid::from(target_site_id))
            .execute(&state.db).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
    }

    // Get and apply mappings
    let mappings = sqlx::query_as::<_, (String, DbUuid)>(
        "SELECT component_name, agent_id FROM binding_profile_mappings WHERE profile_id = $1",
    ).bind(target_profile_id).fetch_all(&state.db).await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let selected_component_ids: Option<std::collections::HashSet<Uuid>> = component_ids.as_ref().map(|ids| ids.iter().copied().collect());

    let mut swapped = 0;
    let mut swapped_component_ids = Vec::new();

    for (comp_name, new_agent_id) in &mappings {
        let comp_info = sqlx::query_as::<_, (DbUuid, Option<DbUuid>, Option<String>, Option<String>, Option<String>)>(
            "SELECT id, agent_id, check_cmd, start_cmd, stop_cmd FROM components WHERE application_id = $1 AND name = $2",
        ).bind(DbUuid::from(app_id)).bind(comp_name).fetch_optional(&state.db).await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if let Some((comp_id, cur_agent, cur_check, cur_start, cur_stop)) = comp_info {
            if mode == "SELECTIVE" {
                if let Some(ref selected) = selected_component_ids {
                    if !selected.contains(&comp_id) { continue; }
                } else { continue; }
            }

            let before = serde_json::json!({"agent_id": cur_agent, "check_cmd": cur_check, "start_cmd": cur_start, "stop_cmd": cur_stop, "profile": source_profile_name});

            let cmd_overrides = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
                r#"SELECT check_cmd_override, start_cmd_override, stop_cmd_override FROM site_overrides WHERE component_id = $1 AND site_id = $2"#,
            ).bind(crate::db::bind_id(comp_id)).bind(DbUuid::from(target_site_id))
            .fetch_optional(&state.db).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
            let (check_override, start_override, stop_override) = cmd_overrides.unwrap_or((None, None, None));

            sqlx::query(&format!(
                "UPDATE components SET agent_id = $2, check_cmd = COALESCE($3, check_cmd), start_cmd = COALESCE($4, start_cmd), stop_cmd = COALESCE($5, stop_cmd), updated_at = {} WHERE id = $1",
                crate::db::sql::now()
            ))
            .bind(crate::db::bind_id(comp_id)).bind(new_agent_id)
            .bind(&check_override).bind(&start_override).bind(&stop_override)
            .execute(&state.db).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

            let after = serde_json::json!({"agent_id": new_agent_id, "check_cmd": check_override.as_ref().or(cur_check.as_ref()), "start_cmd": start_override.as_ref().or(cur_start.as_ref()), "stop_cmd": stop_override.as_ref().or(cur_stop.as_ref()), "profile": target_profile_name});

            #[cfg(feature = "postgres")]
            let _ = sqlx::query("INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot) VALUES ('component_switchover', $1, $2, $3, $4)")
                .bind(crate::db::bind_id(comp_id)).bind(DbUuid::from(initiated_by))
                .bind(DbJson::from(before.clone())).bind(DbJson::from(after.clone())).execute(&state.db).await;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let _ = sqlx::query("INSERT INTO config_versions (id, resource_type, resource_id, changed_by, before_snapshot, after_snapshot) VALUES ($1, 'component_switchover', $2, $3, $4, $5)")
                .bind(crate::db::bind_id(uuid::Uuid::new_v4())).bind(crate::db::bind_id(comp_id))
                .bind(DbUuid::from(initiated_by)).bind(DbJson::from(before)).bind(DbJson::from(after)).execute(&state.db).await;

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
