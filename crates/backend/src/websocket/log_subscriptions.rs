//! Log subscription manager for real-time log streaming.
//!
//! Tracks which frontend connections are subscribed to logs from which agents/gateways.

use dashmap::{DashMap, DashSet};
use uuid::Uuid;

/// Manages log subscriptions from frontend clients.
///
/// Frontend clients can subscribe to logs from specific agents or gateways.
/// When log entries arrive, they are routed only to subscribed connections.
pub struct LogSubscriptionManager {
    /// agent_id → set of conn_ids subscribed to that agent's logs
    agent_subscriptions: DashMap<Uuid, DashSet<Uuid>>,
    /// gateway_id → set of conn_ids subscribed to that gateway's logs
    gateway_subscriptions: DashMap<Uuid, DashSet<Uuid>>,
    /// conn_id → minimum log level filter ("TRACE", "DEBUG", "INFO", "WARN", "ERROR")
    /// Stored per connection, applies to all their subscriptions
    connection_levels: DashMap<Uuid, String>,
}

impl LogSubscriptionManager {
    /// Create a new LogSubscriptionManager.
    pub fn new() -> Self {
        Self {
            agent_subscriptions: DashMap::new(),
            gateway_subscriptions: DashMap::new(),
            connection_levels: DashMap::new(),
        }
    }

    /// Subscribe a connection to agent logs.
    pub fn subscribe_agent(&self, conn_id: Uuid, agent_id: Uuid, min_level: String) {
        self.agent_subscriptions
            .entry(agent_id)
            .or_default()
            .insert(conn_id);
        self.connection_levels.insert(conn_id, min_level);
        tracing::debug!(
            conn_id = %conn_id,
            agent_id = %agent_id,
            "Added log subscription for agent"
        );
    }

    /// Subscribe a connection to gateway logs.
    pub fn subscribe_gateway(&self, conn_id: Uuid, gateway_id: Uuid, min_level: String) {
        self.gateway_subscriptions
            .entry(gateway_id)
            .or_default()
            .insert(conn_id);
        self.connection_levels.insert(conn_id, min_level);
        tracing::debug!(
            conn_id = %conn_id,
            gateway_id = %gateway_id,
            "Added log subscription for gateway"
        );
    }

    /// Unsubscribe a connection from agent logs.
    pub fn unsubscribe_agent(&self, conn_id: Uuid, agent_id: Uuid) {
        if let Some(subs) = self.agent_subscriptions.get(&agent_id) {
            subs.remove(&conn_id);
        }
        tracing::debug!(
            conn_id = %conn_id,
            agent_id = %agent_id,
            "Removed log subscription for agent"
        );
    }

    /// Unsubscribe a connection from gateway logs.
    pub fn unsubscribe_gateway(&self, conn_id: Uuid, gateway_id: Uuid) {
        if let Some(subs) = self.gateway_subscriptions.get(&gateway_id) {
            subs.remove(&conn_id);
        }
        tracing::debug!(
            conn_id = %conn_id,
            gateway_id = %gateway_id,
            "Removed log subscription for gateway"
        );
    }

    /// Remove all subscriptions for a connection (called on disconnect).
    pub fn remove_connection(&self, conn_id: Uuid) {
        // Clean up agent subscriptions
        for entry in self.agent_subscriptions.iter() {
            entry.value().remove(&conn_id);
        }
        // Clean up gateway subscriptions
        for entry in self.gateway_subscriptions.iter() {
            entry.value().remove(&conn_id);
        }
        self.connection_levels.remove(&conn_id);
        tracing::debug!(conn_id = %conn_id, "Removed all log subscriptions for connection");
    }

    /// Get all connections subscribed to a specific agent's logs.
    pub fn get_agent_subscribers(&self, agent_id: Uuid) -> Vec<Uuid> {
        self.agent_subscriptions
            .get(&agent_id)
            .map(|subs| subs.iter().map(|e| *e).collect())
            .unwrap_or_default()
    }

    /// Get all connections subscribed to a specific gateway's logs.
    pub fn get_gateway_subscribers(&self, gateway_id: Uuid) -> Vec<Uuid> {
        self.gateway_subscriptions
            .get(&gateway_id)
            .map(|subs| subs.iter().map(|e| *e).collect())
            .unwrap_or_default()
    }

    /// Get the minimum log level for a connection.
    pub fn get_connection_level(&self, conn_id: Uuid) -> String {
        self.connection_levels
            .get(&conn_id)
            .map(|e| e.clone())
            .unwrap_or_else(|| "INFO".to_string())
    }

    /// Check if a log level passes the filter for a connection.
    pub fn level_passes_filter(&self, conn_id: Uuid, level: &str) -> bool {
        let min_level = self.get_connection_level(conn_id);
        appcontrol_common::level_passes_filter(level, &min_level)
    }
}

impl Default for LogSubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_agent() {
        let manager = LogSubscriptionManager::new();
        let conn_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        manager.subscribe_agent(conn_id, agent_id, "INFO".to_string());

        let subs = manager.get_agent_subscribers(agent_id);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], conn_id);
    }

    #[test]
    fn test_subscribe_gateway() {
        let manager = LogSubscriptionManager::new();
        let conn_id = Uuid::new_v4();
        let gateway_id = Uuid::new_v4();

        manager.subscribe_gateway(conn_id, gateway_id, "DEBUG".to_string());

        let subs = manager.get_gateway_subscribers(gateway_id);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], conn_id);
    }

    #[test]
    fn test_unsubscribe() {
        let manager = LogSubscriptionManager::new();
        let conn_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        manager.subscribe_agent(conn_id, agent_id, "INFO".to_string());
        manager.unsubscribe_agent(conn_id, agent_id);

        let subs = manager.get_agent_subscribers(agent_id);
        assert!(subs.is_empty());
    }

    #[test]
    fn test_remove_connection() {
        let manager = LogSubscriptionManager::new();
        let conn_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let gateway_id = Uuid::new_v4();

        manager.subscribe_agent(conn_id, agent_id, "INFO".to_string());
        manager.subscribe_gateway(conn_id, gateway_id, "DEBUG".to_string());
        manager.remove_connection(conn_id);

        assert!(manager.get_agent_subscribers(agent_id).is_empty());
        assert!(manager.get_gateway_subscribers(gateway_id).is_empty());
    }

    #[test]
    fn test_level_filter() {
        let manager = LogSubscriptionManager::new();
        let conn_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        manager.subscribe_agent(conn_id, agent_id, "WARN".to_string());

        // WARN and ERROR should pass
        assert!(manager.level_passes_filter(conn_id, "WARN"));
        assert!(manager.level_passes_filter(conn_id, "ERROR"));
        // INFO and DEBUG should not pass
        assert!(!manager.level_passes_filter(conn_id, "INFO"));
        assert!(!manager.level_passes_filter(conn_id, "DEBUG"));
    }

    #[test]
    fn test_multiple_subscribers() {
        let manager = LogSubscriptionManager::new();
        let conn1 = Uuid::new_v4();
        let conn2 = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        manager.subscribe_agent(conn1, agent_id, "INFO".to_string());
        manager.subscribe_agent(conn2, agent_id, "DEBUG".to_string());

        let subs = manager.get_agent_subscribers(agent_id);
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&conn1));
        assert!(subs.contains(&conn2));
    }
}
