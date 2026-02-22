use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub agent_id: Uuid,
    pub hostname: String,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub cert_fingerprint: Option<String>,
}

/// Tracks connected agents with bidirectional lookups.
pub struct AgentRegistry {
    /// conn_id → agent info
    agents: DashMap<Uuid, AgentInfo>,
    /// agent_id → conn_id (reverse index for fast lookups)
    agent_to_conn: DashMap<Uuid, Uuid>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            agent_to_conn: DashMap::new(),
        }
    }

    /// Register an agent. Returns the previous AgentInfo if this agent_id was already registered
    /// (reconnection scenario — old connection replaced).
    pub fn register(&self, conn_id: Uuid, agent_id: Uuid, hostname: String, cert_fingerprint: Option<String>) -> Option<AgentInfo> {
        let now = chrono::Utc::now();

        // If this agent_id was already registered on a different conn, remove old entry
        let previous = if let Some((_, old_conn_id)) = self.agent_to_conn.remove(&agent_id) {
            let old_info = self.agents.remove(&old_conn_id).map(|(_, info)| info);
            if old_info.is_some() {
                tracing::warn!(
                    agent_id = %agent_id,
                    old_conn = %old_conn_id,
                    new_conn = %conn_id,
                    "Agent re-registered (replacing old connection)"
                );
            }
            old_info
        } else {
            None
        };

        let info = AgentInfo {
            agent_id,
            hostname: hostname.clone(),
            last_heartbeat: now,
            connected_at: now,
            cert_fingerprint,
        };
        self.agents.insert(conn_id, info);
        self.agent_to_conn.insert(agent_id, conn_id);

        tracing::info!(
            agent_id = %agent_id,
            hostname = %hostname,
            conn_id = %conn_id,
            "Agent registered"
        );

        previous
    }

    /// Unregister by conn_id. Returns the AgentInfo if found (for disconnect notifications).
    pub fn unregister(&self, conn_id: Uuid) -> Option<AgentInfo> {
        if let Some((_, info)) = self.agents.remove(&conn_id) {
            self.agent_to_conn.remove(&info.agent_id);
            tracing::info!(
                agent_id = %info.agent_id,
                hostname = %info.hostname,
                "Agent unregistered"
            );
            Some(info)
        } else {
            None
        }
    }

    pub fn heartbeat(&self, conn_id: Uuid) {
        if let Some(mut entry) = self.agents.get_mut(&conn_id) {
            entry.last_heartbeat = chrono::Utc::now();
        }
    }

    pub fn connected_count(&self) -> usize {
        self.agents.len()
    }

    pub fn list_agents(&self) -> Vec<AgentInfo> {
        self.agents
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Look up the agent_id for a given conn_id.
    pub fn get_agent_id(&self, conn_id: Uuid) -> Option<Uuid> {
        self.agents.get(&conn_id).map(|entry| entry.agent_id)
    }

    /// Look up the conn_id for a given agent_id.
    pub fn get_conn_id(&self, agent_id: Uuid) -> Option<Uuid> {
        self.agent_to_conn
            .get(&agent_id)
            .map(|entry| *entry.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let reg = AgentRegistry::new();
        let conn_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        reg.register(conn_id, agent_id, "host1".to_string(), None);

        assert_eq!(reg.connected_count(), 1);
        assert_eq!(reg.get_agent_id(conn_id), Some(agent_id));
        assert_eq!(reg.get_conn_id(agent_id), Some(conn_id));
    }

    #[test]
    fn test_unregister_returns_info() {
        let reg = AgentRegistry::new();
        let conn_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        reg.register(conn_id, agent_id, "host1".to_string(), None);

        let info = reg.unregister(conn_id).expect("should return AgentInfo");
        assert_eq!(info.agent_id, agent_id);
        assert_eq!(info.hostname, "host1");
        assert_eq!(reg.connected_count(), 0);
        assert!(reg.get_conn_id(agent_id).is_none());
    }

    #[test]
    fn test_unregister_unknown_returns_none() {
        let reg = AgentRegistry::new();
        assert!(reg.unregister(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_re_register_replaces_old_connection() {
        let reg = AgentRegistry::new();
        let agent_id = Uuid::new_v4();
        let conn1 = Uuid::new_v4();
        let conn2 = Uuid::new_v4();

        reg.register(conn1, agent_id, "host1".to_string(), None);
        let prev = reg.register(conn2, agent_id, "host1".to_string(), None);

        assert!(prev.is_some());
        assert_eq!(reg.connected_count(), 1);
        assert_eq!(reg.get_conn_id(agent_id), Some(conn2));
    }
}
