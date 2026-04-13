//! Shared types for import wizard.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::resolution::AvailableAgent;

// ══════════════════════════════════════════════════════════════════════
// Preview types
// ══════════════════════════════════════════════════════════════════════

/// Request to preview import resolution
#[derive(Debug, Deserialize)]
pub struct ImportPreviewRequest {
    pub content: String,
    pub format: String,
    pub gateway_ids: Vec<Uuid>,
    pub dr_gateway_ids: Option<Vec<Uuid>>,
}

/// Response from import preview
#[derive(Debug, Serialize)]
pub struct ImportPreviewResponse {
    pub valid: bool,
    pub application_name: String,
    pub component_count: usize,
    pub all_resolved: bool,
    pub components: Vec<ComponentResolution>,
    pub available_agents: Vec<AvailableAgent>,
    pub dr_available_agents: Option<Vec<AvailableAgent>>,
    pub dr_suggestions: Option<Vec<DrSuggestion>>,
    pub warnings: Vec<String>,
    pub existing_application: Option<ExistingApplicationInfo>,
}

#[derive(Debug, Serialize)]
pub struct ExistingApplicationInfo {
    pub id: Uuid,
    pub name: String,
    pub component_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ComponentResolution {
    pub name: String,
    pub host: Option<String>,
    pub component_type: String,
    pub resolution: ComponentResolutionStatus,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ComponentResolutionStatus {
    Resolved {
        agent_id: Uuid,
        agent_hostname: String,
        gateway_id: Option<Uuid>,
        gateway_name: Option<String>,
        resolved_via: String,
    },
    Multiple {
        candidates: Vec<AgentCandidateDto>,
    },
    Unresolved,
    NoHost,
}

#[derive(Debug, Serialize)]
pub struct AgentCandidateDto {
    pub agent_id: Uuid,
    pub hostname: String,
    pub gateway_id: Option<Uuid>,
    pub gateway_name: Option<String>,
    pub ip_addresses: Vec<String>,
    pub matched_via: String,
}

#[derive(Debug, Serialize)]
pub struct DrSuggestion {
    pub component_name: String,
    pub primary_host: String,
    pub suggested_dr_host: Option<String>,
    pub dr_resolution: Option<ComponentResolutionStatus>,
}

// ══════════════════════════════════════════════════════════════════════
// Execute types
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictAction {
    #[default]
    Fail,
    Rename,
    Update,
}

#[derive(Debug, Deserialize)]
pub struct ImportExecuteRequest {
    pub content: String,
    pub format: String,
    pub site_id: Option<Uuid>,
    pub profile: ProfileConfig,
    /// Single DR profile (backward compat — prefer dr_profiles)
    pub dr_profile: Option<ProfileConfig>,
    /// Multiple DR profiles (one per DR site)
    #[serde(default)]
    pub dr_profiles: Vec<ProfileConfig>,
    #[serde(default)]
    pub conflict_action: ConflictAction,
    pub new_name: Option<String>,
}

impl ImportExecuteRequest {
    /// Returns all DR profiles: merges the legacy single `dr_profile` with `dr_profiles` vec.
    pub fn all_dr_profiles(&self) -> Vec<&ProfileConfig> {
        let mut result: Vec<&ProfileConfig> = self.dr_profiles.iter().collect();
        if let Some(ref single) = self.dr_profile {
            // Only add if not already present by name
            if !result.iter().any(|p| p.name == single.name) {
                result.push(single);
            }
        }
        result
    }
}

#[derive(Debug, Deserialize)]
pub struct ProfileConfig {
    pub name: String,
    pub description: Option<String>,
    pub profile_type: String,
    pub gateway_ids: Vec<Uuid>,
    pub auto_failover: Option<bool>,
    pub mappings: Vec<MappingConfig>,
}

#[derive(Debug, Deserialize)]
pub struct MappingConfig {
    pub component_name: String,
    pub agent_id: Uuid,
    pub resolved_via: String,
}

#[derive(Debug, Serialize)]
pub struct ImportExecuteResponse {
    pub application_id: Uuid,
    pub application_name: String,
    pub components_created: usize,
    pub profiles_created: Vec<String>,
    pub active_profile: String,
    pub warnings: Vec<String>,
}

// ══════════════════════════════════════════════════════════════════════
// Internal data structures (parsing)
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub(crate) struct ImportData {
    #[allow(dead_code)]
    pub format_version: Option<String>,
    pub application: ApplicationData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApplicationData {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_tags")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub variables: Vec<VariableData>,
    #[serde(default)]
    pub groups: Vec<GroupData>,
    #[serde(default)]
    pub components: Vec<ComponentData>,
    #[serde(default)]
    pub dependencies: Vec<DependencyData>,
    #[serde(default)]
    pub binding_profiles: Vec<WizardBindingProfile>,
    #[serde(flatten)]
    pub _extra: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub(crate) struct WizardBindingProfile {
    pub name: Option<String>,
    #[serde(default)]
    pub profile_type: Option<String>,
    pub site: Option<WizardProfileSite>,
    #[serde(default)]
    pub mappings: Vec<WizardBindingMapping>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct WizardProfileSite {
    pub code: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WizardBindingMapping {
    pub component_name: Option<String>,
    pub host: Option<String>,
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    use serde_json::Value;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>()
            .pipe(Ok),
        Value::Object(obj) => Ok(obj
            .into_iter()
            .map(|(k, v)| format!("{}:{}", k, v.as_str().unwrap_or(&v.to_string())))
            .collect()),
        Value::Null => Ok(Vec::new()),
        _ => Err(de::Error::custom("tags must be array or object")),
    }
}

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}
impl<T> Pipe for T {}

#[derive(Debug, Deserialize)]
pub(crate) struct VariableData {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GroupData {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ComponentData {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(alias = "type")]
    pub component_type: Option<String>,
    pub icon: Option<String>,
    pub group: Option<String>,
    pub host: Option<String>,
    #[serde(default)]
    pub commands: CommandsData,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    #[serde(default)]
    pub custom_commands: Vec<CustomCommandData>,
    #[serde(default)]
    pub links: Vec<LinkData>,
    pub position: Option<PositionData>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    #[serde(default = "default_check_interval", alias = "check_interval_secs")]
    pub check_interval_seconds: i32,
    #[serde(default = "default_start_timeout", alias = "start_timeout_secs")]
    pub start_timeout_seconds: i32,
    #[serde(default = "default_stop_timeout", alias = "stop_timeout_secs")]
    pub stop_timeout_seconds: i32,
    #[serde(default)]
    pub is_optional: bool,
    #[serde(default, alias = "protected")]
    pub rebuild_protected: bool,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Vec<String>>,
    #[serde(default)]
    pub site_overrides: Vec<SiteOverrideData>,
    #[serde(flatten)]
    pub _extra: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SiteOverrideData {
    pub site_code: String,
    pub host_override: Option<String>,
    pub check_cmd_override: Option<String>,
    pub start_cmd_override: Option<String>,
    pub stop_cmd_override: Option<String>,
    pub rebuild_cmd_override: Option<String>,
    pub env_vars_override: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PositionData {
    pub x: f32,
    pub y: f32,
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
pub(crate) struct CommandsData {
    pub check: Option<CommandDetail>,
    pub start: Option<CommandDetail>,
    pub stop: Option<CommandDetail>,
    pub integrity_check: Option<CommandDetail>,
    pub post_start_check: Option<CommandDetail>,
    pub infra_check: Option<CommandDetail>,
    pub rebuild: Option<CommandDetail>,
    pub rebuild_infra: Option<CommandDetail>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CommandDetail {
    pub cmd: String,
    #[allow(dead_code)]
    pub timeout_seconds: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CustomCommandData {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub parameters: Vec<CommandParamData>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CommandParamData {
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default = "default_param_type")]
    pub param_type: String,
    pub enum_values: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}
fn default_param_type() -> String {
    "string".to_string()
}

#[derive(Debug, Deserialize)]
pub(crate) struct LinkData {
    pub label: String,
    pub url: String,
    #[serde(default = "default_link_type")]
    pub link_type: String,
}

fn default_link_type() -> String {
    "other".to_string()
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct DependencyData {
    pub from: String,
    pub to: String,
    #[serde(alias = "type")]
    pub dep_type: Option<String>,
}

// ══════════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════════

use crate::error::ApiError;

pub(crate) fn parse_import_content(content: &str, format: &str) -> Result<ImportData, ApiError> {
    match format.to_lowercase().as_str() {
        "json" => {
            if let Ok(data) = serde_json::from_str::<ImportData>(content) {
                return Ok(data);
            }
            serde_json::from_str::<ApplicationData>(content)
                .map(|app| ImportData {
                    format_version: None,
                    application: app,
                })
                .map_err(|e| {
                    tracing::warn!("JSON parse error: {}", e);
                    ApiError::Validation(format!("Invalid JSON: {}", e))
                })
        }
        "yaml" | "yml" => {
            if let Ok(data) = serde_yaml::from_str::<ImportData>(content) {
                return Ok(data);
            }
            serde_yaml::from_str::<ApplicationData>(content)
                .map(|app| ImportData {
                    format_version: None,
                    application: app,
                })
                .map_err(|e| {
                    tracing::warn!("YAML parse error: {}", e);
                    ApiError::Validation(format!("Invalid YAML: {}", e))
                })
        }
        _ => Err(ApiError::Validation(format!(
            "Unsupported format '{}'. Use 'json' or 'yaml'",
            format
        ))),
    }
}

pub(crate) fn default_icon_for_type(comp_type: &str) -> &'static str {
    match comp_type.to_lowercase().as_str() {
        "database" | "db" => "database",
        "middleware" | "mq" | "queue" | "messaging" | "layers" => "layers",
        "appserver" | "app" | "application" | "server" => "server",
        "webfront" | "web" | "webserver" | "frontend" => "globe",
        "service" | "svc" | "api" => "cog",
        "batch" | "job" | "scheduler" => "clock",
        "loadbalancer" | "lb" | "proxy" | "gateway" => "network",
        "cache" | "redis" | "memcached" => "zap",
        _ => "box",
    }
}
