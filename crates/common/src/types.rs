use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The 8 possible states for a component in the FSM.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumString,
    strum::Display,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComponentState {
    Unknown,
    Running,
    Degraded,
    Failed,
    Stopped,
    Starting,
    Stopping,
    Unreachable,
}

/// Permission levels (per application). Ordered: None < View < Operate < Edit < Manage < Owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    None = 0,
    View = 1,
    Operate = 2,
    Edit = 3,
    Manage = 4,
    Owner = 5,
}

impl PermissionLevel {
    pub fn from_str_level(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "view" => Some(Self::View),
            "operate" => Some(Self::Operate),
            "edit" => Some(Self::Edit),
            "manage" => Some(Self::Manage),
            "owner" => Some(Self::Owner),
            _ => None,
        }
    }
}

/// Types of checks that can be run on a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    Health,
    Integrity,
    PostStart,
    Infrastructure,
}

/// Result of a check execution on a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub component_id: Uuid,
    pub check_type: CheckType,
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub duration_ms: u32,
    pub at: DateTime<Utc>,
}

/// Status for a diagnostic check level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Ok,
    Fail,
    NotAvailable,
}

/// Diagnostic recommendation based on the 3-level assessment matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DiagnosticRecommendation {
    Healthy,
    Restart,
    AppRebuild,
    InfraRebuild,
    Unknown,
}

/// Result of a command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub request_id: Uuid,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u32,
}

/// Component types.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumString,
    strum::Display,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ComponentType {
    Database,
    Middleware,
    Appserver,
    Webfront,
    Service,
    Batch,
    Custom,
}

/// Configuration for a component pushed to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentConfig {
    pub component_id: Uuid,
    pub name: String,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub post_start_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    pub rebuild_infra_cmd: Option<String>,
    pub check_interval_seconds: u32,
    pub start_timeout_seconds: u32,
    pub stop_timeout_seconds: u32,
    pub env_vars: serde_json::Value,
}

/// Switchover phases for DR failover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwitchoverPhase {
    Prepare,
    Validate,
    StopSource,
    Sync,
    StartTarget,
    Commit,
}

/// Switchover modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwitchoverMode {
    Full,
    Selective,
    Progressive,
}

/// User roles within an organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrgRole {
    Admin,
    Operator,
    Editor,
    Viewer,
}

impl OrgRole {
    pub fn is_admin(&self) -> bool {
        matches!(self, OrgRole::Admin)
    }
}

// ---------------------------------------------------------------------------
// Discovery types (passive topology scanning)
// ---------------------------------------------------------------------------

/// A process discovered by the agent's passive scanner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredProcess {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    pub user: String,
    pub memory_bytes: u64,
    pub cpu_pct: f32,
    pub listening_ports: Vec<u16>,
}

/// A TCP listener discovered on the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredListener {
    pub port: u16,
    pub protocol: String,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub address: String,
}

/// An outbound TCP connection observed on the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredConnection {
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub state: String,
}

/// A system service (systemd unit / Windows service) discovered on the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredService {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub pid: Option<u32>,
}

// ---------------------------------------------------------------------------
// Air-gap agent update types
// ---------------------------------------------------------------------------

/// Status of an in-progress agent binary update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    Downloading,
    Verifying,
    Applying,
    Complete,
    Failed,
}
