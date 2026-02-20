use dashmap::DashMap;
use std::sync::RwLock;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Routes messages between agents and backend.
pub struct MessageRouter {
    /// conn_id -> agent sender
    agent_connections: DashMap<Uuid, mpsc::UnboundedSender<String>>,
    /// Backend sender (single connection)
    backend_sender: RwLock<Option<mpsc::UnboundedSender<String>>>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            agent_connections: DashMap::new(),
            backend_sender: RwLock::new(None),
        }
    }

    pub fn add_agent_connection(&self, conn_id: Uuid, sender: mpsc::UnboundedSender<String>) {
        self.agent_connections.insert(conn_id, sender);
    }

    pub fn remove_agent_connection(&self, conn_id: Uuid) {
        self.agent_connections.remove(&conn_id);
    }

    pub fn set_backend_sender(&self, sender: mpsc::UnboundedSender<String>) {
        *self.backend_sender.write().unwrap() = Some(sender);
    }

    /// Forward a message from an agent to the backend.
    pub fn forward_to_backend(&self, message: &str) {
        if let Some(sender) = self.backend_sender.read().unwrap().as_ref() {
            let _ = sender.send(message.to_string());
        }
    }

    /// Forward a message from the backend to the appropriate agent.
    /// For now, broadcasts to all agents; can be refined to target specific agents.
    pub fn forward_to_agent(&self, message: &str) {
        // Try to extract target agent_id from the message
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(message) {
            if let Some(payload) = msg.get("payload") {
                if let Some(_component_id) = payload.get("component_id") {
                    // Route to specific agent based on component assignment
                    // For now, broadcast to all
                }
            }
        }

        for entry in self.agent_connections.iter() {
            let _ = entry.value().send(message.to_string());
        }
    }
}
