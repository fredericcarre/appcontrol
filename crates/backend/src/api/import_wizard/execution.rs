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
            let site = crate::repository::import_queries::find_default_site(&state.db, *user.organization_id).await?;
            match site {
                Some((id,)) => id,
                None => {
                    let new_site_id = Uuid::new_v4();
                    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, $3, $4, $5)")
                        .bind(new_site_id).bind(crate::db::bind_id(user.organization_id))
                        .bind("Default Site").bind("DEFAULT").bind("primary")
                        .execute(&state.db).await?;
                    new_site_id
                }
            }
        }
    };

    let original_name = import_data.application.name.clone().unwrap_or_else(|| "Imported Application".to_string());

    // Handle conflicts
    let existing_app: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM applications WHERE organization_id = $1 AND name = $2")
        .bind(crate::db::bind_id(user.organization_id)).bind(&original_name).fetch_optional(&state.db).await?;

    let (app_id, app_name, is_update) = match (&body.conflict_action, existing_app) {
        (_, None) => (Uuid::new_v4(), original_name, false),
        (ConflictAction::Fail, Some(_)) => {
            return Err(ApiError::Conflict(format!("Application '{}' already exists.", original_name)));
        }
        (ConflictAction::Rename, Some(_)) => {
            let new_name = body.new_name.clone().ok_or_else(|| ApiError::Validation("new_name is required when conflict_action is 'rename'".to_string()))?;
            let new_exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM applications WHERE organization_id = $1 AND name = $2")
                .bind(crate::db::bind_id(user.organization_id)).bind(&new_name).fetch_optional(&state.db).await?;
            if new_exists.is_some() { return Err(ApiError::Conflict(format!("Application '{}' also already exists.", new_name))); }
            (Uuid::new_v4(), new_name, false)
        }
        (ConflictAction::Update, Some((existing_id,))) => {
            sqlx::query("DELETE FROM components WHERE application_id = $1").bind(existing_id).execute(&state.db).await?;
            sqlx::query("DELETE FROM binding_profiles WHERE application_id = $1").bind(existing_id).execute(&state.db).await?;
            sqlx::query("DELETE FROM app_variables WHERE application_id = $1").bind(existing_id).execute(&state.db).await?;
            sqlx::query("DELETE FROM component_groups WHERE application_id = $1").bind(existing_id).execute(&state.db).await?;
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
        sqlx::query(&format!("UPDATE applications SET description = $1, site_id = $2, tags = $3, updated_at = {} WHERE id = $4", crate::db::sql::now()))
            .bind(&import_data.application.description).bind(crate::db::bind_id(site_id))
            .bind(&tags_json).bind(crate::db::bind_id(app_id)).execute(&state.db).await?;
        warnings.push("Existing application updated with new components and profiles.".to_string());
    } else {
        sqlx::query("INSERT INTO applications (id, name, description, organization_id, site_id, tags) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(crate::db::bind_id(app_id)).bind(&app_name).bind(&import_data.application.description)
            .bind(crate::db::bind_id(user.organization_id)).bind(crate::db::bind_id(site_id))
            .bind(&tags_json).execute(&state.db).await?;
    }

    // Grant owner
    let _ = sqlx::query("INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) VALUES ($1, $2, 'owner', $2)")
        .bind(crate::db::bind_id(app_id)).bind(crate::db::bind_id(user.user_id)).execute(&state.db).await;

    // Import variables
    for var in &import_data.application.variables {
        sqlx::query("INSERT INTO app_variables (application_id, name, value, description, is_secret) VALUES ($1, $2, $3, $4, $5)")
            .bind(crate::db::bind_id(app_id)).bind(&var.name).bind(&var.value).bind(&var.description).bind(var.is_secret)
            .execute(&state.db).await?;
    }

    // Import groups
    let mut group_map: HashMap<String, Uuid> = HashMap::new();
    for (idx, group) in import_data.application.groups.iter().enumerate() {
        let group_id = Uuid::new_v4();
        sqlx::query("INSERT INTO component_groups (id, application_id, name, description, color, display_order) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(group_id).bind(crate::db::bind_id(app_id)).bind(&group.name).bind(&group.description)
            .bind(&group.color).bind(idx as i32).execute(&state.db).await?;
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

        sqlx::query(
            r#"INSERT INTO components (
                id, application_id, name, display_name, description, component_type,
                icon, group_id, host, agent_id, check_cmd, start_cmd, stop_cmd,
                integrity_check_cmd, post_start_check_cmd, infra_check_cmd,
                rebuild_cmd, rebuild_infra_cmd,
                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds,
                is_optional, position_x, position_y, cluster_size, cluster_nodes
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26
            )"#,
        )
        .bind(crate::db::bind_id(comp_id)).bind(crate::db::bind_id(app_id))
        .bind(&comp_name).bind(&comp.display_name).bind(&comp.description).bind(comp_type)
        .bind(icon).bind(group_id).bind(&comp.host).bind(crate::db::bind_id(agent_id))
        .bind(check_cmd).bind(start_cmd).bind(stop_cmd).bind(integrity_cmd)
        .bind(post_start_cmd).bind(infra_cmd).bind(rebuild_cmd).bind(rebuild_infra_cmd)
        .bind(comp.check_interval_seconds).bind(comp.start_timeout_seconds).bind(comp.stop_timeout_seconds)
        .bind(comp.is_optional).bind(pos_x).bind(pos_y).bind(comp.cluster_size).bind(&cluster_nodes_json)
        .execute(&state.db).await?;

        comp_name_to_id.insert(comp_name.clone(), comp_id);
        components_created += 1;

        // Import custom commands
        for custom_cmd in &comp.custom_commands {
            let cmd_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO component_commands (id, component_id, name, command, description, requires_confirmation) VALUES ($1, $2, $3, $4, $5, $6)"#)
                .bind(cmd_id).bind(crate::db::bind_id(comp_id)).bind(&custom_cmd.name)
                .bind(&custom_cmd.command).bind(&custom_cmd.description).bind(custom_cmd.requires_confirmation)
                .execute(&state.db).await?;

            for (pidx, param) in custom_cmd.parameters.iter().enumerate() {
                let enum_vals_json = param.enum_values.as_ref().and_then(|v| serde_json::to_value(v).ok());
                sqlx::query(r#"INSERT INTO command_input_params (command_id, name, description, default_value, validation_regex, required, param_type, enum_values, display_order) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#)
                    .bind(cmd_id).bind(&param.name).bind(&param.description).bind(&param.default_value)
                    .bind(&param.validation_regex).bind(param.required).bind(&param.param_type)
                    .bind(&enum_vals_json).bind(pidx as i32).execute(&state.db).await?;
            }
        }

        // Import links
        for (lidx, link) in comp.links.iter().enumerate() {
            sqlx::query("INSERT INTO component_links (component_id, label, url, link_type, display_order) VALUES ($1, $2, $3, $4, $5)")
                .bind(crate::db::bind_id(comp_id)).bind(&link.label).bind(&link.url).bind(&link.link_type).bind(lidx as i32)
                .execute(&state.db).await?;
        }

        // Import site overrides
        for override_data in &comp.site_overrides {
            let site_row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM sites WHERE organization_id = $1 AND code = $2")
                .bind(crate::db::bind_id(user.organization_id)).bind(&override_data.site_code)
                .fetch_optional(&state.db).await?;

            let override_site_id = match site_row {
                Some((id,)) => id,
                None => { warnings.push(format!("Component '{}': site override for code '{}' skipped", comp_name, override_data.site_code)); continue; }
            };

            let agent_id_override: Option<Uuid> = if let Some(ref host) = override_data.host_override {
                let agent_row = crate::repository::import_queries::find_agent_at_site_by_host(&state.db, *user.organization_id, override_site_id, host).await?;
                match agent_row {
                    Some((id,)) => Some(id),
                    None => { warnings.push(format!("Component '{}': site '{}' host_override '{}' could not be resolved", comp_name, override_data.site_code, host)); None }
                }
            } else { None };

            sqlx::query(r#"INSERT INTO site_overrides (component_id, site_id, agent_id_override, check_cmd_override, start_cmd_override, stop_cmd_override, rebuild_cmd_override, env_vars_override) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (component_id, site_id) DO UPDATE SET agent_id_override = EXCLUDED.agent_id_override, check_cmd_override = EXCLUDED.check_cmd_override, start_cmd_override = EXCLUDED.start_cmd_override, stop_cmd_override = EXCLUDED.stop_cmd_override, rebuild_cmd_override = EXCLUDED.rebuild_cmd_override, env_vars_override = EXCLUDED.env_vars_override"#)
                .bind(crate::db::bind_id(comp_id)).bind(override_site_id).bind(agent_id_override)
                .bind(&override_data.check_cmd_override).bind(&override_data.start_cmd_override)
                .bind(&override_data.stop_cmd_override).bind(&override_data.rebuild_cmd_override)
                .bind(&override_data.env_vars_override).execute(&state.db).await?;
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
        sqlx::query("INSERT INTO dependencies (application_id, from_component_id, to_component_id) VALUES ($1, $2, $3)")
            .bind(crate::db::bind_id(app_id)).bind(from_id).bind(to_id).execute(&state.db).await?;
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
    sqlx::query(r#"INSERT INTO binding_profiles (id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#)
        .bind(primary_profile_id).bind(crate::db::bind_id(app_id))
        .bind(&body.profile.name).bind(&body.profile.description).bind(&body.profile.profile_type)
        .bind(true).bind(UuidArray::from(body.profile.gateway_ids.clone()))
        .bind(body.profile.auto_failover.unwrap_or(false)).bind(crate::db::bind_id(user.user_id))
        .execute(&state.db).await?;

    for mapping in &body.profile.mappings {
        let host = import_data.application.components.iter()
            .find(|c| c.name.as_ref() == Some(&mapping.component_name))
            .and_then(|c| c.host.clone()).unwrap_or_default();
        sqlx::query(r#"INSERT INTO binding_profile_mappings (profile_id, component_name, host, agent_id, resolved_via) VALUES ($1, $2, $3, $4, $5)"#)
            .bind(primary_profile_id).bind(&mapping.component_name).bind(&host)
            .bind(mapping.agent_id).bind(&mapping.resolved_via).execute(&state.db).await?;
    }

    let mut profiles_created = vec![body.profile.name.clone()];

    // Create DR profile if specified
    if let Some(ref dr_profile) = body.dr_profile {
        let dr_profile_id = Uuid::new_v4();
        sqlx::query(r#"INSERT INTO binding_profiles (id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#)
            .bind(dr_profile_id).bind(crate::db::bind_id(app_id))
            .bind(&dr_profile.name).bind(&dr_profile.description).bind(&dr_profile.profile_type)
            .bind(false).bind(UuidArray::from(dr_profile.gateway_ids.clone()))
            .bind(dr_profile.auto_failover.unwrap_or(false)).bind(crate::db::bind_id(user.user_id))
            .execute(&state.db).await?;

        for mapping in &dr_profile.mappings {
            let host = import_data.application.components.iter()
                .find(|c| c.name.as_ref() == Some(&mapping.component_name))
                .and_then(|c| c.host.clone()).unwrap_or_default();
            sqlx::query(r#"INSERT INTO binding_profile_mappings (profile_id, component_name, host, agent_id, resolved_via) VALUES ($1, $2, $3, $4, $5)"#)
                .bind(dr_profile_id).bind(&mapping.component_name).bind(&host)
                .bind(mapping.agent_id).bind(&mapping.resolved_via).execute(&state.db).await?;
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
