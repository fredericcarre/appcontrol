//! Switchover VALIDATE phase.

use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::repository::switchover_queries as repo;
use crate::AppState;

use super::{get_switchover_details, SwitchoverError};

/// Execute the VALIDATE phase.
pub(crate) async fn execute_validate(
    state: &Arc<AppState>,
    app_id: Uuid,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    let mut issues = Vec::new();

    let target_info = get_switchover_details(&state.db, switchover_id).await?;
    let target_site_id: Uuid = target_info["target_site_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SwitchoverError::ValidationFailed("Missing target_site_id".to_string()))?;

    let site_info = repo::get_site_info(&state.db, target_site_id)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    match site_info {
        None => issues.push("Target site does not exist".to_string()),
        Some((_, false)) => issues.push("Target site is inactive".to_string()),
        _ => {}
    }

    let target_profile = repo::find_profile_for_site(&state.db, app_id, target_site_id)
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

    let components = repo::get_app_components(&state.db, app_id)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    let mut components_with_mapping = 0;
    for (_comp_id, comp_name) in &components {
        let has_mapping = repo::has_profile_mapping(&state.db, target_profile_id, comp_name)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        if has_mapping {
            components_with_mapping += 1;
        } else {
            tracing::warn!(app_id = %app_id, component = %comp_name, profile = %target_profile_name,
                "Component has no mapping in target profile (will be skipped)");
        }
    }

    if components_with_mapping == 0 {
        issues.push("No components have mappings in the target profile".to_string());
    }

    let target_agents = repo::get_target_agents_for_profile(&state.db, target_profile_id)
        .await
        .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    for (agent_id, hostname) in &target_agents {
        let last_heartbeat = repo::get_agent_last_heartbeat(&state.db, *agent_id)
            .await
            .map_err(|e| SwitchoverError::Database(e.to_string()))?;

        match last_heartbeat {
            Some(hb) if (chrono::Utc::now() - hb).num_seconds() > 120 => issues.push(format!(
                "Agent '{}' ({}) last heartbeat was {}s ago (stale)",
                hostname,
                agent_id,
                (chrono::Utc::now() - hb).num_seconds()
            )),
            None => issues.push(format!(
                "Agent '{}' ({}) has never sent a heartbeat",
                hostname, agent_id
            )),
            _ => {}
        }
    }

    let validation_result = serde_json::json!({
        "target_profile": target_profile_name,
        "components_total": components.len(),
        "components_with_mapping": components_with_mapping,
        "target_agents_checked": target_agents.len(),
        "issues": issues, "valid": issues.is_empty(),
    });

    if !issues.is_empty() {
        return Err(SwitchoverError::ValidationFailed(
            serde_json::to_string(&issues).unwrap_or_default(),
        ));
    }

    Ok(validation_result)
}
