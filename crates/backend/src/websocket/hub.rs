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
///
/// Performance features:
/// - `app_subscriptions`: reverse index from app_id → set of conn_ids.
///   Broadcast is O(subscribers) instead of O(all_connections).
/// - `request_to_agent`: tracks request_id → agent_id for Ack routing
///   when CommandResult comes back (it doesn't contain agent_id).
pub struct Hub {
    /// Frontend client connections
    connections: DashMap<Uuid, Connection>,
    /// app_id → set of conn_ids subscribed to that app (reverse index for broadcast)
    app_subscriptions: DashMap<Uuid, HashSet<Uuid>>,
    /// gateway_id → gateway connection (supports multiple gateways)
    gateways: DashMap<Uuid, GatewayConnection>,
    /// agent_id → gateway_id (routing table: which gateway hosts which agent)
    agent_to_gateway: DashMap<Uuid, Uuid>,
    /// request_id → agent_id (for routing Acks back to the correct agent)
    request_to_agent: DashMap<Uuid, Uuid>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            app_subscriptions: DashMap::new(),
            gateways: DashMap::new(),
            agent_to_gateway: DashMap::new(),
            request_to_agent: DashMap::new(),
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
        // Clean up app_subscriptions reverse index
        if let Some((_, conn)) = self.connections.remove(&conn_id) {
            for app_id in &conn.subscriptions {
                if let Some(mut subs) = self.app_subscriptions.get_mut(app_id) {
                    subs.remove(&conn_id);
                }
            }
        }
    }

    /// Subscribe to app events. Caller must check permissions first.
    pub fn subscribe(&self, conn_id: Uuid, app_id: Uuid) {
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.subscriptions.insert(app_id);
        }
        // Maintain reverse index
        self.app_subscriptions
            .entry(app_id)
            .or_default()
            .insert(conn_id);
    }

    /// Get the user_id associated with a connection (for permission checking).
    pub fn get_connection_user_id(&self, conn_id: Uuid) -> Option<Uuid> {
        self.connections.get(&conn_id).map(|c| c.user_id)
    }

    pub fn unsubscribe(&self, conn_id: Uuid, app_id: Uuid) {
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.subscriptions.remove(&app_id);
        }
        // Clean up reverse index
        if let Some(mut subs) = self.app_subscriptions.get_mut(&app_id) {
            subs.remove(&conn_id);
        }
    }

    /// Broadcast an event to all connections subscribed to the given app.
    /// Uses the app_subscriptions reverse index for O(subscribers) performance
    /// instead of scanning all connections.
    pub fn broadcast(&self, app_id: impl Into<Uuid>, event: WsEvent) {
        let app_id: Uuid = app_id.into();
        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(_) => return,
        };

        // Use reverse index: only iterate subscribers for this app
        if let Some(subscriber_ids) = self.app_subscriptions.get(&app_id) {
            for conn_id in subscriber_ids.iter() {
                if let Some(conn) = self.connections.get(conn_id) {
                    let _ = conn.sender.send(json.clone());
                }
            }
        }
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Send a message directly to a specific frontend connection.
    /// Used for terminal output which goes to one specific client.
    pub fn send_to_connection(&self, conn_id: Uuid, message: String) -> bool {
        if let Some(conn) = self.connections.get(&conn_id) {
            conn.sender.send(message).is_ok()
        } else {
            false
        }
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
        self.gateways
            .insert(gateway_id, GatewayConnection { sender, zone });
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

    /// Check if an agent is currently connected (has an active gateway route).
    pub fn is_agent_connected(&self, agent_id: Uuid) -> bool {
        self.agent_to_gateway.contains_key(&agent_id)
    }

    /// Get the set of all currently connected agent IDs.
    pub fn connected_agent_ids(&self) -> Vec<Uuid> {
        self.agent_to_gateway.iter().map(|e| *e.key()).collect()
    }

    /// Send a command to a specific agent via its gateway.
    /// The message is wrapped in a GatewayEnvelope with the target_agent_id.
    /// If the message is an ExecuteCommand, the request_id → agent_id mapping
    /// is recorded for Ack routing when CommandResult comes back.
    pub fn send_to_agent(&self, agent_id: impl Into<Uuid>, message: BackendMessage) -> bool {
        let agent_id: Uuid = agent_id.into();
        // Track request_id → agent_id for Ack routing
        if let BackendMessage::ExecuteCommand { request_id, .. } = &message {
            self.request_to_agent.insert(*request_id, agent_id);
        }

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

    /// Look up and consume the agent_id associated with a request_id.
    /// Used to route Acks back to the correct agent when CommandResult arrives.
    pub fn resolve_request_agent(&self, request_id: Uuid) -> Option<Uuid> {
        self.request_to_agent
            .remove(&request_id)
            .map(|(_, agent_id)| agent_id)
    }

    /// Check if any gateway is connected.
    pub fn has_gateway(&self) -> bool {
        !self.gateways.is_empty()
    }

    /// Check if a specific gateway is connected.
    pub fn is_gateway_connected(&self, gateway_id: Uuid) -> bool {
        self.gateways.contains_key(&gateway_id)
    }

    /// Get the set of connected gateway IDs.
    pub fn connected_gateway_ids(&self) -> Vec<Uuid> {
        self.gateways.iter().map(|entry| *entry.key()).collect()
    }

    pub fn gateway_count(&self) -> usize {
        self.gateways.len()
    }

    pub fn routed_agent_count(&self) -> usize {
        self.agent_to_gateway.len()
    }

    /// Send a message to a specific gateway.
    /// Returns true if the message was sent, false if the gateway is not connected.
    pub fn send_to_gateway(&self, gateway_id: Uuid, message: &str) -> bool {
        if let Some(gw) = self.gateways.get(&gateway_id) {
            let _ = gw.sender.send(message.to_string());
            true
        } else {
            tracing::warn!(
                gateway_id = %gateway_id,
                "Cannot send message to gateway: not connected"
            );
            false
        }
    }

    // -----------------------------------------------------------------------
    // Security disconnect methods
    // -----------------------------------------------------------------------

    /// Forcibly disconnect an agent for security reasons (e.g., blocked).
    /// Sends a DisconnectAgent message to the gateway and removes the routing.
    pub fn disconnect_agent(&self, agent_id: Uuid) {
        // Look up which gateway this agent is connected to
        let gateway_id = match self.agent_to_gateway.get(&agent_id) {
            Some(entry) => *entry.value(),
            None => {
                tracing::debug!(
                    agent_id = %agent_id,
                    "Agent not connected — nothing to disconnect"
                );
                return;
            }
        };

        // Send DisconnectAgent command to the gateway
        let disconnect_msg = BackendMessage::DisconnectAgent {
            agent_id,
            reason: "Agent blocked by administrator".to_string(),
        };

        let envelope = GatewayEnvelope::ForwardToAgent {
            target_agent_id: agent_id,
            message: disconnect_msg,
        };

        if let Ok(json) = serde_json::to_string(&envelope) {
            if let Some(gw) = self.gateways.get(&gateway_id) {
                let _ = gw.sender.send(json);
            }
        }

        // Remove the routing entry
        self.agent_to_gateway.remove(&agent_id);
        tracing::info!(agent_id = %agent_id, "Agent forcibly disconnected");
    }

    /// Block an agent permanently. Adds the agent to the gateway's blocklist
    /// so it will be rejected even if it tries to reconnect.
    /// This is more persistent than disconnect_agent which only drops the current connection.
    pub fn block_agent(&self, agent_id: Uuid, reason: &str) {
        // Send BlockAgent to ALL gateways so the agent is blocked everywhere
        let block_msg = GatewayEnvelope::BlockAgent {
            agent_id,
            reason: reason.to_string(),
        };

        if let Ok(json) = serde_json::to_string(&block_msg) {
            for gateway in self.gateways.iter() {
                let _ = gateway.sender.send(json.clone());
            }
        }

        // Also remove the routing entry if the agent is currently connected
        self.agent_to_gateway.remove(&agent_id);

        tracing::info!(
            agent_id = %agent_id,
            reason = %reason,
            "Agent blocked on all gateways"
        );
    }

    /// Unblock a previously blocked agent. Removes the agent from the gateway's blocklist
    /// so it can reconnect.
    pub fn unblock_agent(&self, agent_id: Uuid) {
        // Send UnblockAgent to ALL gateways
        let unblock_msg = GatewayEnvelope::UnblockAgent { agent_id };

        if let Ok(json) = serde_json::to_string(&unblock_msg) {
            for gateway in self.gateways.iter() {
                let _ = gateway.sender.send(json.clone());
            }
        }

        tracing::info!(agent_id = %agent_id, "Agent unblocked on all gateways");
    }

    /// Forcibly disconnect a gateway for security reasons (e.g., blocked).
    /// Sends a DisconnectGateway message to the gateway and removes it.
    pub fn disconnect_gateway(&self, gateway_id: Uuid) {
        // Send DisconnectGateway message to the gateway before removing it
        if let Some(gw) = self.gateways.get(&gateway_id) {
            let disconnect_msg = GatewayEnvelope::DisconnectGateway {
                reason: "Gateway blocked by administrator".to_string(),
            };
            if let Ok(json) = serde_json::to_string(&disconnect_msg) {
                let _ = gw.sender.send(json);
            }
        }

        // Find all agents routed through this gateway
        let agents: Vec<Uuid> = self
            .agent_to_gateway
            .iter()
            .filter(|entry| *entry.value() == gateway_id)
            .map(|entry| *entry.key())
            .collect();

        // Remove agent routes
        for agent_id in &agents {
            self.agent_to_gateway.remove(agent_id);
        }

        // Remove the gateway
        self.gateways.remove(&gateway_id);

        tracing::info!(
            gateway_id = %gateway_id,
            disconnected_agents = agents.len(),
            "Gateway forcibly disconnected"
        );
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
            cluster_member_id: None,
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
            _ => {
                panic!("Expected ForwardToAgent");
            }
        }
    }

    #[test]
    fn test_request_to_agent_tracking() {
        let hub = Hub::new();
        let gw = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();

        let (tx, _rx) = mpsc::unbounded_channel();
        hub.register_gateway(gw, "PRD".to_string(), tx);
        hub.register_agent_route(agent_id, gw);

        // send_to_agent with ExecuteCommand should track request_id
        let cmd = BackendMessage::ExecuteCommand {
            request_id,
            component_id: Uuid::new_v4(),
            command: "start".to_string(),
            timeout_seconds: 30,
            exec_mode: "sync".to_string(),
            cluster_member_id: None,
        };
        hub.send_to_agent(agent_id, cmd);

        // Should be able to resolve request → agent
        assert_eq!(hub.resolve_request_agent(request_id), Some(agent_id));
        // Consumed — second call returns None
        assert_eq!(hub.resolve_request_agent(request_id), None);
    }

    #[test]
    fn test_broadcast_uses_app_subscription_index() {
        let hub = Hub::new();

        let conn1 = Uuid::new_v4();
        let conn2 = Uuid::new_v4();
        let conn3 = Uuid::new_v4();
        let app_a = Uuid::new_v4();
        let app_b = Uuid::new_v4();

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        let (tx3, mut rx3) = mpsc::unbounded_channel();

        hub.add_connection(conn1, Uuid::new_v4(), tx1);
        hub.add_connection(conn2, Uuid::new_v4(), tx2);
        hub.add_connection(conn3, Uuid::new_v4(), tx3);

        hub.subscribe(conn1, app_a);
        hub.subscribe(conn2, app_a);
        hub.subscribe(conn3, app_b);

        // Broadcast to app_a — only conn1 and conn2
        hub.broadcast(
            app_a,
            WsEvent::StateChange {
                component_id: Uuid::new_v4(),
                app_id: app_a,
                component_name: Some("test-component".to_string()),
                app_name: Some("test-app".to_string()),
                from: appcontrol_common::ComponentState::Unknown,
                to: appcontrol_common::ComponentState::Running,
                at: chrono::Utc::now(),
            },
        );

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
        assert!(rx3.try_recv().is_err()); // not subscribed to app_a
    }

    #[test]
    fn test_unsubscribe_cleans_reverse_index() {
        let hub = Hub::new();
        let conn = Uuid::new_v4();
        let app = Uuid::new_v4();

        let (tx, _rx) = mpsc::unbounded_channel();
        hub.add_connection(conn, Uuid::new_v4(), tx);
        hub.subscribe(conn, app);

        assert!(hub
            .app_subscriptions
            .get(&app)
            .map_or(false, |s| s.contains(&conn)));

        hub.unsubscribe(conn, app);

        assert!(hub
            .app_subscriptions
            .get(&app)
            .map_or(true, |s| !s.contains(&conn)));
    }

    #[test]
    fn test_remove_connection_cleans_subscriptions() {
        let hub = Hub::new();
        let conn = Uuid::new_v4();
        let app = Uuid::new_v4();

        let (tx, _rx) = mpsc::unbounded_channel();
        hub.add_connection(conn, Uuid::new_v4(), tx);
        hub.subscribe(conn, app);

        hub.remove_connection(conn);

        assert!(hub
            .app_subscriptions
            .get(&app)
            .map_or(true, |s| !s.contains(&conn)));
        assert_eq!(hub.connection_count(), 0);
    }
}
