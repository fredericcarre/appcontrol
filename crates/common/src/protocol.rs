use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{CheckResult, ComponentConfig, ComponentState};

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
    },
    Register {
        agent_id: Uuid,
        hostname: String,
        labels: serde_json::Value,
        version: String,
    },
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
    },
    UpdateConfig {
        components: Vec<ComponentConfig>,
    },
    Ack {
        request_id: Uuid,
    },
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
    },
    /// An agent disconnected from this gateway.
    AgentDisconnected {
        agent_id: Uuid,
        hostname: String,
    },
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
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::CommandResult {
                exit_code,
                stdout,
                stderr,
                ..
            } => {
                assert_eq!(exit_code, 1);
                assert_eq!(stdout, "output");
                assert_eq!(stderr, "error");
            }
            _ => panic!("Expected CommandResult"),
        }
    }

    #[test]
    fn test_agent_message_register_roundtrip() {
        let msg = AgentMessage::Register {
            agent_id: Uuid::new_v4(),
            hostname: "server01".to_string(),
            labels: serde_json::json!({"role": "database", "env": "prod"}),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            AgentMessage::Register {
                hostname, version, ..
            } => {
                assert_eq!(hostname, "server01");
                assert_eq!(version, "0.1.0");
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
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::ExecuteCommand {
                command,
                timeout_seconds,
                ..
            } => {
                assert_eq!(command, "systemctl start nginx");
                assert_eq!(timeout_seconds, 60);
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
        let msg = BackendMessage::Ack { request_id: rid };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: BackendMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendMessage::Ack { request_id } => {
                assert_eq!(request_id, rid);
            }
            _ => panic!("Expected Ack"),
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
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: super::GatewayMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            super::GatewayMessage::AgentConnected {
                agent_id,
                hostname,
            } => {
                assert_eq!(agent_id, aid);
                assert_eq!(hostname, "server01");
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
            super::GatewayMessage::AgentDisconnected {
                agent_id,
                hostname,
            } => {
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
}
