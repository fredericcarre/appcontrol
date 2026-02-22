use dashmap::DashMap;
use std::collections::HashSet;
use tokio::sync::mpsc;
use uuid::Uuid;

use appcontrol_common::{BackendMessage, GatewayEnvelope, WsEvent};

/// A frontend WebSocket connection.
#[allow(dead_code)]
struct Connection {
    user_id: Uuid,
    sender: mpsc::UnboundedSender<String>,
    subscriptions: HashSet<Uuid>, // app_ids
}

/// A registered gateway connection.
struct GatewayConnection {
    sender: mpsc::UnboundedSender<String>,
    #[allow(dead_code)]
    zone: String,
}

/// WebSocket hub. Manages frontend connections, multiple gateway connections,
/// and routes commands to agents via the correct gateway.
pub struct Hub {
    /// Frontend client connections
    connections: DashMap<Uuid, Connection>,
    /// gateway_id → gateway connection (supports multiple gateways)
    gateways: DashMap<Uuid, GatewayConnection>,
    /// agent_id → gateway_id (routing table: which gateway hosts which agent)
    agent_to_gateway: DashMap<Uuid, Uuid>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            gateways: DashMap::new(),
            agent_to_gateway: DashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Frontend connection management
    // -----------------------------------------------------------------------

    pub fn add_connection(
        &self,
        conn_id: Uuid,
        user_id: Uuid,
        sender: mpsc::UnboundedSender<String>,
    ) {
        self.connections.insert(
            conn_id,
            Connection {
                user_id,
                sender,
                subscriptions: HashSet::new(),
            },
        );
    }

    pub fn remove_connection(&self, conn_id: Uuid) {
        self.connections.remove(&conn_id);
    }

    /// Subscribe to app events. Caller must check permissions first.
    pub fn subscribe(&self, conn_id: Uuid, app_id: Uuid) {
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.subscriptions.insert(app_id);
        }
    }

    /// Get the user_id associated with a connection (for permission checking).
    pub fn get_connection_user_id(&self, conn_id: Uuid) -> Option<Uuid> {
        self.connections.get(&conn_id).map(|c| c.user_id)
    }

    pub fn unsubscribe(&self, conn_id: Uuid, app_id: Uuid) {
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.subscriptions.remove(&app_id);
        }
    }

    /// Broadcast an event to all connections subscribed to the given app.
    pub fn broadcast(&self, app_id: Uuid, event: WsEvent) {
        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(_) => return,
        };

        for entry in self.connections.iter() {
            let conn = entry.value();
            if conn.subscriptions.contains(&app_id) {
                let _ = conn.sender.send(json.clone());
            }
        }
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    // -----------------------------------------------------------------------
    // Gateway management (multi-gateway)
    // -----------------------------------------------------------------------

    /// Register a gateway connection.
    pub fn register_gateway(
        &self,
        gateway_id: Uuid,
        zone: String,
        sender: mpsc::UnboundedSender<String>,
    ) {
        tracing::info!(
            gateway_id = %gateway_id,
            zone = %zone,
            "Gateway registered"
        );
        self.gateways.insert(
            gateway_id,
            GatewayConnection { sender, zone },
        );
    }

    /// Unregister a gateway and clean up all agent→gateway mappings.
    pub fn unregister_gateway(&self, gateway_id: Uuid) {
        self.gateways.remove(&gateway_id);

        // Remove all agent mappings that pointed to this gateway
        let orphaned: Vec<Uuid> = self
            .agent_to_gateway
            .iter()
            .filter(|entry| *entry.value() == gateway_id)
            .map(|entry| *entry.key())
            .collect();

        for agent_id in &orphaned {
            self.agent_to_gateway.remove(agent_id);
        }

        tracing::info!(
            gateway_id = %gateway_id,
            orphaned_agents = orphaned.len(),
            "Gateway unregistered"
        );
    }

    /// Record that an agent is reachable via a specific gateway.
    pub fn register_agent_route(&self, agent_id: Uuid, gateway_id: Uuid) {
        tracing::debug!(
            agent_id = %agent_id,
            gateway_id = %gateway_id,
            "Agent route registered"
        );
        self.agent_to_gateway.insert(agent_id, gateway_id);
    }

    /// Remove an agent's routing entry.
    pub fn unregister_agent_route(&self, agent_id: Uuid) {
        if self.agent_to_gateway.remove(&agent_id).is_some() {
            tracing::debug!(agent_id = %agent_id, "Agent route removed");
        }
    }

    /// Send a command to a specific agent via its gateway.
    /// The message is wrapped in a GatewayEnvelope with the target_agent_id.
    pub fn send_to_agent(&self, agent_id: Uuid, message: BackendMessage) -> bool {
        // Look up which gateway this agent is connected to
        let gateway_id = match self.agent_to_gateway.get(&agent_id) {
            Some(entry) => *entry.value(),
            None => {
                tracing::warn!(
                    agent_id = %agent_id,
                    "No gateway route for agent — cannot deliver command"
                );
                return false;
            }
        };

        // Wrap the message in a GatewayEnvelope for targeted routing
        let envelope = GatewayEnvelope::ForwardToAgent {
            target_agent_id: agent_id,
            message,
        };

        let json = match serde_json::to_string(&envelope) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize GatewayEnvelope: {}", e);
                return false;
            }
        };

        // Send to the gateway
        if let Some(gw) = self.gateways.get(&gateway_id) {
            if gw.sender.send(json).is_ok() {
                return true;
            }
            tracing::warn!(
                gateway_id = %gateway_id,
                "Gateway channel closed"
            );
        } else {
            tracing::warn!(
                gateway_id = %gateway_id,
                agent_id = %agent_id,
                "Gateway not connected"
            );
        }
        false
    }

    /// Check if any gateway is connected.
    pub fn has_gateway(&self) -> bool {
        !self.gateways.is_empty()
    }

    pub fn gateway_count(&self) -> usize {
        self.gateways.len()
    }

    pub fn routed_agent_count(&self) -> usize {
        self.agent_to_gateway.len()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_gateway_routing() {
        let hub = Hub::new();

        let gw1 = Uuid::new_v4();
        let gw2 = Uuid::new_v4();
        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();

        hub.register_gateway(gw1, "PRD".to_string(), tx1);
        hub.register_gateway(gw2, "DMZ".to_string(), tx2);

        hub.register_agent_route(agent_a, gw1);
        hub.register_agent_route(agent_b, gw2);

        // Send to agent_a → should go to gw1
        let msg = BackendMessage::Ack {
            request_id: Uuid::new_v4(),
            sequence_id: None,
        };
        assert!(hub.send_to_agent(agent_a, msg));
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_err());

        // Send to agent_b → should go to gw2
        let msg = BackendMessage::Ack {
            request_id: Uuid::new_v4(),
            sequence_id: None,
        };
        assert!(hub.send_to_agent(agent_b, msg));
        assert!(rx1.try_recv().is_err());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_send_to_unrouted_agent_fails() {
        let hub = Hub::new();
        let unknown = Uuid::new_v4();
        let msg = BackendMessage::Ack {
            request_id: Uuid::new_v4(),
            sequence_id: None,
        };
        assert!(!hub.send_to_agent(unknown, msg));
    }

    #[test]
    fn test_unregister_gateway_cleans_agent_routes() {
        let hub = Hub::new();
        let gw = Uuid::new_v4();
        let agent1 = Uuid::new_v4();
        let agent2 = Uuid::new_v4();

        let (tx, _rx) = mpsc::unbounded_channel();
        hub.register_gateway(gw, "PRD".to_string(), tx);
        hub.register_agent_route(agent1, gw);
        hub.register_agent_route(agent2, gw);

        assert_eq!(hub.routed_agent_count(), 2);

        hub.unregister_gateway(gw);

        assert_eq!(hub.gateway_count(), 0);
        assert_eq!(hub.routed_agent_count(), 0);
    }

    #[test]
    fn test_envelope_contains_target_agent_id() {
        let hub = Hub::new();
        let gw = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let (tx, mut rx) = mpsc::unbounded_channel();
        hub.register_gateway(gw, "PRD".to_string(), tx);
        hub.register_agent_route(agent_id, gw);

        let cmd = BackendMessage::ExecuteCommand {
            request_id: Uuid::new_v4(),
            component_id: Uuid::new_v4(),
            command: "start".to_string(),
            timeout_seconds: 30,
            exec_mode: "sync".to_string(),
        };
        hub.send_to_agent(agent_id, cmd);

        let json = rx.try_recv().unwrap();
        let envelope: GatewayEnvelope = serde_json::from_str(&json).unwrap();
        match envelope {
            GatewayEnvelope::ForwardToAgent {
                target_agent_id,
                message,
            } => {
                assert_eq!(target_agent_id, agent_id);
                assert!(matches!(message, BackendMessage::ExecuteCommand { .. }));
            }
        }
    }
}
