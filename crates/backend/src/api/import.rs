//! Map Importers: converts various formats to v4 model.
//!
//! Supported formats:
//! - YAML (v3 legacy): Old AppControl YAML maps with actions, variables, groups
//! - JSON (v4 native): Current v4 format with full structure
//!
//! Both importers create:
//! - Application + site (if needed)
//! - Component groups
//! - Components with all commands
//! - Dependencies
//! - App variables
//! - Component commands + input parameters
//! - Component links (hypertext resources)

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

// ── Old YAML format structures ──────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldMap {
    application: OldApplication,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldApplication {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    variables: Vec<OldVariable>,
    #[serde(default)]
    components: Vec<OldComponent>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldVariable {
    name: Option<String>,
    value: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldComponent {
    name: Option<String>,
    #[serde(alias = "displayName")]
    display_name: Option<String>,
    description: Option<String>,
    #[serde(alias = "componentType", alias = "type")]
    component_type: Option<String>,
    group: Option<String>,
    icon: Option<String>,
    agent: Option<String>,
    #[serde(alias = "dependsOn", alias = "depends_on", default)]
    depends_on: Vec<String>,
    #[serde(default)]
    actions: Vec<OldAction>,
    #[serde(alias = "hypertextLinks", alias = "hypertext_links", default)]
    hypertext_links: Vec<OldHypertextLink>,
    // Position hints
    #[serde(alias = "positionX")]
    position_x: Option<f32>,
    #[serde(alias = "positionY")]
    position_y: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldAction {
    name: Option<String>,
    #[serde(alias = "displayName")]
    display_name: Option<String>,
    command: Option<String>,
    #[serde(alias = "actionType", alias = "type")]
    action_type: Option<String>,
    description: Option<String>,
    #[serde(alias = "requiresConfirmation")]
    requires_confirmation: Option<bool>,
    #[serde(default)]
    parameters: Vec<OldParameter>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldParameter {
    name: Option<String>,
    description: Option<String>,
    #[serde(alias = "defaultValue")]
    default_value: Option<String>,
    #[serde(alias = "validationRegex", alias = "validation")]
    validation_regex: Option<String>,
    required: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OldHypertextLink {
    #[serde(alias = "displayName")]
    label: Option<String>,
    url: Option<String>,
    #[serde(alias = "type")]
    link_type: Option<String>,
}

// ── Import result ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ImportResult {
    application_id: Uuid,
    application_name: String,
    components_created: usize,
    groups_created: usize,
    variables_created: usize,
    commands_created: usize,
    dependencies_created: usize,
    links_created: usize,
    warnings: Vec<String>,
}

// ── Import endpoint ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub yaml: String,
    pub site_id: Uuid,
}

/// POST /api/v1/import/yaml
/// Import an old AppControl YAML map into the v4 model.
pub async fn import_yaml_map(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ImportRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Parse YAML
    let old_map: OldMap = serde_yaml::from_str(&body.yaml).map_err(|e| {
        tracing::warn!("YAML parse error: {}", e);
        ApiError::Validation(format!("Invalid YAML: {}", e))
    })?;

    let app_name = old_map
        .application
        .name
        .as_deref()
        .unwrap_or("Imported Application");
    let app_desc = old_map.application.description.as_deref();

    let app_id = Uuid::new_v4();
    let mut warnings = Vec::new();

    // Log import action
    log_action(
        &state.db,
        user.user_id,
        "import_yaml",
        "application",
        app_id,
        json!({"name": app_name}),
    )
    .await?;

    // Create application
    sqlx::query(
        "INSERT INTO applications (id, name, description, organization_id, site_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(app_name)
    .bind(app_desc)
    .bind(user.organization_id)
    .bind(body.site_id)
    .execute(&state.db)
    .await?;

    // Grant owner to importing user
    let _ = sqlx::query(
        "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) VALUES ($1, $2, 'owner', $2)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(user.user_id)
    .execute(&state.db)
    .await;

    // ── Import variables ────────────────────────────────────────────
    let mut variables_created = 0;
    for var in &old_map.application.variables {
        let name = match &var.name {
            Some(n) => n,
            None => {
                warnings.push("Skipping variable with no name".to_string());
                continue;
            }
        };
        let value = var.value.as_deref().unwrap_or("");

        sqlx::query(
            "INSERT INTO app_variables (application_id, name, value, description) VALUES ($1, $2, $3, $4)",
        )
        .bind(crate::db::bind_id(app_id))
        .bind(name)
        .bind(value)
        .bind(var.description.as_deref())
        .execute(&state.db)
        .await?;

        variables_created += 1;
    }

    // ── Import groups ───────────────────────────────────────────────
    let mut group_map: HashMap<String, Uuid> = HashMap::new();
    let mut groups_created = 0;

    // Collect unique groups from components
    for comp in &old_map.application.components {
        if let Some(ref group_name) = comp.group {
            if !group_map.contains_key(group_name) {
                let group_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO component_groups (id, application_id, name, display_order) VALUES ($1, $2, $3, $4)",
                )
                .bind(group_id)
                .bind(crate::db::bind_id(app_id))
                .bind(group_name)
                .bind(groups_created as i32)
                .execute(&state.db)
                .await?;

                group_map.insert(group_name.clone(), group_id);
                groups_created += 1;
            }
        }
    }

    // ── Import components ───────────────────────────────────────────
    let mut comp_name_to_id: HashMap<String, Uuid> = HashMap::new();
    let mut components_created = 0;
    let mut commands_created = 0;
    let mut links_created = 0;

    for (idx, comp) in old_map.application.components.iter().enumerate() {
        let comp_name = match &comp.name {
            Some(n) => n,
            None => {
                warnings.push(format!("Skipping component at index {} with no name", idx));
                continue;
            }
        };

        let comp_id = Uuid::new_v4();
        let group_id = comp.group.as_ref().and_then(|g| group_map.get(g)).copied();

        // Map component type from old format
        let comp_type = map_component_type(comp.component_type.as_deref());
        let icon = comp
            .icon
            .as_deref()
            .unwrap_or(default_icon_for_type(comp_type));

        // Extract standard actions → core commands
        let check_cmd = find_action_cmd(&comp.actions, &["check", "status", "health"]);
        let start_cmd = find_action_cmd(&comp.actions, &["start", "startup", "launch"]);
        let stop_cmd = find_action_cmd(&comp.actions, &["stop", "shutdown", "halt"]);
        let integrity_cmd = find_action_cmd(&comp.actions, &["integrity", "verify", "validate"]);

        // Calculate grid position if not specified
        let pos_x = comp.position_x.unwrap_or((idx % 5) as f32 * 250.0);
        let pos_y = comp.position_y.unwrap_or((idx / 5) as f32 * 200.0);

        sqlx::query(
            r#"INSERT INTO components (id, application_id, name, display_name, description, component_type,
                icon, group_id, check_cmd, start_cmd, stop_cmd, integrity_check_cmd,
                position_x, position_y)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
        )
        .bind(crate::db::bind_id(comp_id))
        .bind(crate::db::bind_id(app_id))
        .bind(comp_name)
        .bind(comp.display_name.as_deref())
        .bind(comp.description.as_deref())
        .bind(comp_type)
        .bind(icon)
        .bind(group_id)
        .bind(&check_cmd)
        .bind(&start_cmd)
        .bind(&stop_cmd)
        .bind(&integrity_cmd)
        .bind(pos_x)
        .bind(pos_y)
        .execute(&state.db)
        .await?;

        comp_name_to_id.insert(comp_name.clone(), comp_id);
        components_created += 1;

        // ── Import custom commands (non-standard actions) ───────────
        for action in &comp.actions {
            let action_name = match &action.name {
                Some(n) => n.to_lowercase(),
                None => continue,
            };

            // Skip standard actions that are already mapped to core commands
            if is_standard_action(&action_name) {
                continue;
            }

            let cmd_text = match &action.command {
                Some(c) => c,
                None => continue,
            };

            let cmd_id = Uuid::new_v4();
            let display = action.display_name.as_deref().unwrap_or(&action_name);

            sqlx::query(
                r#"INSERT INTO component_commands (id, component_id, name, command, description, requires_confirmation)
                VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(cmd_id)
            .bind(crate::db::bind_id(comp_id))
            .bind(display)
            .bind(cmd_text)
            .bind(action.description.as_deref())
            .bind(action.requires_confirmation.unwrap_or(false))
            .execute(&state.db)
            .await?;

            commands_created += 1;

            // Import input parameters for this command
            for (pidx, param) in action.parameters.iter().enumerate() {
                let param_name = match &param.name {
                    Some(n) => n,
                    None => continue,
                };

                sqlx::query(
                    r#"INSERT INTO command_input_params (command_id, name, description, default_value, validation_regex, required, display_order)
                    VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
                )
                .bind(cmd_id)
                .bind(param_name)
                .bind(param.description.as_deref())
                .bind(param.default_value.as_deref())
                .bind(param.validation_regex.as_deref())
                .bind(param.required.unwrap_or(true))
                .bind(pidx as i32)
                .execute(&state.db)
                .await?;
            }
        }

        // ── Import hypertext links ──────────────────────────────────
        for link in &comp.hypertext_links {
            let label = match &link.label {
                Some(l) => l,
                None => continue,
            };
            let url = match &link.url {
                Some(u) => u,
                None => continue,
            };

            let link_type = map_link_type(link.link_type.as_deref());

            sqlx::query(
                "INSERT INTO component_links (component_id, label, url, link_type) VALUES ($1, $2, $3, $4)",
            )
            .bind(crate::db::bind_id(comp_id))
            .bind(label)
            .bind(url)
            .bind(link_type)
            .execute(&state.db)
            .await?;

            links_created += 1;
        }
    }

    // ── Import dependencies ─────────────────────────────────────────
    let mut dependencies_created = 0;
    for comp in &old_map.application.components {
        let comp_name = match &comp.name {
            Some(n) => n,
            None => continue,
        };
        let from_id = match comp_name_to_id.get(comp_name) {
            Some(id) => *id,
            None => continue,
        };

        for dep_name in &comp.depends_on {
            let to_id = match comp_name_to_id.get(dep_name) {
                Some(id) => *id,
                None => {
                    warnings.push(format!(
                        "Component '{}' depends on '{}' which was not found",
                        comp_name, dep_name
                    ));
                    continue;
                }
            };

            sqlx::query(
                "INSERT INTO dependencies (application_id, from_component_id, to_component_id) VALUES ($1, $2, $3)",
            )
            .bind(crate::db::bind_id(app_id))
            .bind(from_id)
            .bind(to_id)
            .execute(&state.db)
            .await?;

            dependencies_created += 1;
        }
    }

    let result = ImportResult {
        application_id: app_id,
        application_name: app_name.to_string(),
        components_created,
        groups_created,
        variables_created,
        commands_created,
        dependencies_created,
        links_created,
        warnings,
    };

    Ok((StatusCode::CREATED, Json(json!(result))))
}

// ── Helper functions ────────────────────────────────────────────────

fn map_component_type(old_type: Option<&str>) -> &str {
    match old_type.map(|s| s.to_lowercase()).as_deref() {
        Some("database") | Some("db") | Some("sql") | Some("sqlserver") | Some("mysql")
        | Some("postgresql") | Some("oracle") | Some("mongodb") => "database",
        Some("middleware") | Some("mq") | Some("rabbitmq") | Some("kafka") | Some("redis") => {
            "middleware"
        }
        Some("appserver") | Some("tomcat") | Some("jboss") | Some("wildfly") | Some("jetty") => {
            "appserver"
        }
        Some("webfront") | Some("web") | Some("nginx") | Some("apache") | Some("iis") => "webfront",
        Some("service") | Some("api") | Some("microservice") => "service",
        Some("batch") | Some("job") | Some("cron") | Some("scheduler") => "batch",
        Some("firewall")
        | Some("network")
        | Some("vm")
        | Some("azure")
        | Some("cloud")
        | Some("backup")
        | Some("storage")
        | Some("infra")
        | Some("infrastructure") => "custom",
        _ => "custom",
    }
}

fn default_icon_for_type(comp_type: &str) -> &str {
    match comp_type {
        "database" => "database",
        "middleware" => "layers",
        "appserver" => "server",
        "webfront" => "globe",
        "service" => "cog",
        "batch" => "clock",
        _ => "box",
    }
}

fn find_action_cmd(actions: &[OldAction], keywords: &[&str]) -> Option<String> {
    for action in actions {
        if let Some(ref name) = action.name {
            let lower = name.to_lowercase();
            for kw in keywords {
                if lower.contains(kw) {
                    return action.command.clone();
                }
            }
        }
        if let Some(ref action_type) = action.action_type {
            let lower = action_type.to_lowercase();
            for kw in keywords {
                if lower.contains(kw) {
                    return action.command.clone();
                }
            }
        }
    }
    None
}

fn is_standard_action(name: &str) -> bool {
    matches!(
        name,
        "check"
            | "status"
            | "health"
            | "start"
            | "startup"
            | "launch"
            | "stop"
            | "shutdown"
            | "halt"
            | "integrity"
            | "verify"
            | "validate"
    )
}

fn map_link_type(old_type: Option<&str>) -> &str {
    match old_type.map(|s| s.to_lowercase()).as_deref() {
        Some("documentation") | Some("doc") | Some("docs") => "documentation",
        Some("cmdb") => "cmdb",
        Some("monitoring") | Some("grafana") | Some("prometheus") | Some("zabbix") => "monitoring",
        Some("log") | Some("splunk") | Some("elk") | Some("kibana") => "log",
        Some("runbook") | Some("procedure") => "runbook",
        _ => "other",
    }
}

// ══════════════════════════════════════════════════════════════════════
// JSON v4 Import
// ══════════════════════════════════════════════════════════════════════

/// v4 native JSON format structures (matching export.rs output)
#[derive(Debug, Deserialize)]
pub struct JsonImportRequest {
    pub json: String,
    pub site_id: Uuid,
    /// Optional: If set, import into an existing application (merge mode)
    pub merge_into_app_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct V4Import {
    format_version: Option<String>,
    application: V4Application,
}

#[derive(Debug, Deserialize)]
struct V4Application {
    name: String,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    variables: Vec<V4Variable>,
    #[serde(default)]
    groups: Vec<V4Group>,
    #[serde(default)]
    components: Vec<V4Component>,
    #[serde(default)]
    dependencies: Vec<V4Dependency>,
}

#[derive(Debug, Deserialize)]
struct V4Variable {
    name: String,
    value: String,
    description: Option<String>,
    #[serde(default)]
    is_secret: bool,
}

#[derive(Debug, Deserialize)]
struct V4Group {
    name: String,
    description: Option<String>,
    color: Option<String>,
    #[serde(default)]
    display_order: i32,
}

#[derive(Debug, Deserialize)]
struct V4Component {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    #[serde(alias = "type")]
    component_type: Option<String>,
    icon: Option<String>,
    group: Option<String>,
    host: Option<String>,
    #[serde(default)]
    commands: V4Commands,
    #[serde(default)]
    custom_commands: Vec<V4CustomCommand>,
    #[serde(default)]
    links: Vec<V4Link>,
    position_x: Option<f32>,
    position_y: Option<f32>,
    #[serde(default = "default_check_interval")]
    check_interval_seconds: i32,
    #[serde(default = "default_start_timeout")]
    start_timeout_seconds: i32,
    #[serde(default = "default_stop_timeout")]
    stop_timeout_seconds: i32,
    #[serde(default)]
    is_optional: bool,
    /// Cluster size (number of nodes, >= 2 for clusters)
    cluster_size: Option<i32>,
    /// List of cluster node hostnames/IPs
    cluster_nodes: Option<Vec<String>>,
}

fn default_check_interval() -> i32 {
    30
}
fn default_start_timeout() -> i32 {
    300
}
fn default_stop_timeout() -> i32 {
    120
}

#[derive(Debug, Deserialize, Default)]
struct V4Commands {
    check: Option<V4CommandDetail>,
    start: Option<V4CommandDetail>,
    stop: Option<V4CommandDetail>,
    integrity_check: Option<V4CommandDetail>,
    post_start_check: Option<V4CommandDetail>,
    infra_check: Option<V4CommandDetail>,
    rebuild: Option<V4CommandDetail>,
    rebuild_infra: Option<V4CommandDetail>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct V4CommandDetail {
    cmd: String,
    timeout_seconds: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct V4CustomCommand {
    name: String,
    command: String,
    description: Option<String>,
    #[serde(default)]
    requires_confirmation: bool,
    #[serde(default)]
    parameters: Vec<V4CommandParam>,
}

#[derive(Debug, Deserialize)]
struct V4CommandParam {
    name: String,
    description: Option<String>,
    default_value: Option<String>,
    validation_regex: Option<String>,
    #[serde(default = "default_required")]
    required: bool,
    #[serde(default = "default_param_type")]
    param_type: String,
    enum_values: Option<Vec<String>>,
}

fn default_required() -> bool {
    true
}
fn default_param_type() -> String {
    "string".to_string()
}

#[derive(Debug, Deserialize)]
struct V4Link {
    label: String,
    url: String,
    #[serde(default = "default_link_type")]
    link_type: String,
}

fn default_link_type() -> String {
    "other".to_string()
}

#[derive(Debug, Deserialize)]
struct V4Dependency {
    from: String,
    to: String,
    dep_type: Option<String>,
}

/// POST /api/v1/import/json
/// Import a v4 native JSON map into the database.
pub async fn import_json_map(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<JsonImportRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Parse JSON
    let import: V4Import = serde_json::from_str(&body.json).map_err(|e| {
        tracing::warn!("JSON parse error: {}", e);
        ApiError::Validation(format!("Invalid JSON: {}", e))
    })?;

    // Validate format version (if present)
    if let Some(ref ver) = import.format_version {
        if !ver.starts_with("4.") {
            return Err(ApiError::Validation(format!(
                "Unsupported format version '{}'. Expected 4.x",
                ver
            )));
        }
    }

    let app_data = &import.application;
    let mut warnings: Vec<String> = Vec::new();

    let app_id = Uuid::new_v4();

    // Log import action BEFORE creating
    log_action(
        &state.db,
        user.user_id,
        "import_json",
        "application",
        app_id,
        json!({"name": &app_data.name}),
    )
    .await?;

    // Create application
    let tags_json = serde_json::to_value(&app_data.tags).unwrap_or(Value::Null);
    sqlx::query(
        "INSERT INTO applications (id, name, description, organization_id, site_id, tags) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(&app_data.name)
    .bind(&app_data.description)
    .bind(user.organization_id)
    .bind(body.site_id)
    .bind(&tags_json)
    .execute(&state.db)
    .await?;

    // Grant owner to importing user
    let _ = sqlx::query(
        "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) VALUES ($1, $2, 'owner', $2)",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(user.user_id)
    .execute(&state.db)
    .await;

    // ── Import variables ────────────────────────────────────────────
    let mut variables_created = 0;
    for var in &app_data.variables {
        sqlx::query(
            "INSERT INTO app_variables (application_id, name, value, description, is_secret) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(crate::db::bind_id(app_id))
        .bind(&var.name)
        .bind(&var.value)
        .bind(&var.description)
        .bind(var.is_secret)
        .execute(&state.db)
        .await?;

        variables_created += 1;
    }

    // ── Import groups ───────────────────────────────────────────────
    let mut group_map: HashMap<String, Uuid> = HashMap::new();
    let mut groups_created = 0;

    for group in &app_data.groups {
        let group_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO component_groups (id, application_id, name, description, color, display_order) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(group_id)
        .bind(crate::db::bind_id(app_id))
        .bind(&group.name)
        .bind(&group.description)
        .bind(&group.color)
        .bind(group.display_order)
        .execute(&state.db)
        .await?;

        group_map.insert(group.name.clone(), group_id);
        groups_created += 1;
    }

    // ── Import components ───────────────────────────────────────────
    let mut comp_name_to_id: HashMap<String, Uuid> = HashMap::new();
    let mut components_created = 0;
    let mut commands_created = 0;
    let mut links_created = 0;

    for (idx, comp) in app_data.components.iter().enumerate() {
        let comp_id = Uuid::new_v4();

        // Resolve group by name
        let group_id = comp.group.as_ref().and_then(|g| {
            let id = group_map.get(g).copied();
            if id.is_none() && !g.is_empty() {
                warnings.push(format!(
                    "Component '{}' references unknown group '{}'",
                    comp.name, g
                ));
            }
            id
        });

        let comp_type = comp.component_type.as_deref().unwrap_or("service");
        let icon = comp
            .icon
            .as_deref()
            .unwrap_or(default_icon_for_type(comp_type));

        // Extract commands
        let check_cmd = comp.commands.check.as_ref().map(|c| c.cmd.clone());
        let start_cmd = comp.commands.start.as_ref().map(|c| c.cmd.clone());
        let stop_cmd = comp.commands.stop.as_ref().map(|c| c.cmd.clone());
        let integrity_cmd = comp
            .commands
            .integrity_check
            .as_ref()
            .map(|c| c.cmd.clone());
        let post_start_cmd = comp
            .commands
            .post_start_check
            .as_ref()
            .map(|c| c.cmd.clone());
        let infra_cmd = comp.commands.infra_check.as_ref().map(|c| c.cmd.clone());
        let rebuild_cmd = comp.commands.rebuild.as_ref().map(|c| c.cmd.clone());
        let rebuild_infra_cmd = comp.commands.rebuild_infra.as_ref().map(|c| c.cmd.clone());

        // Calculate grid position if not specified
        let pos_x = comp.position_x.unwrap_or((idx % 5) as f32 * 250.0);
        let pos_y = comp.position_y.unwrap_or((idx / 5) as f32 * 200.0);

        // Convert cluster_nodes to JSONB
        let cluster_nodes_json: Option<serde_json::Value> = comp
            .cluster_nodes
            .as_ref()
            .map(|nodes| serde_json::json!(nodes));

        sqlx::query(
            r#"INSERT INTO components (
                id, application_id, name, display_name, description, component_type,
                icon, group_id, host, check_cmd, start_cmd, stop_cmd,
                integrity_check_cmd, post_start_check_cmd, infra_check_cmd,
                rebuild_cmd, rebuild_infra_cmd,
                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds,
                is_optional, position_x, position_y, cluster_size, cluster_nodes
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25
            )"#,
        )
        .bind(crate::db::bind_id(comp_id))
        .bind(crate::db::bind_id(app_id))
        .bind(&comp.name)
        .bind(&comp.display_name)
        .bind(&comp.description)
        .bind(comp_type)
        .bind(icon)
        .bind(group_id)
        .bind(&comp.host)
        .bind(&check_cmd)
        .bind(&start_cmd)
        .bind(&stop_cmd)
        .bind(&integrity_cmd)
        .bind(&post_start_cmd)
        .bind(&infra_cmd)
        .bind(&rebuild_cmd)
        .bind(&rebuild_infra_cmd)
        .bind(comp.check_interval_seconds)
        .bind(comp.start_timeout_seconds)
        .bind(comp.stop_timeout_seconds)
        .bind(comp.is_optional)
        .bind(pos_x)
        .bind(pos_y)
        .bind(comp.cluster_size)
        .bind(&cluster_nodes_json)
        .execute(&state.db)
        .await?;

        comp_name_to_id.insert(comp.name.clone(), comp_id);
        components_created += 1;

        // ── Import custom commands ─────────────────────────────────────
        for custom_cmd in &comp.custom_commands {
            let cmd_id = Uuid::new_v4();

            sqlx::query(
                r#"INSERT INTO component_commands (id, component_id, name, command, description, requires_confirmation)
                VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(cmd_id)
            .bind(crate::db::bind_id(comp_id))
            .bind(&custom_cmd.name)
            .bind(&custom_cmd.command)
            .bind(&custom_cmd.description)
            .bind(custom_cmd.requires_confirmation)
            .execute(&state.db)
            .await?;

            commands_created += 1;

            // Import parameters
            for (pidx, param) in custom_cmd.parameters.iter().enumerate() {
                let enum_vals_json = param
                    .enum_values
                    .as_ref()
                    .and_then(|v| serde_json::to_value(v).ok());

                sqlx::query(
                    r#"INSERT INTO command_input_params (
                        command_id, name, description, default_value, validation_regex,
                        required, param_type, enum_values, display_order
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
                )
                .bind(cmd_id)
                .bind(&param.name)
                .bind(&param.description)
                .bind(&param.default_value)
                .bind(&param.validation_regex)
                .bind(param.required)
                .bind(&param.param_type)
                .bind(&enum_vals_json)
                .bind(pidx as i32)
                .execute(&state.db)
                .await?;
            }
        }

        // ── Import links ───────────────────────────────────────────────
        for (lidx, link) in comp.links.iter().enumerate() {
            sqlx::query(
                "INSERT INTO component_links (component_id, label, url, link_type, display_order) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(crate::db::bind_id(comp_id))
            .bind(&link.label)
            .bind(&link.url)
            .bind(&link.link_type)
            .bind(lidx as i32)
            .execute(&state.db)
            .await?;

            links_created += 1;
        }
    }

    // ── Import dependencies ─────────────────────────────────────────
    let mut dependencies_created = 0;

    for dep in &app_data.dependencies {
        let from_id = match comp_name_to_id.get(&dep.from) {
            Some(id) => *id,
            None => {
                warnings.push(format!(
                    "Dependency from '{}' to '{}': source component not found",
                    dep.from, dep.to
                ));
                continue;
            }
        };

        let to_id = match comp_name_to_id.get(&dep.to) {
            Some(id) => *id,
            None => {
                warnings.push(format!(
                    "Dependency from '{}' to '{}': target component not found",
                    dep.from, dep.to
                ));
                continue;
            }
        };

        let dep_type = dep.dep_type.as_deref().unwrap_or("strong");

        sqlx::query(
            "INSERT INTO dependencies (application_id, from_component_id, to_component_id, dep_type) VALUES ($1, $2, $3, $4)",
        )
        .bind(crate::db::bind_id(app_id))
        .bind(from_id)
        .bind(to_id)
        .bind(dep_type)
        .execute(&state.db)
        .await?;

        dependencies_created += 1;
    }

    // Validate DAG (no cycles)
    let dag_result = crate::core::dag::build_dag(&state.db, app_id).await;
    if let Ok(dag) = dag_result {
        if let Err(cycle_err) = dag.topological_levels() {
            warnings.push(format!("Warning: DAG contains a cycle - {}", cycle_err));
        }
    }

    let result = ImportResult {
        application_id: app_id,
        application_name: app_data.name.clone(),
        components_created,
        groups_created,
        variables_created,
        commands_created,
        dependencies_created,
        links_created,
        warnings,
    };

    Ok((StatusCode::CREATED, Json(json!(result))))
}
