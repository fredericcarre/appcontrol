//! Import execution handler.

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::dag;
use crate::db::UuidArray;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::repository::import_queries as repo;
use crate::AppState;

use super::types::*;

/// POST /api/v1/import/execute
pub async fn execute_import(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ImportExecuteRequest>,
) -> Result<(StatusCode, Json<ImportExecuteResponse>), ApiError> {
    let import_data = parse_import_content(&body.content, &body.format)?;

    // Resolve site_id
    let site_id = match body.site_id {
        Some(id) => id,
        None => {
            let site = repo::find_default_site(&state.db, *user.organization_id).await?;
            match site {
                Some((id,)) => id,
                None => {
                    let new_site_id = Uuid::new_v4();
                    repo::create_default_site(&state.db, new_site_id, *user.organization_id).await?;
                    new_site_id
                }
            }
        }
    };

    let original_name = import_data.application.name.clone().unwrap_or_else(|| "Imported Application".to_string());

    // Handle conflicts
    let existing_app = repo::find_app_by_name(&state.db, *user.organization_id, &original_name).await?;

    let (app_id, app_name, is_update) = match (&body.conflict_action, existing_app) {
        (_, None) => (Uuid::new_v4(), original_name, false),
        (ConflictAction::Fail, Some(_)) => {
            return Err(ApiError::Conflict(format!("Application '{}' already exists.", original_name)));
        }
        (ConflictAction::Rename, Some(_)) => {
            let new_name = body.new_name.clone().ok_or_else(|| ApiError::Validation("new_name is required when conflict_action is 'rename'".to_string()))?;
            let new_exists = repo::find_app_by_name(&state.db, *user.organization_id, &new_name).await?;
            if new_exists.is_some() { return Err(ApiError::Conflict(format!("Application '{}' also already exists.", new_name))); }
            (Uuid::new_v4(), new_name, false)
        }
        (ConflictAction::Update, Some((existing_id,))) => {
            repo::delete_app_children_for_update(&state.db, existing_id).await?;
            (existing_id, original_name, true)
        }
    };

    // Validate all components have mappings
    let component_names: Vec<_> = import_data.application.components.iter().filter_map(|c| c.name.clone()).collect();
    let mapped_names: Vec<_> = body.profile.mappings.iter().map(|m| &m.component_name).collect();
    for name in &component_names {
        if !mapped_names.contains(&name) {
            return Err(ApiError::Validation(format!("Component '{}' has no agent mapping.", name)));
        }
    }

    let mappings_map: HashMap<_, _> = body.profile.mappings.iter().map(|m| (m.component_name.clone(), m)).collect();
    let mut warnings = Vec::new();

    let action_type = if is_update { "update_with_profiles" } else { "import_with_profiles" };
    log_action(&state.db, user.user_id, action_type, "application", app_id,
        json!({"name": &app_name, "profile": &body.profile.name, "dr_profile": body.dr_profile.as_ref().map(|p| &p.name), "is_update": is_update}),
    ).await?;

    // Create or update application
    let tags_json = serde_json::to_value(&import_data.application.tags).unwrap_or(Value::Null);
    if is_update {
        repo::update_application_for_import(&state.db, app_id, import_data.application.description.as_deref(), site_id, &tags_json).await?;
        warnings.push("Existing application updated with new components and profiles.".to_string());
    } else {
        repo::insert_application_for_import(&state.db, app_id, &app_name, import_data.application.description.as_deref(), *user.organization_id, site_id, &tags_json).await?;
    }

    // Grant owner
    let _ = repo::grant_owner_permission(&state.db, app_id, *user.user_id).await;

    // Import variables
    for var in &import_data.application.variables {
        repo::create_app_variable_with_secret(&state.db, app_id, &var.name, &var.value, var.description.as_deref(), var.is_secret).await?;
    }

    // Import groups
    let mut group_map: HashMap<String, Uuid> = HashMap::new();
    for (idx, group) in import_data.application.groups.iter().enumerate() {
        let group_id = Uuid::new_v4();
        repo::create_component_group_full(&state.db, group_id, app_id, &group.name, group.description.as_deref(), group.color.as_deref(), idx as i32).await?;
        group_map.insert(group.name.clone(), group_id);
    }

    // Import components
    let mut comp_name_to_id: HashMap<String, Uuid> = HashMap::new();
    let mut components_created = 0;

    for (idx, comp) in import_data.application.components.iter().enumerate() {
        let comp_name = comp.name.clone().unwrap_or_else(|| format!("component_{}", idx));
        let comp_id = Uuid::new_v4();
        let agent_id = mappings_map.get(&comp_name).map(|m| m.agent_id)
            .ok_or_else(|| ApiError::Validation(format!("No mapping for component '{}'", comp_name)))?;

        let group_id = comp.group.as_ref().and_then(|g| group_map.get(g)).copied();
        let comp_type = comp.component_type.as_deref().unwrap_or("service");
        let icon = comp.icon.as_deref().unwrap_or(default_icon_for_type(comp_type));

        let check_cmd = comp.check_cmd.as_ref().or_else(|| comp.commands.check.as_ref().map(|c| &c.cmd));
        let start_cmd = comp.start_cmd.as_ref().or_else(|| comp.commands.start.as_ref().map(|c| &c.cmd));
        let stop_cmd = comp.stop_cmd.as_ref().or_else(|| comp.commands.stop.as_ref().map(|c| &c.cmd));
        let integrity_cmd = comp.integrity_check_cmd.as_ref().or_else(|| comp.commands.integrity_check.as_ref().map(|c| &c.cmd));
        let post_start_cmd = comp.commands.post_start_check.as_ref().map(|c| &c.cmd);
        let infra_cmd = comp.infra_check_cmd.as_ref().or_else(|| comp.commands.infra_check.as_ref().map(|c| &c.cmd));
        let rebuild_cmd = comp.rebuild_cmd.as_ref().or_else(|| comp.commands.rebuild.as_ref().map(|c| &c.cmd));
        let rebuild_infra_cmd = comp.commands.rebuild_infra.as_ref().map(|c| &c.cmd);

        let pos_x = comp.position.as_ref().map(|p| p.x).or(comp.position_x).unwrap_or((idx % 5) as f32 * 250.0);
        let pos_y = comp.position.as_ref().map(|p| p.y).or(comp.position_y).unwrap_or((idx / 5) as f32 * 200.0);
        let cluster_nodes_json: Option<serde_json::Value> = comp.cluster_nodes.as_ref().map(|nodes| serde_json::json!(nodes));

        repo::insert_import_component_with_agent(
            &state.db, comp_id, app_id, &comp_name, comp.display_name.as_deref(),
            comp.description.as_deref(), comp_type, icon, group_id, comp.host.as_deref(),
            agent_id, check_cmd.map(|s| s.as_str()), start_cmd.map(|s| s.as_str()),
            stop_cmd.map(|s| s.as_str()), integrity_cmd.map(|s| s.as_str()),
            post_start_cmd.map(|s| s.as_str()), infra_cmd.map(|s| s.as_str()),
            rebuild_cmd.map(|s| s.as_str()), rebuild_infra_cmd.map(|s| s.as_str()),
            comp.check_interval_seconds, comp.start_timeout_seconds, comp.stop_timeout_seconds,
            comp.is_optional, pos_x, pos_y, comp.cluster_size, &cluster_nodes_json,
        ).await?;

        comp_name_to_id.insert(comp_name.clone(), comp_id);
        components_created += 1;

        // Import custom commands
        for custom_cmd in &comp.custom_commands {
            let cmd_id = Uuid::new_v4();
            repo::create_component_command(&state.db, cmd_id, comp_id, &custom_cmd.name, &custom_cmd.command, custom_cmd.description.as_deref(), custom_cmd.requires_confirmation).await?;

            for (pidx, param) in custom_cmd.parameters.iter().enumerate() {
                let enum_vals_json = param.enum_values.as_ref().and_then(|v| serde_json::to_value(v).ok());
                repo::create_command_input_param_full(
                    &state.db, cmd_id, &param.name, param.description.as_deref(),
                    param.default_value.as_deref(), param.validation_regex.as_deref(),
                    param.required, &param.param_type, &enum_vals_json, pidx as i32,
                ).await?;
            }
        }

        // Import links
        for (lidx, link) in comp.links.iter().enumerate() {
            repo::create_component_link_ordered(&state.db, comp_id, &link.label, &link.url, &link.link_type, lidx as i32).await?;
        }

        // Import site overrides
        for override_data in &comp.site_overrides {
            let site_row = repo::find_site_by_code(&state.db, *user.organization_id, &override_data.site_code).await?;

            let override_site_id = match site_row {
                Some((id,)) => id,
                None => { warnings.push(format!("Component '{}': site override for code '{}' skipped", comp_name, override_data.site_code)); continue; }
            };

            let agent_id_override: Option<Uuid> = if let Some(ref host) = override_data.host_override {
                let agent_row = repo::find_agent_at_site_by_host(&state.db, *user.organization_id, override_site_id, host).await?;
                match agent_row {
                    Some((id,)) => Some(id),
                    None => { warnings.push(format!("Component '{}': site '{}' host_override '{}' could not be resolved", comp_name, override_data.site_code, host)); None }
                }
            } else { None };

            repo::upsert_site_override(
                &state.db, comp_id, override_site_id, agent_id_override,
                override_data.check_cmd_override.as_deref(), override_data.start_cmd_override.as_deref(),
                override_data.stop_cmd_override.as_deref(), override_data.rebuild_cmd_override.as_deref(),
                &override_data.env_vars_override,
            ).await?;
        }
    }

    // Import dependencies
    for dep in &import_data.application.dependencies {
        let from_id = match comp_name_to_id.get(&dep.from) {
            Some(id) => *id,
            None => { warnings.push(format!("Dependency from '{}' to '{}': source not found", dep.from, dep.to)); continue; }
        };
        let to_id = match comp_name_to_id.get(&dep.to) {
            Some(id) => *id,
            None => { warnings.push(format!("Dependency from '{}' to '{}': target not found", dep.from, dep.to)); continue; }
        };
        repo::create_dependency(&state.db, app_id, from_id, to_id).await?;
    }

    // Validate DAG
    let dag_result = dag::build_dag(&state.db, app_id).await;
    if let Ok(dag) = dag_result {
        if let Err(cycle_err) = dag.topological_levels() {
            warnings.push(format!("Warning: DAG contains a cycle - {}", cycle_err));
        }
    }

    // Create primary binding profile
    let primary_profile_id = Uuid::new_v4();
    repo::create_binding_profile(
        &state.db, primary_profile_id, app_id, &body.profile.name, body.profile.description.as_deref(),
        &body.profile.profile_type, true, UuidArray::from(body.profile.gateway_ids.clone()),
        body.profile.auto_failover.unwrap_or(false), *user.user_id,
    ).await?;

    for mapping in &body.profile.mappings {
        let host = import_data.application.components.iter()
            .find(|c| c.name.as_ref() == Some(&mapping.component_name))
            .and_then(|c| c.host.clone()).unwrap_or_default();
        repo::create_binding_profile_mapping(&state.db, primary_profile_id, &mapping.component_name, &host, mapping.agent_id, &mapping.resolved_via).await?;
    }

    let mut profiles_created = vec![body.profile.name.clone()];

    // Create DR profile if specified
    if let Some(ref dr_profile) = body.dr_profile {
        let dr_profile_id = Uuid::new_v4();
        repo::create_binding_profile(
            &state.db, dr_profile_id, app_id, &dr_profile.name, dr_profile.description.as_deref(),
            &dr_profile.profile_type, false, UuidArray::from(dr_profile.gateway_ids.clone()),
            dr_profile.auto_failover.unwrap_or(false), *user.user_id,
        ).await?;

        for mapping in &dr_profile.mappings {
            let host = import_data.application.components.iter()
                .find(|c| c.name.as_ref() == Some(&mapping.component_name))
                .and_then(|c| c.host.clone()).unwrap_or_default();
            repo::create_binding_profile_mapping(&state.db, dr_profile_id, &mapping.component_name, &host, mapping.agent_id, &mapping.resolved_via).await?;
        }
        profiles_created.push(dr_profile.name.clone());
    }

    crate::websocket::push_config_to_affected_agents(&state, Some(app_id), None, None).await;

    let response = ImportExecuteResponse {
        application_id: app_id, application_name: app_name,
        components_created, profiles_created, active_profile: body.profile.name.clone(), warnings,
    };

    Ok((StatusCode::CREATED, Json(response)))
}
