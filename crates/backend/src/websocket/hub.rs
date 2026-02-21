use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::RwLock;
use tokio::sync::mpsc;
use uuid::Uuid;

use appcontrol_common::{BackendMessage, WsEvent};

/// A connection entry in the hub.
#[allow(dead_code)]
struct Connection {
    user_id: Uuid,
    sender: mpsc::UnboundedSender<String>,
    subscriptions: HashSet<Uuid>, // app_ids
}

/// WebSocket subscription hub. Manages frontend connections, gateway connection,
/// and routes commands to agents via the gateway.
pub struct Hub {
    /// Frontend client connections
    connections: DashMap<Uuid, Connection>,
    /// Gateway sender — used to dispatch commands to agents
    gateway_sender: RwLock<Option<mpsc::UnboundedSender<String>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            gateway_sender: RwLock::new(None),
        }
    }

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

    pub fn subscribe(&self, conn_id: Uuid, app_id: Uuid) {
        if let Some(mut conn) = self.connections.get_mut(&conn_id) {
            conn.subscriptions.insert(app_id);
        }
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

    /// Register the gateway connection sender.
    pub fn set_gateway_sender(&self, sender: mpsc::UnboundedSender<String>) {
        *self.gateway_sender.write().unwrap() = Some(sender);
    }

    /// Clear the gateway sender (on disconnect).
    pub fn clear_gateway_sender(&self) {
        *self.gateway_sender.write().unwrap() = None;
    }

    /// Send a command to an agent via the gateway.
    pub fn send_to_agent(&self, message: BackendMessage) -> bool {
        let json = match serde_json::to_string(&message) {
            Ok(j) => j,
            Err(_) => return false,
        };
        if let Some(sender) = self.gateway_sender.read().unwrap().as_ref() {
            sender.send(json).is_ok()
        } else {
            tracing::warn!("No gateway connected, cannot send command to agent");
            false
        }
    }

    /// Check if a gateway is connected.
    pub fn has_gateway(&self) -> bool {
        self.gateway_sender.read().unwrap().is_some()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
