//! Query functions for import domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{DbJson, DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Application creation for import
// ============================================================================

/// Create an application during YAML import.
#[allow(clippy::too_many_arguments)]
pub async fn create_import_application(
    pool: &DbPool,
    app_id: Uuid,
    name: &str,
    description: Option<&str>,
    org_id: Uuid,
    site_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO applications (id, name, description, organization_id, site_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(description)
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(site_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Create an application during JSON import (with tags).
pub async fn create_import_application_with_tags(
    pool: &DbPool,
    app_id: Uuid,
    name: &str,
    description: Option<&str>,
    org_id: Uuid,
    site_id: Uuid,
    tags_json: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO applications (id, name, description, organization_id, site_id, tags) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(description)
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(site_id))
    .bind(tags_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Grant owner permission to a user on an application.
pub async fn grant_owner_permission(
    pool: &DbPool,
    app_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    let _ = sqlx::query(
        "INSERT INTO app_permissions_users (id, application_id, user_id, permission_level, granted_by) VALUES ($1, $2, $3, 'owner', $3)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(user_id))
    .execute(pool)
    .await;
    Ok(())
}

/// Create an app variable.
pub async fn create_app_variable(
    pool: &DbPool,
    app_id: Uuid,
    name: &str,
    value: &str,
    description: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_variables (id, application_id, name, value, description) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(value)
    .bind(description)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create an app variable with secret flag.
pub async fn create_app_variable_with_secret(
    pool: &DbPool,
    app_id: Uuid,
    name: &str,
    value: &str,
    description: Option<&str>,
    is_secret: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_variables (id, application_id, name, value, description, is_secret) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(value)
    .bind(description)
    .bind(is_secret)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component group.
pub async fn create_component_group(
    pool: &DbPool,
    group_id: Uuid,
    app_id: Uuid,
    name: &str,
    display_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO component_groups (id, application_id, name, display_order) VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(group_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(display_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component group with full details.
pub async fn create_component_group_full(
    pool: &DbPool,
    group_id: Uuid,
    app_id: Uuid,
    name: &str,
    description: Option<&str>,
    color: Option<&str>,
    display_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO component_groups (id, application_id, name, description, color, display_order) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(crate::db::bind_id(group_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(description)
    .bind(color)
    .bind(display_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component during YAML import (basic fields).
pub async fn create_import_component_yaml(
    pool: &DbPool,
    comp_id: Uuid,
    app_id: Uuid,
    name: &str,
    display_name: Option<&str>,
    description: Option<&str>,
    component_type: &str,
    icon: &str,
    group_id: Option<Uuid>,
    check_cmd: &Option<String>,
    start_cmd: &Option<String>,
    stop_cmd: &Option<String>,
    integrity_cmd: &Option<String>,
    pos_x: f32,
    pos_y: f32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO components (id, application_id, name, display_name, description, component_type,
            icon, group_id, check_cmd, start_cmd, stop_cmd, integrity_check_cmd,
            position_x, position_y, current_state)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'STOPPED')"#,
    )
    .bind(crate::db::bind_id(comp_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(component_type)
    .bind(icon)
    .bind(group_id.map(crate::db::bind_id))
    .bind(check_cmd)
    .bind(start_cmd)
    .bind(stop_cmd)
    .bind(integrity_cmd)
    .bind(pos_x)
    .bind(pos_y)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component during JSON import (full fields).
pub async fn create_import_component_json(
    pool: &DbPool,
    comp_id: Uuid,
    app_id: Uuid,
    name: &str,
    display_name: Option<&str>,
    description: Option<&str>,
    component_type: &str,
    icon: &str,
    group_id: Option<Uuid>,
    host: Option<&str>,
    check_cmd: &Option<String>,
    start_cmd: &Option<String>,
    stop_cmd: &Option<String>,
    integrity_cmd: &Option<String>,
    post_start_cmd: &Option<String>,
    infra_cmd: &Option<String>,
    rebuild_cmd: &Option<String>,
    rebuild_infra_cmd: &Option<String>,
    check_interval_seconds: i32,
    start_timeout_seconds: i32,
    stop_timeout_seconds: i32,
    is_optional: bool,
    pos_x: f32,
    pos_y: f32,
    cluster_size: Option<i32>,
    cluster_nodes_json: &Option<Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO components (
            id, application_id, name, display_name, description, component_type,
            icon, group_id, host, check_cmd, start_cmd, stop_cmd,
            integrity_check_cmd, post_start_check_cmd, infra_check_cmd,
            rebuild_cmd, rebuild_infra_cmd,
            check_interval_seconds, start_timeout_seconds, stop_timeout_seconds,
            is_optional, position_x, position_y, cluster_size, cluster_nodes, current_state
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, 'STOPPED'
        )"#,
    )
    .bind(crate::db::bind_id(comp_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(component_type)
    .bind(icon)
    .bind(group_id.map(crate::db::bind_id))
    .bind(host)
    .bind(check_cmd)
    .bind(start_cmd)
    .bind(stop_cmd)
    .bind(integrity_cmd)
    .bind(post_start_cmd)
    .bind(infra_cmd)
    .bind(rebuild_cmd)
    .bind(rebuild_infra_cmd)
    .bind(check_interval_seconds)
    .bind(start_timeout_seconds)
    .bind(stop_timeout_seconds)
    .bind(is_optional)
    .bind(pos_x)
    .bind(pos_y)
    .bind(cluster_size)
    .bind(cluster_nodes_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component command.
pub async fn create_component_command(
    pool: &DbPool,
    cmd_id: Uuid,
    component_id: Uuid,
    name: &str,
    command: &str,
    description: Option<&str>,
    requires_confirmation: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO component_commands (id, component_id, name, command, description, requires_confirmation)
        VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(cmd_id))
    .bind(crate::db::bind_id(component_id))
    .bind(name)
    .bind(command)
    .bind(description)
    .bind(requires_confirmation)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a command input parameter (basic).
pub async fn create_command_input_param(
    pool: &DbPool,
    command_id: Uuid,
    name: &str,
    description: Option<&str>,
    default_value: Option<&str>,
    validation_regex: Option<&str>,
    required: bool,
    display_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO command_input_params (id, command_id, name, description, default_value, validation_regex, required, display_order)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(command_id))
    .bind(name)
    .bind(description)
    .bind(default_value)
    .bind(validation_regex)
    .bind(required)
    .bind(display_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a command input parameter (full, with param_type and enum_values).
pub async fn create_command_input_param_full(
    pool: &DbPool,
    command_id: Uuid,
    name: &str,
    description: Option<&str>,
    default_value: Option<&str>,
    validation_regex: Option<&str>,
    required: bool,
    param_type: &str,
    enum_values: &Option<Value>,
    display_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO command_input_params (
            id, command_id, name, description, default_value, validation_regex,
            required, param_type, enum_values, display_order
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(command_id))
    .bind(name)
    .bind(description)
    .bind(default_value)
    .bind(validation_regex)
    .bind(required)
    .bind(param_type)
    .bind(enum_values)
    .bind(display_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component link.
pub async fn create_component_link(
    pool: &DbPool,
    component_id: Uuid,
    label: &str,
    url: &str,
    link_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO component_links (id, component_id, label, url, link_type) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(component_id))
    .bind(label)
    .bind(url)
    .bind(link_type)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a component link with display order.
pub async fn create_component_link_ordered(
    pool: &DbPool,
    component_id: Uuid,
    label: &str,
    url: &str,
    link_type: &str,
    display_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO component_links (id, component_id, label, url, link_type, display_order) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(component_id))
    .bind(label)
    .bind(url)
    .bind(link_type)
    .bind(display_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a dependency.
pub async fn create_dependency(
    pool: &DbPool,
    app_id: Uuid,
    from_component_id: Uuid,
    to_component_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dependencies (id, application_id, from_component_id, to_component_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(from_component_id))
    .bind(crate::db::bind_id(to_component_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a dependency with type.
pub async fn create_dependency_typed(
    pool: &DbPool,
    app_id: Uuid,
    from_component_id: Uuid,
    to_component_id: Uuid,
    dep_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dependencies (id, application_id, from_component_id, to_component_id, dep_type) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(from_component_id))
    .bind(crate::db::bind_id(to_component_id))
    .bind(dep_type)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Import Wizard queries (api/import_wizard.rs)
// ============================================================================

/// Find an agent by hostname or IP at a specific site.
pub async fn find_agent_at_site_by_host(
    pool: &DbPool,
    org_id: Uuid,
    site_id: Uuid,
    host: &str,
) -> Result<Option<(Uuid,)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (Uuid,)>(
            r#"SELECT a.id FROM agents a
               JOIN gateways g ON a.gateway_id = g.id
               WHERE a.organization_id = $1 AND g.site_id = $2
               AND (a.hostname ILIKE $3 OR EXISTS (
                 SELECT 1 FROM jsonb_array_elements_text(a.ip_addresses) ip WHERE ip = $3
               ))
               LIMIT 1"#,
        )
        .bind(org_id)
        .bind(site_id)
        .bind(host)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (Uuid,)>(
            r#"SELECT a.id FROM agents a
               JOIN gateways g ON a.gateway_id = g.id
               WHERE a.organization_id = $1 AND g.site_id = $2
               AND (a.hostname LIKE $3 OR EXISTS (
                 SELECT 1 FROM json_each(a.ip_addresses) WHERE value = $3
               ))
               LIMIT 1"#,
        )
        .bind(DbUuid::from(org_id))
        .bind(DbUuid::from(site_id))
        .bind(host)
        .fetch_optional(pool)
        .await
    }
}

/// Create a default site for an organization.
pub async fn create_default_site(
    pool: &DbPool,
    site_id: Uuid,
    org_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, $3, $4, $5)")
        .bind(crate::db::bind_id(site_id))
        .bind(crate::db::bind_id(org_id))
        .bind("Default Site")
        .bind("DEFAULT")
        .bind("primary")
        .execute(pool)
        .await?;
    Ok(())
}

/// Find an existing application by org and name. Returns (id,).
pub async fn find_app_by_name(
    pool: &DbPool,
    org_id: Uuid,
    name: &str,
) -> Result<Option<(Uuid,)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM applications WHERE organization_id = $1 AND name = $2",
    )
    .bind(crate::db::bind_id(org_id))
    .bind(name)
    .fetch_optional(pool)
    .await
}

/// Delete components, binding_profiles, app_variables, and component_groups for an update-style import.
pub async fn delete_app_children_for_update(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM components WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM binding_profiles WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM app_variables WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM component_groups WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Update an existing application (for update-style import).
pub async fn update_application_for_import(
    pool: &DbPool,
    app_id: Uuid,
    description: Option<&str>,
    site_id: Uuid,
    tags_json: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE applications SET description = $1, site_id = $2, tags = $3, updated_at = {} WHERE id = $4",
        crate::db::sql::now()
    ))
    .bind(description)
    .bind(crate::db::bind_id(site_id))
    .bind(tags_json)
    .bind(crate::db::bind_id(app_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a new application with tags (for fresh import).
pub async fn insert_application_for_import(
    pool: &DbPool,
    app_id: Uuid,
    name: &str,
    description: Option<&str>,
    org_id: Uuid,
    site_id: Uuid,
    tags_json: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO applications (id, name, description, organization_id, site_id, tags) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(crate::db::bind_id(app_id))
        .bind(name)
        .bind(description)
        .bind(crate::db::bind_id(org_id))
        .bind(crate::db::bind_id(site_id))
        .bind(tags_json)
        .execute(pool)
        .await?;
    Ok(())
}

/// Insert an import component with agent_id and full fields.
pub async fn insert_import_component_with_agent(
    pool: &DbPool,
    comp_id: Uuid,
    app_id: Uuid,
    name: &str,
    display_name: Option<&str>,
    description: Option<&str>,
    component_type: &str,
    icon: &str,
    group_id: Option<Uuid>,
    host: Option<&str>,
    agent_id: Uuid,
    check_cmd: Option<&str>,
    start_cmd: Option<&str>,
    stop_cmd: Option<&str>,
    integrity_cmd: Option<&str>,
    post_start_cmd: Option<&str>,
    infra_cmd: Option<&str>,
    rebuild_cmd: Option<&str>,
    rebuild_infra_cmd: Option<&str>,
    check_interval_seconds: i32,
    start_timeout_seconds: i32,
    stop_timeout_seconds: i32,
    is_optional: bool,
    pos_x: f32,
    pos_y: f32,
    cluster_size: Option<i32>,
    cluster_nodes_json: &Option<Value>,
) -> Result<(), sqlx::Error> {
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
    .bind(crate::db::bind_id(comp_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(display_name)
    .bind(description)
    .bind(component_type)
    .bind(icon)
    .bind(group_id.map(crate::db::bind_id))
    .bind(host)
    .bind(crate::db::bind_id(agent_id))
    .bind(check_cmd)
    .bind(start_cmd)
    .bind(stop_cmd)
    .bind(integrity_cmd)
    .bind(post_start_cmd)
    .bind(infra_cmd)
    .bind(rebuild_cmd)
    .bind(rebuild_infra_cmd)
    .bind(check_interval_seconds)
    .bind(start_timeout_seconds)
    .bind(stop_timeout_seconds)
    .bind(is_optional)
    .bind(pos_x)
    .bind(pos_y)
    .bind(cluster_size)
    .bind(cluster_nodes_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Find a site by org and code. Returns (id,).
pub async fn find_site_by_code(
    pool: &DbPool,
    org_id: Uuid,
    code: &str,
) -> Result<Option<(Uuid,)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid,)>("SELECT id FROM sites WHERE organization_id = $1 AND code = $2")
        .bind(crate::db::bind_id(org_id))
        .bind(code)
        .fetch_optional(pool)
        .await
}

/// Upsert a site override for a component.
pub async fn upsert_site_override(
    pool: &DbPool,
    component_id: Uuid,
    site_id: Uuid,
    agent_id_override: Option<Uuid>,
    check_cmd_override: Option<&str>,
    start_cmd_override: Option<&str>,
    stop_cmd_override: Option<&str>,
    rebuild_cmd_override: Option<&str>,
    env_vars_override: &Option<Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO site_overrides (id, component_id, site_id, agent_id_override, check_cmd_override, start_cmd_override, stop_cmd_override, rebuild_cmd_override, env_vars_override)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (component_id, site_id) DO UPDATE SET
            agent_id_override = EXCLUDED.agent_id_override,
            check_cmd_override = EXCLUDED.check_cmd_override,
            start_cmd_override = EXCLUDED.start_cmd_override,
            stop_cmd_override = EXCLUDED.stop_cmd_override,
            rebuild_cmd_override = EXCLUDED.rebuild_cmd_override,
            env_vars_override = EXCLUDED.env_vars_override"#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(component_id))
    .bind(crate::db::bind_id(site_id))
    .bind(agent_id_override.map(crate::db::bind_id))
    .bind(check_cmd_override)
    .bind(start_cmd_override)
    .bind(stop_cmd_override)
    .bind(rebuild_cmd_override)
    .bind(env_vars_override)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a binding profile with gateway IDs.
pub async fn create_binding_profile(
    pool: &DbPool,
    profile_id: Uuid,
    app_id: Uuid,
    name: &str,
    description: Option<&str>,
    profile_type: &str,
    is_active: bool,
    gateway_ids: crate::db::UuidArray,
    auto_failover: bool,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO binding_profiles (id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(crate::db::bind_id(profile_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(description)
    .bind(profile_type)
    .bind(is_active)
    .bind(gateway_ids)
    .bind(auto_failover)
    .bind(crate::db::bind_id(created_by))
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a binding profile mapping.
pub async fn create_binding_profile_mapping(
    pool: &DbPool,
    profile_id: Uuid,
    component_name: &str,
    host: &str,
    agent_id: Uuid,
    resolved_via: &str,
) -> Result<(), sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO binding_profile_mappings (id, profile_id, component_name, host, agent_id, resolved_via) VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(id))
    .bind(crate::db::bind_id(profile_id))
    .bind(component_name)
    .bind(host)
    .bind(crate::db::bind_id(agent_id))
    .bind(resolved_via)
    .execute(pool)
    .await?;
    Ok(())
}

/// Find an existing application by org and name for preview. Returns (id, name, created_at).
pub async fn find_existing_app_for_preview(
    pool: &DbPool,
    org_id: Uuid,
    name: &str,
) -> Result<Option<(Uuid, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, created_at FROM applications WHERE organization_id = $1 AND name = $2",
    )
    .bind(crate::db::bind_id(org_id))
    .bind(name)
    .fetch_optional(pool)
    .await
}

/// Find the default site for an organization (prefer 'primary' type).
pub async fn find_default_site(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Option<(Uuid,)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM sites WHERE organization_id = $1 AND is_active = true ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM sites WHERE organization_id = $1 AND is_active = 1 ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
        )
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await
    }
}
