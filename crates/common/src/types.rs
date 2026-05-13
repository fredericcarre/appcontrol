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
    utoipa::ToSchema,
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
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
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

    /// Returns true if this permission level allows viewing resources
    pub fn can_view(&self) -> bool {
        *self >= PermissionLevel::View
    }

    /// Returns true if this permission level allows operating (start/stop) resources
    pub fn can_operate(&self) -> bool {
        *self >= PermissionLevel::Operate
    }

    /// Returns true if this permission level allows editing resources
    pub fn can_edit(&self) -> bool {
        *self >= PermissionLevel::Edit
    }

    /// Returns true if this permission level allows managing permissions
    pub fn can_manage(&self) -> bool {
        *self >= PermissionLevel::Manage
    }

    /// Returns true if this permission level is owner
    pub fn is_owner(&self) -> bool {
        *self >= PermissionLevel::Owner
    }
}

/// Types of checks that can be run on a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    Health,
    Integrity,
    PostStart,
    Infrastructure,
}

/// Result of a check execution on a component.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CheckResult {
    pub component_id: Uuid,
    pub check_type: CheckType,
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub duration_ms: u32,
    pub at: DateTime<Utc>,
    /// Generic metrics extracted from stdout (any valid JSON).
    ///
    /// Check commands can return JSON to provide rich operational data:
    /// - `{"active_users": 12, "users": ["Alice", "Bob"]}`
    /// - `{"queue_depth": 150, "consumers": 3}`
    /// - `{"connections": 45, "replication_lag_ms": 10}`
    ///
    /// The frontend renders this generically without interpreting the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub metrics: Option<serde_json::Value>,
    /// When set, this result concerns a single member of a fan-out cluster.
    /// Absent for regular components or aggregate-mode clusters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_member_id: Option<Uuid>,
}

/// Status for a diagnostic check level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Ok,
    Fail,
    NotAvailable,
}

/// Diagnostic recommendation based on the 3-level assessment matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum DiagnosticRecommendation {
    Healthy,
    Restart,
    AppRebuild,
    InfraRebuild,
    Unknown,
}

/// Result of a command execution.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    utoipa::ToSchema,
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    #[schema(value_type = Object)]
    pub env_vars: serde_json::Value,
    /// When non-empty, this component is in fan-out cluster mode and the agent
    /// must run checks/commands per member instead of at the component level.
    /// The backend pre-filters this list to members assigned to the receiving
    /// agent (via `cluster_members.agent_id`).
    #[serde(default)]
    pub cluster_members: Vec<ClusterMemberConfig>,
    /// Optional native (non-shell) check / start / stop specs. When set the
    /// agent runs them via its built-in runners (HTTP probe, TCP connect,
    /// process probe) instead of spawning a shell — useful for hosts that
    /// don't have curl/wget/comparable utilities installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_native: Option<NativeCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_native: Option<NativeCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_native: Option<NativeCommand>,
}

/// Typed, agent-runnable command. Same payload shape on the wire as in the
/// `*_native` JSON columns on `components`. The discriminator is `kind`.
///
/// Today only `http` is implemented end-to-end; `tcp` and `process` are
/// scaffolded so we can add them without a protocol break.
///
/// The `Debug` impl is custom so secrets (`bearer_token`, `Authorization`
/// headers) never leak into `tracing` output or audit logs that include
/// command details. Use the auto `Serialize` for on-wire transit (the
/// agent needs the real value to dial), but `redacted()` for anywhere a
/// human might read the result back.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeCommand {
    /// HTTP probe. Considered successful if the response status is in the
    /// 2xx range, or matches `expect_status` exactly when set.
    Http {
        #[serde(default = "default_http_method")]
        method: String,
        url: String,
        /// Free-form headers. When `bearer_token` is set, the agent adds an
        /// `Authorization: Bearer <token>` automatically (and an explicit
        /// `Authorization` entry here wins over it for callers who need
        /// non-bearer schemes).
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
        /// Convenience for `Authorization: Bearer …`. Kept on its own field
        /// so operators don't have to know the header name and so we can
        /// mask it in Debug / audit output. None = no bearer auth.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bearer_token: Option<String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        expect_status: Option<u16>,
        /// Substring that must appear in the response body. None = any body.
        #[serde(default)]
        expect_body_contains: Option<String>,
        #[serde(default = "default_http_timeout_seconds")]
        timeout_seconds: u32,
        /// Skip TLS verification — for self-signed prod gear.
        #[serde(default)]
        insecure: bool,
    },
    /// TCP connect probe. Successful if the agent can open a TCP socket to
    /// `host:port` within `timeout_seconds`.
    TcpConnect {
        host: String,
        port: u16,
        #[serde(default = "default_tcp_timeout_seconds")]
        timeout_seconds: u32,
    },
}

impl NativeCommand {
    /// Substitute `{hostname}` and `{install_path}` placeholders in every
    /// string-shaped field that an operator might want to template per
    /// fan-out member: HTTP url / headers / body / expect_body_contains
    /// for `Http`, `host` for `TcpConnect`. Anything that isn't a recognised
    /// placeholder is left alone — including stray `{}` braces that just
    /// happen to look like one. `install_path = None` deletes the
    /// placeholder rather than leaving the literal string in.
    ///
    /// This is the templating step the agent applies when a fan-out parent
    /// has `*_native` set and a specific member has no override of its
    /// own — same model as `check_cmd`/`start_cmd`/`stop_cmd` on the shell
    /// side, except substitution is field-aware.
    pub fn templated_for_member(&self, hostname: &str, install_path: Option<&str>) -> Self {
        let install_path = install_path.unwrap_or("");
        let render = |s: &str| -> String {
            s.replace("{hostname}", hostname)
                .replace("{install_path}", install_path)
        };
        match self {
            NativeCommand::Http {
                method,
                url,
                headers,
                bearer_token,
                body,
                expect_status,
                expect_body_contains,
                timeout_seconds,
                insecure,
            } => NativeCommand::Http {
                method: method.clone(),
                url: render(url),
                headers: headers
                    .iter()
                    .map(|(k, v)| (k.clone(), render(v)))
                    .collect(),
                bearer_token: bearer_token.clone(),
                body: body.as_deref().map(render),
                expect_status: *expect_status,
                expect_body_contains: expect_body_contains.as_deref().map(render),
                timeout_seconds: *timeout_seconds,
                insecure: *insecure,
            },
            NativeCommand::TcpConnect {
                host,
                port,
                timeout_seconds,
            } => NativeCommand::TcpConnect {
                host: render(host),
                port: *port,
                timeout_seconds: *timeout_seconds,
            },
        }
    }

    /// Return a copy where any secret-bearing fields are replaced with a
    /// fixed redacted marker. Use this when serialising the command into
    /// audit logs, action_log details, or anything an operator can read
    /// back. The on-wire path to the agent must keep using the real
    /// value, so don't run this on the message you actually send.
    pub fn redacted(&self) -> Self {
        match self {
            NativeCommand::Http {
                method,
                url,
                headers,
                bearer_token,
                body,
                expect_status,
                expect_body_contains,
                timeout_seconds,
                insecure,
            } => {
                let masked_headers = headers
                    .iter()
                    .map(|(k, v)| {
                        if k.eq_ignore_ascii_case("authorization")
                            || k.eq_ignore_ascii_case("x-api-key")
                            || k.eq_ignore_ascii_case("cookie")
                        {
                            (k.clone(), "***".to_string())
                        } else {
                            (k.clone(), v.clone())
                        }
                    })
                    .collect();
                NativeCommand::Http {
                    method: method.clone(),
                    url: url.clone(),
                    headers: masked_headers,
                    bearer_token: bearer_token.as_ref().map(|_| "***".to_string()),
                    body: body.clone(),
                    expect_status: *expect_status,
                    expect_body_contains: expect_body_contains.clone(),
                    timeout_seconds: *timeout_seconds,
                    insecure: *insecure,
                }
            }
            other => other.clone(),
        }
    }
}

impl std::fmt::Debug for NativeCommand {
    // Routes through `redacted()` so a `tracing::debug!(?cmd, …)` never
    // prints a Bearer token or an Authorization header. Format mirrors
    // what `derive(Debug)` would have produced so existing log greps
    // still work.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.redacted() {
            NativeCommand::Http {
                method,
                url,
                headers,
                bearer_token,
                body,
                expect_status,
                expect_body_contains,
                timeout_seconds,
                insecure,
            } => f
                .debug_struct("Http")
                .field("method", &method)
                .field("url", &url)
                .field("headers", &headers)
                .field("bearer_token", &bearer_token)
                .field("body", &body)
                .field("expect_status", &expect_status)
                .field("expect_body_contains", &expect_body_contains)
                .field("timeout_seconds", &timeout_seconds)
                .field("insecure", &insecure)
                .finish(),
            NativeCommand::TcpConnect {
                host,
                port,
                timeout_seconds,
            } => f
                .debug_struct("TcpConnect")
                .field("host", &host)
                .field("port", &port)
                .field("timeout_seconds", &timeout_seconds)
                .finish(),
        }
    }
}

fn default_http_method() -> String {
    "GET".to_string()
}
fn default_http_timeout_seconds() -> u32 {
    5
}
fn default_tcp_timeout_seconds() -> u32 {
    3
}

/// How cluster members contribute to the parent component's state.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumString,
    strum::Display,
    utoipa::ToSchema,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClusterMode {
    /// External aggregation: the component-level check_cmd is assumed to
    /// already aggregate over hosts (F5, Oracle SCAN, JMX, etc.).
    /// `cluster_size` / `cluster_nodes` (V035) are cosmetic.
    #[default]
    Aggregate,
    /// Per-member execution: each `cluster_members` row is a first-class
    /// entity with its own agent, commands, FSM and history.
    FanOut,
}

/// Policy that derives the parent component's state from member states
/// (only meaningful when `cluster_mode = FanOut`).
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumString,
    strum::Display,
    utoipa::ToSchema,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClusterHealthPolicy {
    /// All RUNNING → RUNNING; any non-RUNNING → DEGRADED; 0 RUNNING → FAILED.
    #[default]
    AllHealthy,
    /// Any RUNNING → RUNNING; 0 RUNNING → FAILED.
    AnyHealthy,
    /// count(RUNNING) ≥ ⌈N/2⌉+1 → RUNNING; else DEGRADED; 0 RUNNING → FAILED.
    Quorum,
    /// %RUNNING ≥ min_healthy_pct → RUNNING; ≥50% → DEGRADED; else FAILED.
    ThresholdPct,
}

/// Configuration for a single fan-out cluster member pushed to its agent.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ClusterMemberConfig {
    pub member_id: Uuid,
    pub hostname: String,
    /// Resolved check command (override or inherited from component).
    pub check_cmd: Option<String>,
    /// Resolved start command.
    pub start_cmd: Option<String>,
    /// Resolved stop command.
    pub stop_cmd: Option<String>,
    /// Merged env vars (component env_vars + per-member override on top).
    #[serde(default)]
    #[schema(value_type = Object)]
    pub env_vars: serde_json::Value,
    /// Optional install path for templating into native URLs/bodies.
    #[serde(default)]
    pub install_path: Option<String>,
    /// Per-member native overrides. None = inherit from parent component
    /// (which the agent then templates with this member's hostname /
    /// install_path). `serde(default)` so older agents and older snapshots
    /// roundtrip cleanly when the column is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_native: Option<NativeCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_native: Option<NativeCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_native: Option<NativeCommand>,
}

/// Switchover phases for DR failover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SwitchoverMode {
    Full,
    Selective,
    Progressive,
}

/// User roles within an organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Technology identification from process/cmdline pattern matching.
/// This enables automatic icon assignment, naming, and layer grouping.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TechnologyHint {
    /// Technology identifier (e.g., "elasticsearch", "rabbitmq", "mysql")
    pub id: String,
    /// Human-readable display name (e.g., "ElasticSearch", "RabbitMQ", "MySQL")
    pub display_name: String,
    /// Icon identifier for frontend (e.g., "elastic", "rabbitmq", "mysql")
    pub icon: String,
    /// Layer/category for grouping (e.g., "Database", "Middleware", "Infrastructure")
    pub layer: String,
}

/// A process discovered by the agent's passive scanner.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredProcess {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    pub user: String,
    /// Windows domain (AD) or empty for local accounts
    #[serde(default)]
    pub domain: Option<String>,
    pub memory_bytes: u64,
    pub cpu_pct: f32,
    pub listening_ports: Vec<u16>,
    /// Key environment variables (filtered: HOME, PATH, DB_*, *_PORT, etc.)
    #[serde(default)]
    pub env_vars: std::collections::HashMap<String, String>,
    /// Working directory of the process (Linux: /proc/[pid]/cwd)
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Config files detected via open file descriptors
    #[serde(default)]
    pub config_files: Vec<DiscoveredConfigFile>,
    /// Log files detected via open file descriptors
    #[serde(default)]
    pub log_files: Vec<DiscoveredLogFile>,
    /// Suggested check/start/stop commands (from service cross-referencing)
    #[serde(default)]
    pub command_suggestion: Option<CommandSuggestion>,
    /// Matched system service name (systemd unit / Windows service)
    #[serde(default)]
    pub matched_service: Option<String>,
    /// Detected technology (icon, display name, layer) from pattern matching
    #[serde(default)]
    pub technology_hint: Option<TechnologyHint>,
}

/// A config file found open by a discovered process.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredConfigFile {
    pub path: String,
    /// Extracted connection-relevant entries (host:port, URLs, DSNs)
    #[serde(default)]
    pub extracted_endpoints: Vec<ExtractedEndpoint>,
}

/// A connection endpoint extracted from a config file.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExtractedEndpoint {
    /// Config key or context (e.g. "spring.datasource.url", "REDIS_HOST")
    pub key: String,
    /// Raw value (e.g. "jdbc:postgresql://db-srv:5432/orders")
    pub value: String,
    /// Parsed hostname if available
    #[serde(default)]
    pub parsed_host: Option<String>,
    /// Parsed port if available
    #[serde(default)]
    pub parsed_port: Option<u16>,
    /// Inferred technology (e.g. "postgresql", "redis", "rabbitmq")
    #[serde(default)]
    pub technology: Option<String>,
}

/// A log file found open by a discovered process.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredLogFile {
    pub path: String,
    pub size_bytes: u64,
}

/// Suggested operational commands for a discovered process.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CommandSuggestion {
    pub check_cmd: String,
    #[serde(default)]
    pub start_cmd: Option<String>,
    #[serde(default)]
    pub stop_cmd: Option<String>,
    #[serde(default)]
    pub restart_cmd: Option<String>,
    /// Command to view logs (e.g., "tail -100 /var/log/mysql.log")
    #[serde(default)]
    pub logs_cmd: Option<String>,
    /// Command to show version (e.g., "mysql --version")
    #[serde(default)]
    pub version_cmd: Option<String>,
    /// Confidence level: "high" (systemd/service), "medium" (pidfile), "low" (pgrep)
    pub confidence: String,
    /// Source of the suggestion: "systemd", "windows-service", "docker", "process"
    pub source: String,
}

/// A scheduled job (cron, systemd timer, Windows Task Scheduler).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredScheduledJob {
    pub name: String,
    /// Cron expression or human-readable schedule
    pub schedule: String,
    /// The command that runs
    pub command: String,
    /// Which user runs this job
    pub user: String,
    /// Source: "crontab", "cron.d", "systemd-timer", "task-scheduler"
    pub source: String,
    /// Whether the job is currently enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// A TCP listener discovered on the host.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredListener {
    pub port: u16,
    pub protocol: String,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub address: String,
}

/// An outbound TCP connection observed on the host.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredConnection {
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub state: String,
}

/// A system service (systemd unit / Windows service) discovered on the host.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredService {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub pid: Option<u32>,
}

/// A firewall rule discovered on the host (Windows netsh / Linux iptables/firewalld).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoveredFirewallRule {
    /// Rule name (Windows) or chain/rule number (Linux)
    pub name: String,
    /// "allow" or "block"
    pub action: String,
    /// "in" or "out"
    pub direction: String,
    /// TCP, UDP, or "any"
    pub protocol: String,
    /// Local port(s) this rule applies to
    pub local_port: Option<u16>,
    /// Remote port(s) this rule applies to
    #[serde(default)]
    pub remote_port: Option<u16>,
    /// Whether the rule is currently enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Air-gap agent update types
// ---------------------------------------------------------------------------

/// Status of an in-progress agent binary update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    Downloading,
    Verifying,
    Applying,
    Complete,
    Failed,
}

#[cfg(test)]
mod native_command_tests {
    use super::*;

    #[test]
    fn templated_http_substitutes_hostname_and_install_path() {
        let cmd = NativeCommand::Http {
            method: "GET".to_string(),
            url: "https://{hostname}:8443/health".to_string(),
            headers: [("X-Path".to_string(), "{install_path}/v1".to_string())]
                .into_iter()
                .collect(),
            bearer_token: Some("secret".to_string()),
            body: Some("ping {hostname}".to_string()),
            expect_status: Some(200),
            expect_body_contains: Some("from {hostname}".to_string()),
            timeout_seconds: 3,
            insecure: false,
        };

        let out = cmd.templated_for_member("srv-042.prod", Some("/opt/jboss"));
        match out {
            NativeCommand::Http {
                url,
                headers,
                body,
                expect_body_contains,
                bearer_token,
                ..
            } => {
                assert_eq!(url, "https://srv-042.prod:8443/health");
                assert_eq!(
                    headers.get("X-Path").map(String::as_str),
                    Some("/opt/jboss/v1")
                );
                assert_eq!(body.as_deref(), Some("ping srv-042.prod"));
                assert_eq!(expect_body_contains.as_deref(), Some("from srv-042.prod"));
                // Bearer token is opaque, not templated.
                assert_eq!(bearer_token.as_deref(), Some("secret"));
            }
            _ => panic!("expected Http"),
        }
    }

    #[test]
    fn templated_tcp_substitutes_host_only() {
        let cmd = NativeCommand::TcpConnect {
            host: "{hostname}.internal".to_string(),
            port: 5432,
            timeout_seconds: 2,
        };
        let out = cmd.templated_for_member("db-01", None);
        match out {
            NativeCommand::TcpConnect { host, port, .. } => {
                assert_eq!(host, "db-01.internal");
                assert_eq!(port, 5432);
            }
            _ => panic!("expected TcpConnect"),
        }
    }

    #[test]
    fn redacted_masks_bearer_and_authorization_headers() {
        let cmd = NativeCommand::Http {
            method: "GET".to_string(),
            url: "https://api/health".to_string(),
            headers: [
                ("Authorization".to_string(), "Bearer xyz".to_string()),
                ("X-API-Key".to_string(), "k".to_string()),
                ("X-Trace".to_string(), "abc".to_string()),
            ]
            .into_iter()
            .collect(),
            bearer_token: Some("real-token".to_string()),
            body: None,
            expect_status: None,
            expect_body_contains: None,
            timeout_seconds: 3,
            insecure: false,
        };

        let r = cmd.redacted();
        match r {
            NativeCommand::Http {
                bearer_token,
                headers,
                ..
            } => {
                assert_eq!(bearer_token.as_deref(), Some("***"));
                assert_eq!(
                    headers.get("Authorization").map(String::as_str),
                    Some("***")
                );
                assert_eq!(headers.get("X-API-Key").map(String::as_str), Some("***"));
                // Non-sensitive headers untouched.
                assert_eq!(headers.get("X-Trace").map(String::as_str), Some("abc"));
            }
            _ => panic!("expected Http"),
        }
    }
}
