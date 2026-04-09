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
    crate::repository::import_queries::create_import_application(
        &state.db,
        app_id,
        app_name,
        app_desc,
        *user.organization_id,
        body.site_id,
    )
    .await?;

    // Grant owner to importing user
    crate::repository::import_queries::grant_owner_permission(&state.db, app_id, *user.user_id)
        .await?;

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

        crate::repository::import_queries::create_app_variable(
            &state.db,
            app_id,
            name,
            value,
            var.description.as_deref(),
        )
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
                crate::repository::import_queries::create_component_group(
                    &state.db,
                    group_id,
                    app_id,
                    group_name,
                    groups_created as i32,
                )
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

        crate::repository::import_queries::create_import_component_yaml(
            &state.db,
            comp_id,
            app_id,
            comp_name,
            comp.display_name.as_deref(),
            comp.description.as_deref(),
            comp_type,
            icon,
            group_id,
            &check_cmd,
            &start_cmd,
            &stop_cmd,
            &integrity_cmd,
            pos_x,
            pos_y,
        )
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

            crate::repository::import_queries::create_component_command(
                &state.db,
                cmd_id,
                comp_id,
                display,
                cmd_text,
                action.description.as_deref(),
                action.requires_confirmation.unwrap_or(false),
            )
            .await?;

            commands_created += 1;

            // Import input parameters for this command
            for (pidx, param) in action.parameters.iter().enumerate() {
                let param_name = match &param.name {
                    Some(n) => n,
                    None => continue,
                };

                crate::repository::import_queries::create_command_input_param(
                    &state.db,
                    cmd_id,
                    param_name,
                    param.description.as_deref(),
                    param.default_value.as_deref(),
                    param.validation_regex.as_deref(),
                    param.required.unwrap_or(true),
                    pidx as i32,
                )
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

            crate::repository::import_queries::create_component_link(
                &state.db, comp_id, label, url, link_type,
            )
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

            crate::repository::import_queries::create_dependency(&state.db, app_id, from_id, to_id)
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

    // Notify agents about new components so they start health checks
    crate::websocket::push_config_to_affected_agents(&state, Some(app_id), None, None).await;

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
    #[serde(default)]
    binding_profiles: Vec<V4BindingProfile>,
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

#[derive(Debug, Deserialize)]
struct V4BindingProfile {
    name: String,
    #[serde(default = "default_profile_type")]
    profile_type: String,
    #[serde(default)]
    is_active: bool,
    description: Option<String>,
    /// Target site for this profile (optional - resolved by site code/name)
    site: Option<V4ProfileSite>,
    #[serde(default)]
    mappings: Vec<V4BindingMapping>,
}

#[derive(Debug, Deserialize)]
struct V4ProfileSite {
    name: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct V4BindingMapping {
    component_name: String,
    host: String,
    #[serde(default = "default_resolved_via")]
    resolved_via: String,
}

fn default_profile_type() -> String {
    "custom".to_string()
}
fn default_resolved_via() -> String {
    "manual".to_string()
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
    crate::repository::import_queries::create_import_application_with_tags(
        &state.db,
        app_id,
        &app_data.name,
        app_data.description.as_deref(),
        *user.organization_id,
        body.site_id,
        &tags_json,
    )
    .await?;

    // Grant owner to importing user
    crate::repository::import_queries::grant_owner_permission(&state.db, app_id, *user.user_id)
        .await?;

    // ── Import variables ────────────────────────────────────────────
    let mut variables_created = 0;
    for var in &app_data.variables {
        crate::repository::import_queries::create_app_variable_with_secret(
            &state.db,
            app_id,
            &var.name,
            &var.value,
            var.description.as_deref(),
            var.is_secret,
        )
        .await?;

        variables_created += 1;
    }

    // ── Import groups ───────────────────────────────────────────────
    let mut group_map: HashMap<String, Uuid> = HashMap::new();
    let mut groups_created = 0;

    for group in &app_data.groups {
        let group_id = Uuid::new_v4();
        crate::repository::import_queries::create_component_group_full(
            &state.db,
            group_id,
            app_id,
            &group.name,
            group.description.as_deref(),
            group.color.as_deref(),
            group.display_order,
        )
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

        crate::repository::import_queries::create_import_component_json(
            &state.db,
            comp_id,
            app_id,
            &comp.name,
            comp.display_name.as_deref(),
            comp.description.as_deref(),
            comp_type,
            icon,
            group_id,
            comp.host.as_deref(),
            &check_cmd,
            &start_cmd,
            &stop_cmd,
            &integrity_cmd,
            &post_start_cmd,
            &infra_cmd,
            &rebuild_cmd,
            &rebuild_infra_cmd,
            comp.check_interval_seconds,
            comp.start_timeout_seconds,
            comp.stop_timeout_seconds,
            comp.is_optional,
            pos_x,
            pos_y,
            comp.cluster_size,
            &cluster_nodes_json,
        )
        .await?;

        comp_name_to_id.insert(comp.name.clone(), comp_id);
        components_created += 1;

        // ── Import custom commands ─────────────────────────────────────
        for custom_cmd in &comp.custom_commands {
            let cmd_id = Uuid::new_v4();

            crate::repository::import_queries::create_component_command(
                &state.db,
                cmd_id,
                comp_id,
                &custom_cmd.name,
                &custom_cmd.command,
                custom_cmd.description.as_deref(),
                custom_cmd.requires_confirmation,
            )
            .await?;

            commands_created += 1;

            // Import parameters
            for (pidx, param) in custom_cmd.parameters.iter().enumerate() {
                let enum_vals_json = param
                    .enum_values
                    .as_ref()
                    .and_then(|v| serde_json::to_value(v).ok());

                crate::repository::import_queries::create_command_input_param_full(
                    &state.db,
                    cmd_id,
                    &param.name,
                    param.description.as_deref(),
                    param.default_value.as_deref(),
                    param.validation_regex.as_deref(),
                    param.required,
                    &param.param_type,
                    &enum_vals_json,
                    pidx as i32,
                )
                .await?;
            }
        }

        // ── Import links ───────────────────────────────────────────────
        for (lidx, link) in comp.links.iter().enumerate() {
            crate::repository::import_queries::create_component_link_ordered(
                &state.db,
                comp_id,
                &link.label,
                &link.url,
                &link.link_type,
                lidx as i32,
            )
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

        crate::repository::import_queries::create_dependency_typed(
            &state.db, app_id, from_id, to_id, dep_type,
        )
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

    // ── Import binding profiles ────────────────────────────────────
    let mut _profiles_created = 0;

    for profile in &app_data.binding_profiles {
        // Resolve target site from profile.site (by code or name)
        let mut gateway_ids: Vec<Uuid> = Vec::new();
        if let Some(ref site_ref) = profile.site {
            // Try to find site by code first, then by name
            let site_id: Option<Uuid> = if let Some(ref code) = site_ref.code {
                crate::repository::import_queries::find_site_by_code(
                    &state.db,
                    *user.organization_id,
                    code,
                )
                .await
                .ok()
                .flatten()
                .map(|(id,)| id.into_inner())
            } else if let Some(ref name) = site_ref.name {
                crate::repository::misc_queries::find_site_by_name(
                    &state.db,
                    *user.organization_id,
                    name,
                )
                .await
                .ok()
                .flatten()
            } else {
                None
            };

            // Find gateways for this site
            if let Some(sid) = site_id {
                if let Ok(gw_ids) =
                    crate::repository::misc_queries::get_gateway_ids_for_site(&state.db, sid).await
                {
                    gateway_ids = gw_ids.into_iter().map(|g| g.into_inner()).collect();
                }
            } else {
                warnings.push(format!(
                    "Binding profile '{}': target site not found (code={:?}, name={:?})",
                    profile.name, site_ref.code, site_ref.name,
                ));
            }
        }

        // Create the binding profile
        let profile_id = Uuid::new_v4();
        let gw_array = crate::db::UuidArray(gateway_ids.clone());
        if let Err(e) = crate::repository::import_queries::create_binding_profile(
            &state.db,
            profile_id,
            app_id,
            &profile.name,
            profile.description.as_deref(),
            &profile.profile_type,
            profile.is_active,
            gw_array,
            false, // auto_failover
            *user.user_id,
        )
        .await
        {
            warnings.push(format!(
                "Binding profile '{}': creation failed - {}",
                profile.name, e
            ));
            continue;
        }

        // Create mappings - resolve agent_id from host
        for mapping in &profile.mappings {
            // Try to resolve agent_id from hostname (search across all agents in the org)
            let agent_id = crate::repository::misc_queries::find_agent_by_hostname(
                &state.db,
                *user.organization_id,
                &mapping.host,
            )
            .await
            .ok()
            .flatten();

            if let Some(aid) = agent_id {
                if let Err(e) = crate::repository::import_queries::create_binding_profile_mapping(
                    &state.db,
                    profile_id,
                    &mapping.component_name,
                    &mapping.host,
                    aid,
                    &mapping.resolved_via,
                )
                .await
                {
                    warnings.push(format!(
                        "Binding profile '{}' mapping for '{}': {}",
                        profile.name, mapping.component_name, e
                    ));
                }
            } else {
                warnings.push(format!(
                    "Binding profile '{}' mapping for '{}': agent not found for host '{}'",
                    profile.name, mapping.component_name, mapping.host
                ));
            }
        }

        _profiles_created += 1;
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

    // Notify agents about new components so they start health checks
    crate::websocket::push_config_to_affected_agents(&state, Some(app_id), None, None).await;

    Ok((StatusCode::CREATED, Json(json!(result))))
}
