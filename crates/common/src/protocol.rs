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
        from: ComponentState,
        to: ComponentState,
        at: DateTime<Utc>,
    },
    CheckResultEvent {
        component_id: Uuid,
        app_id: Uuid,
        check_type: String,
        exit_code: i32,
        at: DateTime<Utc>,
    },
    CommandResultEvent {
        request_id: Uuid,
        component_id: Uuid,
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
}

/// Messages sent from Gateway to Backend.
/// The gateway wraps agent messages and adds lifecycle notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum GatewayMessage {
    /// Gateway self-registration when connecting to backend.
    Register {
        gateway_id: Uuid,
        zone: String,
        version: String,
    },
    /// Forward an agent message (agent_id is inside the AgentMessage).
    AgentMessage(AgentMessage),
    /// An agent connected to this gateway.
    AgentConnected {
        agent_id: Uuid,
        hostname: String,
        /// TLS certificate fingerprint extracted by gateway during handshake.
        #[serde(default)]
        cert_fingerprint: Option<String>,
        /// TLS certificate CN extracted by gateway during handshake.
        #[serde(default)]
        cert_cn: Option<String>,
    },
    /// An agent disconnected from this gateway.
    AgentDisconnected { agent_id: Uuid, hostname: String },
}

/// Client subscription message (frontend → backend WebSocket).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum WsClientMessage {
    Subscribe { app_id: Uuid },
    Unsubscribe { app_id: Uuid },
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
            at: Utc::now(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::Heartbeat { cpu, memory, .. } => {
                assert!((cpu - 45.2).abs() < f32::EPSILON);
                assert!((memory - 72.1).abs() < f32::EPSILON);
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
            cert_fingerprint: Some("sha256:abc123".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::Register {
                hostname,
                ip_addresses,
                version,
                cert_fingerprint,
                ..
            } => {
                assert_eq!(hostname, "server01.prod.company.com");
                assert_eq!(ip_addresses, vec!["10.0.1.42", "172.16.0.5"]);
                assert_eq!(version, "0.1.0");
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
        }
    }

    #[test]
    fn test_gateway_message_register_roundtrip() {
        let gw_id = Uuid::new_v4();
        let msg = super::GatewayMessage::Register {
            gateway_id: gw_id,
            zone: "PRD".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::Register {
                gateway_id, zone, ..
            } => {
                assert_eq!(gateway_id, gw_id);
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
            cert_fingerprint: Some("sha256:deadbeef".to_string()),
            cert_cn: Some("agent-server01".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::AgentConnected {
                agent_id,
                hostname,
                cert_fingerprint,
                cert_cn,
            } => {
                assert_eq!(agent_id, aid);
                assert_eq!(hostname, "server01");
                assert_eq!(cert_fingerprint, Some("sha256:deadbeef".to_string()));
                assert_eq!(cert_cn, Some("agent-server01".to_string()));
            }
            _ => panic!("Expected AgentConnected"),
        }
    }

    #[test]
    fn test_gateway_message_agent_connected_backward_compat() {
        // Old gateways may not send cert_fingerprint/cert_cn
        let json = r#"{"type":"AgentConnected","payload":{"agent_id":"550e8400-e29b-41d4-a716-446655440000","hostname":"server01"}}"#;
        let msg: super::GatewayMessage = serde_json::from_str(json).unwrap();
        match msg {
            super::GatewayMessage::AgentConnected {
                cert_fingerprint,
                cert_cn,
                ..
            } => {
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
            at: Utc::now(),
        };
        let msg = super::GatewayMessage::AgentMessage(inner);
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::AgentMessage(agent_msg) => match agent_msg {
                AgentMessage::Heartbeat { cpu, memory, .. } => {
                    assert!((cpu - 50.0).abs() < f32::EPSILON);
                    assert!((memory - 60.0).abs() < f32::EPSILON);
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
            at: Utc::now(),
        };
        let register = AgentMessage::Register {
            agent_id: Uuid::new_v4(),
            hostname: "test".to_string(),
            ip_addresses: vec![],
            labels: serde_json::json!({}),
            version: "0.1.0".to_string(),
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
}
