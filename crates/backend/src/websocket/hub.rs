use dashmap::DashMap;
use std::collections::HashSet;
use tokio::sync::mpsc;
use uuid::Uuid;

use appcontrol_common::WsEvent;

/// A connection entry in the hub.
#[allow(dead_code)]
struct Connection {
    user_id: Uuid,
    sender: mpsc::UnboundedSender<String>,
    subscriptions: HashSet<Uuid>, // app_ids
}

/// WebSocket subscription hub. Manages connections and broadcasts events.
pub struct Hub {
    connections: DashMap<Uuid, Connection>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
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
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
