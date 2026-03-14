use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{CheckResult, ComponentConfig, ComponentState};

/// QoS priority levels for message ordering.
/// Higher priority messages are processed before lower priority ones.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum MessagePriority {
    /// Heartbeats, periodic status — can be delayed or dropped
    Low = 0,
    /// Configuration updates, check results — normal processing
    #[default]
    Normal = 1,
    /// Command results, state transitions — timely delivery
    High = 2,
    /// Emergency operations, switchover commands — immediate delivery
    Critical = 3,
}

/// Messages sent from Agent to Backend (via Gateway).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum AgentMessage {
    Heartbeat {
        agent_id: Uuid,
        cpu: f32,
        memory: f32,
        #[serde(default)]
        disk: Option<f32>,
        at: DateTime<Utc>,
    },
    CheckResult(CheckResult),
    CommandResult {
        request_id: Uuid,
        exit_code: i32,
        stdout: String,
        stderr: String,
        duration_ms: u32,
        /// Monotonic sequence ID for reliable delivery (ack/retransmit).
        #[serde(default)]
        sequence_id: Option<u64>,
    },
    Register {
        agent_id: Uuid,
        hostname: String,
        #[serde(default)]
        ip_addresses: Vec<String>,
        labels: serde_json::Value,
        version: String,
        /// Operating system name (e.g., "macOS", "Linux", "Windows")
        #[serde(default)]
        os_name: Option<String>,
        /// Operating system version (e.g., "14.0", "Ubuntu 22.04")
        #[serde(default)]
        os_version: Option<String>,
        /// CPU architecture (e.g., "x86_64", "aarch64")
        #[serde(default)]
        cpu_arch: Option<String>,
        /// Number of CPU cores
        #[serde(default)]
        cpu_cores: Option<u32>,
        /// Total system memory in MB
        #[serde(default)]
        total_memory_mb: Option<u64>,
        /// Total disk space in GB (primary partition)
        #[serde(default)]
        disk_total_gb: Option<u64>,
        /// SHA-256 fingerprint of the agent's TLS client certificate.
        /// Populated by the gateway after extracting from the TLS handshake.
        #[serde(default)]
        cert_fingerprint: Option<String>,
    },
    /// Streaming output chunk from a running command.
    /// Sent periodically during sync execution for real-time output.
    CommandOutputChunk {
        request_id: Uuid,
        stdout: String,
        stderr: String,
    },
    /// Agent requests certificate renewal by sending a CSR.
    CertificateRenewal {
        agent_id: Uuid,
        csr_pem: String,
    },
    /// Passive discovery report: processes, listeners, connections found on this host.
    DiscoveryReport {
        agent_id: Uuid,
        hostname: String,
        processes: Vec<crate::types::DiscoveredProcess>,
        listeners: Vec<crate::types::DiscoveredListener>,
        connections: Vec<crate::types::DiscoveredConnection>,
        services: Vec<crate::types::DiscoveredService>,
        /// Scheduled jobs (cron, systemd timers, Windows Task Scheduler)
        #[serde(default)]
        scheduled_jobs: Vec<crate::types::DiscoveredScheduledJob>,
        /// Firewall rules (Windows netsh / Linux iptables)
        #[serde(default)]
        firewall_rules: Vec<crate::types::DiscoveredFirewallRule>,
        scanned_at: DateTime<Utc>,
    },
    /// Progress of an air-gap binary update received via WebSocket chunks.
    UpdateProgress {
        update_id: Uuid,
        agent_id: Uuid,
        chunks_received: u32,
        status: crate::types::UpdateStatus,
        #[serde(default)]
        error: Option<String>,
    },
    /// Terminal output data from an interactive shell session.
    TerminalOutput {
        request_id: Uuid,
        /// Raw terminal output bytes (may contain ANSI escape sequences).
        data: Vec<u8>,
    },
    /// Terminal session ended.
    TerminalExit {
        request_id: Uuid,
        exit_code: i32,
    },
    /// Log entry batch from agent (real-time log streaming).
    LogEntries {
        agent_id: Uuid,
        entries: Vec<LogEntry>,
    },
}

/// A single log entry from an agent or gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level: "TRACE", "DEBUG", "INFO", "WARN", "ERROR"
    pub level: String,
    /// Target module (e.g., "appcontrol_agent::scheduler")
    pub target: String,
    /// Log message
    pub message: String,
    /// Timestamp when the log was generated
    pub timestamp: DateTime<Utc>,
    /// Optional structured fields from the span/event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Value>,
}

impl AgentMessage {
    /// Get the QoS priority for this message type.
    pub fn priority(&self) -> MessagePriority {
        match self {
            AgentMessage::Heartbeat { .. } => MessagePriority::Low,
            AgentMessage::CheckResult(_) => MessagePriority::Normal,
            AgentMessage::CommandResult { .. } => MessagePriority::High,
            AgentMessage::CommandOutputChunk { .. } => MessagePriority::Normal,
            AgentMessage::Register { .. } => MessagePriority::Critical,
            AgentMessage::CertificateRenewal { .. } => MessagePriority::High,
            AgentMessage::DiscoveryReport { .. } => MessagePriority::Normal,
            AgentMessage::UpdateProgress { .. } => MessagePriority::High,
            AgentMessage::TerminalOutput { .. } => MessagePriority::High,
            AgentMessage::TerminalExit { .. } => MessagePriority::High,
            AgentMessage::LogEntries { .. } => MessagePriority::Low,
        }
    }
}

/// Messages sent from Backend to Agent (via Gateway).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum BackendMessage {
    ExecuteCommand {
        request_id: Uuid,
        component_id: Uuid,
        command: String,
        timeout_seconds: u32,
        /// Execution mode: "sync" (wait for result) or "detached" (double-fork).
        #[serde(default = "default_exec_mode")]
        exec_mode: String,
    },
    UpdateConfig {
        components: Vec<ComponentConfig>,
    },
    Ack {
        request_id: Uuid,
        /// Echo back the sequence_id for correlation.
        #[serde(default)]
        sequence_id: Option<u64>,
    },
    /// Command the agent to update its binary.
    UpdateAgent {
        binary_url: String,
        checksum_sha256: String,
        target_version: String,
    },
    /// Deliver a signed certificate in response to a CertificateRenewal CSR.
    CertificateResponse {
        cert_pem: String,
        ca_chain_pem: String,
        expires_at: DateTime<Utc>,
    },
    /// Request the agent to report its current approval-pending operations.
    ApprovalResult {
        request_id: Uuid,
        approved: bool,
    },
    /// Air-gap binary update: send a chunk of the new agent binary via WebSocket.
    UpdateBinaryChunk {
        update_id: Uuid,
        target_version: String,
        checksum_sha256: String,
        chunk_index: u32,
        total_chunks: u32,
        total_size: u64,
        /// Base64-encoded binary data (~256KB per chunk).
        data: String,
    },
    /// Request the agent to run a passive discovery scan and report back.
    RequestDiscovery {
        request_id: Uuid,
    },
    /// Disconnect an agent — sent from backend to gateway when cert
    /// pinning fails or certificate has been revoked.
    DisconnectAgent {
        agent_id: Uuid,
        reason: String,
    },
    /// Request agent/gateway to rotate their certificate to a new CA.
    /// During rotation, the entity should:
    /// 1. Trust both old and new CA certificates
    /// 2. Request a new certificate signed by the new CA
    /// 3. Reconnect with the new certificate
    CertificateRotation {
        /// New CA certificate to trust (PEM encoded)
        new_ca_cert: String,
        /// Grace period in seconds before old CA becomes invalid
        grace_period_secs: u64,
        /// Rotation ID for tracking progress
        rotation_id: Uuid,
    },
    /// Start an interactive terminal session (PTY).
    /// Unix only - Windows agents will return an error.
    StartTerminal {
        request_id: Uuid,
        /// Shell to use (default: $SHELL or /bin/bash)
        #[serde(default)]
        shell: Option<String>,
        /// Terminal width in columns
        cols: u16,
        /// Terminal height in rows
        rows: u16,
        /// Additional environment variables
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
    },
    /// Send user input to an active terminal session.
    TerminalInput {
        request_id: Uuid,
        /// Raw input bytes from the user
        data: Vec<u8>,
    },
    /// Resize the terminal window.
    TerminalResize {
        request_id: Uuid,
        cols: u16,
        rows: u16,
    },
    /// Close the terminal session.
    TerminalClose {
        request_id: Uuid,
    },
    /// Request agent to run all health checks immediately.
    /// Used when agent reconnects after being UNREACHABLE to quickly
    /// re-establish correct component states without waiting for intervals.
    RunChecksNow {
        request_id: Uuid,
    },
}

impl BackendMessage {
    /// Get the QoS priority for this message type.
    pub fn priority(&self) -> MessagePriority {
        match self {
            BackendMessage::ExecuteCommand { .. } => MessagePriority::High,
            BackendMessage::UpdateConfig { .. } => MessagePriority::Normal,
            BackendMessage::Ack { .. } => MessagePriority::Low,
            BackendMessage::UpdateAgent { .. } => MessagePriority::Normal,
            BackendMessage::CertificateResponse { .. } => MessagePriority::High,
            BackendMessage::ApprovalResult { .. } => MessagePriority::Critical,
            BackendMessage::UpdateBinaryChunk { .. } => MessagePriority::Normal,
            BackendMessage::RequestDiscovery { .. } => MessagePriority::Normal,
            BackendMessage::DisconnectAgent { .. } => MessagePriority::Critical,
            BackendMessage::CertificateRotation { .. } => MessagePriority::Critical,
            BackendMessage::StartTerminal { .. } => MessagePriority::High,
            BackendMessage::TerminalInput { .. } => MessagePriority::High,
            BackendMessage::TerminalResize { .. } => MessagePriority::Normal,
            BackendMessage::TerminalClose { .. } => MessagePriority::High,
            BackendMessage::RunChecksNow { .. } => MessagePriority::High,
        }
    }
}

fn default_exec_mode() -> String {
    "sync".to_string()
}

/// WebSocket events pushed to frontend clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsEvent {
    StateChange {
        component_id: Uuid,
        app_id: Uuid,
        #[serde(default)]
        component_name: Option<String>,
        #[serde(default)]
        app_name: Option<String>,
        from: ComponentState,
        to: ComponentState,
        at: DateTime<Utc>,
    },
    CheckResultEvent {
        component_id: Uuid,
        app_id: Uuid,
        #[serde(default)]
        component_name: Option<String>,
        #[serde(default)]
        app_name: Option<String>,
        check_type: String,
        exit_code: i32,
        at: DateTime<Utc>,
    },
    CommandResultEvent {
        request_id: Uuid,
        component_id: Uuid,
        #[serde(default)]
        component_name: Option<String>,
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
    /// Streaming output chunk from a running command (real-time).
    CommandOutputChunkEvent {
        request_id: Uuid,
        component_id: Uuid,
        stdout: String,
        stderr: String,
    },
    SwitchoverProgress {
        app_id: Uuid,
        phase: String,
        status: String,
        message: String,
    },
    AgentStatus {
        agent_id: Uuid,
        connected: bool,
    },
    PermissionChange {
        app_id: Uuid,
        user_id: Uuid,
        new_level: String,
    },
    /// Terminal session started successfully.
    TerminalStarted {
        session_id: Uuid,
        agent_id: Uuid,
    },
    /// Terminal output data from the agent.
    TerminalOutput {
        session_id: Uuid,
        /// Base64-encoded terminal output bytes.
        data: String,
    },
    /// Terminal session ended.
    TerminalExit {
        session_id: Uuid,
        exit_code: i32,
    },
    /// Terminal error (e.g., session not found, permission denied).
    TerminalError {
        session_id: Uuid,
        error: String,
    },
    /// Real-time log entry from an agent or gateway.
    LogEntry {
        /// Source type: "agent" or "gateway"
        source_type: String,
        /// Source ID (agent_id or gateway_id)
        source_id: Uuid,
        /// Human-readable source name (hostname for agents, gateway name for gateways)
        source_name: String,
        /// Log level: "TRACE", "DEBUG", "INFO", "WARN", "ERROR"
        level: String,
        /// Target module
        target: String,
        /// Log message
        message: String,
        /// ISO 8601 timestamp
        timestamp: String,
    },
    /// Auto-failover event: DR profile was automatically activated
    AutoFailover {
        app_id: Uuid,
        switchover_id: Uuid,
        from_profile: String,
        to_profile: String,
        unreachable_agents: Vec<String>,
        timestamp: DateTime<Utc>,
    },
}

/// Envelope for Backend → Gateway communication.
/// Each command is wrapped with routing information so the gateway
/// can deliver it to the correct agent (no broadcast).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum GatewayEnvelope {
    /// Route a backend message to a specific agent.
    ForwardToAgent {
        target_agent_id: Uuid,
        message: BackendMessage,
    },
    /// Order the gateway to disconnect and close its connection.
    /// Used when the gateway is blocked or suspended.
    DisconnectGateway { reason: String },
    /// Block an agent permanently until unblocked.
    /// The gateway should add the agent to a blocklist and reject future connections.
    BlockAgent { agent_id: Uuid, reason: String },
    /// Unblock a previously blocked agent.
    /// The gateway should remove the agent from the blocklist.
    UnblockAgent { agent_id: Uuid },
    /// Clear the entire agent blocklist.
    /// Used when a gateway is activated/unblocked to allow all agents to reconnect.
    ClearBlocklist,
}

/// Messages sent from Gateway to Backend.
/// The gateway wraps agent messages and adds lifecycle notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum GatewayMessage {
    /// Gateway self-registration when connecting to backend.
    Register {
        gateway_id: Uuid,
        /// Human-readable name for this gateway (e.g., "gateway-prd-01").
        #[serde(default)]
        name: Option<String>,
        zone: String,
        version: String,
        /// Enrollment token for authentication and organization binding.
        /// Required in production; optional in dev mode for backward compatibility.
        #[serde(default)]
        enrollment_token: Option<String>,
    },
    /// Forward an agent message (agent_id is inside the AgentMessage).
    AgentMessage(AgentMessage),
    /// An agent connected to this gateway.
    AgentConnected {
        agent_id: Uuid,
        hostname: String,
        /// Agent software version (e.g., "1.1.5").
        #[serde(default)]
        version: Option<String>,
        /// TLS certificate fingerprint extracted by gateway during handshake.
        #[serde(default)]
        cert_fingerprint: Option<String>,
        /// TLS certificate CN extracted by gateway during handshake.
        #[serde(default)]
        cert_cn: Option<String>,
    },
    /// An agent disconnected from this gateway.
    AgentDisconnected { agent_id: Uuid, hostname: String },
    /// Log entry batch from the gateway itself (real-time log streaming).
    LogEntries {
        gateway_id: Uuid,
        entries: Vec<LogEntry>,
    },
    /// Periodic heartbeat from gateway to backend (connection health + stats).
    Heartbeat {
        gateway_id: Uuid,
        /// Number of agents currently connected to this gateway.
        connected_agents: usize,
        /// Number of messages buffered while backend was disconnected.
        buffer_messages: usize,
        /// Total bytes buffered while backend was disconnected.
        buffer_bytes: usize,
    },
}

/// Client subscription message (frontend → backend WebSocket).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsClientMessage {
    Subscribe {
        app_id: Uuid,
    },
    Unsubscribe {
        app_id: Uuid,
    },
    /// Start a new terminal session on an agent (admin only).
    TerminalStart {
        agent_id: Uuid,
        #[serde(default)]
        shell: Option<String>,
        cols: u16,
        rows: u16,
    },
    /// Send input data to an active terminal session.
    TerminalInput {
        session_id: Uuid,
        /// Base64-encoded input bytes.
        data: String,
    },
    /// Resize the terminal window.
    TerminalResize {
        session_id: Uuid,
        cols: u16,
        rows: u16,
    },
    /// Close a terminal session.
    TerminalClose {
        session_id: Uuid,
    },
    /// Subscribe to real-time logs from an agent or gateway.
    LogSubscribe {
        /// Agent ID to subscribe to (mutually exclusive with gateway_id)
        #[serde(default)]
        agent_id: Option<Uuid>,
        /// Gateway ID to subscribe to (mutually exclusive with agent_id)
        #[serde(default)]
        gateway_id: Option<Uuid>,
        /// Minimum log level filter: "TRACE", "DEBUG", "INFO", "WARN", "ERROR"
        min_level: String,
    },
    /// Unsubscribe from logs.
    LogUnsubscribe {
        #[serde(default)]
        agent_id: Option<Uuid>,
        #[serde(default)]
        gateway_id: Option<Uuid>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CheckType;

    #[test]
    fn test_agent_message_heartbeat_roundtrip() {
        let msg = AgentMessage::Heartbeat {
            agent_id: Uuid::new_v4(),
            cpu: 45.2,
            memory: 72.1,
            disk: Some(55.5),
            at: Utc::now(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::Heartbeat {
                cpu, memory, disk, ..
            } => {
                assert!((cpu - 45.2).abs() < f32::EPSILON);
                assert!((memory - 72.1).abs() < f32::EPSILON);
                assert_eq!(disk, Some(55.5));
            }
            _ => panic!("Expected Heartbeat"),
        }
    }

    #[test]
    fn test_agent_message_check_result_roundtrip() {
        let msg = AgentMessage::CheckResult(CheckResult {
            component_id: Uuid::new_v4(),
            check_type: CheckType::Health,
            exit_code: 0,
            stdout: Some("OK".to_string()),
            duration_ms: 42,
            at: Utc::now(),
            metrics: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::CheckResult(cr) => {
                assert_eq!(cr.exit_code, 0);
                assert_eq!(cr.stdout, Some("OK".to_string()));
            }
            _ => panic!("Expected CheckResult"),
        }
    }

    #[test]
    fn test_agent_message_command_result_roundtrip() {
        let msg = AgentMessage::CommandResult {
            request_id: Uuid::new_v4(),
            exit_code: 1,
            stdout: "output".to_string(),
            stderr: "error".to_string(),
            duration_ms: 100,
            sequence_id: Some(42),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::CommandResult {
                exit_code,
                stdout,
                stderr,
                sequence_id,
                ..
            } => {
                assert_eq!(exit_code, 1);
                assert_eq!(stdout, "output");
                assert_eq!(stderr, "error");
                assert_eq!(sequence_id, Some(42));
            }
            _ => panic!("Expected CommandResult"),
        }
    }

    #[test]
    fn test_command_result_backward_compat_no_sequence_id() {
        let json = r#"{"type":"CommandResult","payload":{"request_id":"550e8400-e29b-41d4-a716-446655440000","exit_code":0,"stdout":"ok","stderr":"","duration_ms":10}}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::CommandResult { sequence_id, .. } => {
                assert_eq!(sequence_id, None);
            }
            _ => panic!("Expected CommandResult"),
        }
    }

    #[test]
    fn test_agent_message_register_roundtrip() {
        let msg = AgentMessage::Register {
            agent_id: Uuid::new_v4(),
            hostname: "server01.prod.company.com".to_string(),
            ip_addresses: vec!["10.0.1.42".to_string(), "172.16.0.5".to_string()],
            labels: serde_json::json!({"role": "database", "env": "prod"}),
            version: "0.1.0".to_string(),
            os_name: Some("Linux".to_string()),
            os_version: Some("Ubuntu 22.04".to_string()),
            cpu_arch: Some("x86_64".to_string()),
            cpu_cores: Some(8),
            total_memory_mb: Some(16384),
            disk_total_gb: Some(512),
            cert_fingerprint: Some("sha256:abc123".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Verify system info is in the JSON
        assert!(json.contains("os_name"));
        assert!(json.contains("Linux"));
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::Register {
                hostname,
                ip_addresses,
                version,
                os_name,
                cpu_cores,
                cert_fingerprint,
                ..
            } => {
                assert_eq!(hostname, "server01.prod.company.com");
                assert_eq!(ip_addresses, vec!["10.0.1.42", "172.16.0.5"]);
                assert_eq!(version, "0.1.0");
                assert_eq!(os_name, Some("Linux".to_string()));
                assert_eq!(cpu_cores, Some(8));
                assert_eq!(cert_fingerprint, Some("sha256:abc123".to_string()));
            }
            _ => panic!("Expected Register"),
        }
    }

    #[test]
    fn test_agent_message_register_backward_compat_no_ip() {
        // Old agents may not send ip_addresses — serde(default) handles this
        let json = r#"{"type":"Register","payload":{"agent_id":"550e8400-e29b-41d4-a716-446655440000","hostname":"server01","labels":{},"version":"0.1.0"}}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::Register { ip_addresses, .. } => {
                assert!(ip_addresses.is_empty());
            }
            _ => panic!("Expected Register"),
        }
    }

    #[test]
    fn test_backend_message_execute_command_roundtrip() {
        let msg = BackendMessage::ExecuteCommand {
            request_id: Uuid::new_v4(),
            component_id: Uuid::new_v4(),
            command: "systemctl start nginx".to_string(),
            timeout_seconds: 60,
            exec_mode: "detached".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::ExecuteCommand {
                command,
                timeout_seconds,
                exec_mode,
                ..
            } => {
                assert_eq!(command, "systemctl start nginx");
                assert_eq!(timeout_seconds, 60);
                assert_eq!(exec_mode, "detached");
            }
            _ => panic!("Expected ExecuteCommand"),
        }
    }

    #[test]
    fn test_execute_command_backward_compat_default_exec_mode() {
        let json = r#"{"type":"ExecuteCommand","payload":{"request_id":"550e8400-e29b-41d4-a716-446655440000","component_id":"550e8400-e29b-41d4-a716-446655440001","command":"echo hi","timeout_seconds":30}}"#;
        let msg: BackendMessage = serde_json::from_str(json).unwrap();
        match msg {
            BackendMessage::ExecuteCommand { exec_mode, .. } => {
                assert_eq!(exec_mode, "sync");
            }
            _ => panic!("Expected ExecuteCommand"),
        }
    }

    #[test]
    fn test_backend_message_update_config_roundtrip() {
        let msg = BackendMessage::UpdateConfig {
            components: vec![ComponentConfig {
                component_id: Uuid::new_v4(),
                name: "nginx".to_string(),
                check_cmd: Some("pgrep nginx".to_string()),
                start_cmd: Some("systemctl start nginx".to_string()),
                stop_cmd: Some("systemctl stop nginx".to_string()),
                integrity_check_cmd: None,
                post_start_check_cmd: None,
                infra_check_cmd: None,
                rebuild_cmd: None,
                rebuild_infra_cmd: None,
                check_interval_seconds: 30,
                start_timeout_seconds: 120,
                stop_timeout_seconds: 60,
                env_vars: serde_json::json!({}),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::UpdateConfig { components } => {
                assert_eq!(components.len(), 1);
                assert_eq!(components[0].name, "nginx");
            }
            _ => panic!("Expected UpdateConfig"),
        }
    }

    #[test]
    fn test_backend_message_ack_roundtrip() {
        let rid = Uuid::new_v4();
        let msg = BackendMessage::Ack {
            request_id: rid,
            sequence_id: Some(99),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::Ack {
                request_id,
                sequence_id,
            } => {
                assert_eq!(request_id, rid);
                assert_eq!(sequence_id, Some(99));
            }
            _ => panic!("Expected Ack"),
        }
    }

    #[test]
    fn test_update_agent_message_roundtrip() {
        let msg = BackendMessage::UpdateAgent {
            binary_url: "https://releases.appcontrol.io/agent/0.3.0/linux-amd64".to_string(),
            checksum_sha256: "abc123def456".to_string(),
            target_version: "0.3.0".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::UpdateAgent { target_version, .. } => {
                assert_eq!(target_version, "0.3.0");
            }
            _ => panic!("Expected UpdateAgent"),
        }
    }

    #[test]
    fn test_gateway_envelope_forward_roundtrip() {
        let agent_id = Uuid::new_v4();
        let inner = BackendMessage::ExecuteCommand {
            request_id: Uuid::new_v4(),
            component_id: Uuid::new_v4(),
            command: "systemctl start nginx".to_string(),
            timeout_seconds: 60,
            exec_mode: "sync".to_string(),
        };
        let envelope = super::GatewayEnvelope::ForwardToAgent {
            target_agent_id: agent_id,
            message: inner,
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let deserialized: super::GatewayEnvelope = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayEnvelope::ForwardToAgent {
                target_agent_id,
                message,
            } => {
                assert_eq!(target_agent_id, agent_id);
                match message {
                    BackendMessage::ExecuteCommand { command, .. } => {
                        assert_eq!(command, "systemctl start nginx");
                    }
                    _ => panic!("Expected ExecuteCommand"),
                }
            }
            _ => panic!("Expected ForwardToAgent"),
        }
    }

    #[test]
    fn test_gateway_message_register_roundtrip() {
        let gw_id = Uuid::new_v4();
        let msg = super::GatewayMessage::Register {
            gateway_id: gw_id,
            name: Some("gateway-prd-01".to_string()),
            zone: "PRD".to_string(),
            version: "0.1.0".to_string(),
            enrollment_token: Some("ac_enroll_test123".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::Register {
                gateway_id,
                name,
                zone,
                ..
            } => {
                assert_eq!(gateway_id, gw_id);
                assert_eq!(name, Some("gateway-prd-01".to_string()));
                assert_eq!(zone, "PRD");
            }
            _ => panic!("Expected Register"),
        }
    }

    #[test]
    fn test_gateway_message_agent_connected_roundtrip() {
        let aid = Uuid::new_v4();
        let msg = super::GatewayMessage::AgentConnected {
            agent_id: aid,
            hostname: "server01".to_string(),
            version: Some("1.2.3".to_string()),
            cert_fingerprint: Some("sha256:deadbeef".to_string()),
            cert_cn: Some("agent-server01".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::AgentConnected {
                agent_id,
                hostname,
                version,
                cert_fingerprint,
                cert_cn,
            } => {
                assert_eq!(agent_id, aid);
                assert_eq!(hostname, "server01");
                assert_eq!(version, Some("1.2.3".to_string()));
                assert_eq!(cert_fingerprint, Some("sha256:deadbeef".to_string()));
                assert_eq!(cert_cn, Some("agent-server01".to_string()));
            }
            _ => panic!("Expected AgentConnected"),
        }
    }

    #[test]
    fn test_gateway_message_agent_connected_backward_compat() {
        // Old gateways may not send cert_fingerprint/cert_cn/version
        let json = r#"{"type":"AgentConnected","payload":{"agent_id":"550e8400-e29b-41d4-a716-446655440000","hostname":"server01"}}"#;
        let msg: super::GatewayMessage = serde_json::from_str(json).unwrap();
        match msg {
            super::GatewayMessage::AgentConnected {
                version,
                cert_fingerprint,
                cert_cn,
                ..
            } => {
                assert_eq!(version, None);
                assert_eq!(cert_fingerprint, None);
                assert_eq!(cert_cn, None);
            }
            _ => panic!("Expected AgentConnected"),
        }
    }

    #[test]
    fn test_gateway_message_agent_disconnected_roundtrip() {
        let aid = Uuid::new_v4();
        let msg = super::GatewayMessage::AgentDisconnected {
            agent_id: aid,
            hostname: "server01".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::AgentDisconnected { agent_id, hostname } => {
                assert_eq!(agent_id, aid);
                assert_eq!(hostname, "server01");
            }
            _ => panic!("Expected AgentDisconnected"),
        }
    }

    #[test]
    fn test_gateway_message_wraps_agent_message() {
        let inner = AgentMessage::Heartbeat {
            agent_id: Uuid::new_v4(),
            cpu: 50.0,
            memory: 60.0,
            disk: Some(70.0),
            at: Utc::now(),
        };
        let msg = super::GatewayMessage::AgentMessage(inner);
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::AgentMessage(agent_msg) => match agent_msg {
                AgentMessage::Heartbeat {
                    cpu, memory, disk, ..
                } => {
                    assert!((cpu - 50.0).abs() < f32::EPSILON);
                    assert!((memory - 60.0).abs() < f32::EPSILON);
                    assert_eq!(disk, Some(70.0));
                }
                _ => panic!("Expected Heartbeat"),
            },
            _ => panic!("Expected AgentMessage"),
        }
    }

    #[test]
    fn test_agent_message_priority_ordering() {
        let heartbeat = AgentMessage::Heartbeat {
            agent_id: Uuid::new_v4(),
            cpu: 0.0,
            memory: 0.0,
            disk: None,
            at: Utc::now(),
        };
        let register = AgentMessage::Register {
            agent_id: Uuid::new_v4(),
            hostname: "test".to_string(),
            ip_addresses: vec![],
            labels: serde_json::json!({}),
            version: "0.1.0".to_string(),
            os_name: None,
            os_version: None,
            cpu_arch: None,
            cpu_cores: None,
            total_memory_mb: None,
            disk_total_gb: None,
            cert_fingerprint: None,
        };
        let cmd_result = AgentMessage::CommandResult {
            request_id: Uuid::new_v4(),
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
            sequence_id: None,
        };

        assert!(heartbeat.priority() < cmd_result.priority());
        assert!(cmd_result.priority() < register.priority());
    }

    #[test]
    fn test_backend_message_priority() {
        let ack = BackendMessage::Ack {
            request_id: Uuid::new_v4(),
            sequence_id: None,
        };
        let exec = BackendMessage::ExecuteCommand {
            request_id: Uuid::new_v4(),
            component_id: Uuid::new_v4(),
            command: "test".to_string(),
            timeout_seconds: 30,
            exec_mode: "sync".to_string(),
        };
        assert!(ack.priority() < exec.priority());
    }

    #[test]
    fn test_certificate_rotation_message_roundtrip() {
        let rotation_id = Uuid::new_v4();
        let msg = BackendMessage::CertificateRotation {
            new_ca_cert: "-----BEGIN CERTIFICATE-----\nMIIC...".to_string(),
            grace_period_secs: 3600,
            rotation_id,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::CertificateRotation {
                new_ca_cert,
                grace_period_secs,
                rotation_id: rid,
            } => {
                assert!(new_ca_cert.contains("BEGIN CERTIFICATE"));
                assert_eq!(grace_period_secs, 3600);
                assert_eq!(rid, rotation_id);
            }
            _ => panic!("Expected CertificateRotation"),
        }
    }

    #[test]
    fn test_certificate_rotation_priority_is_critical() {
        let msg = BackendMessage::CertificateRotation {
            new_ca_cert: "test".to_string(),
            grace_period_secs: 3600,
            rotation_id: Uuid::new_v4(),
        };
        assert_eq!(msg.priority(), MessagePriority::Critical);
    }
}
