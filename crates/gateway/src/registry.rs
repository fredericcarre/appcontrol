use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub agent_id: Uuid,
    pub hostname: String,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// Tracks connected agents.
pub struct AgentRegistry {
    /// conn_id -> agent info
    agents: DashMap<Uuid, AgentInfo>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
        }
    }

    pub fn register(&self, conn_id: Uuid, agent_id: Uuid, hostname: String) {
        let now = chrono::Utc::now();
        self.agents.insert(conn_id, AgentInfo {
            agent_id,
            hostname,
            last_heartbeat: now,
            connected_at: now,
        });
        tracing::info!("Agent {} ({}) registered as connection {}", agent_id, self.agents.get(&conn_id).map(|a| a.hostname.clone()).unwrap_or_default(), conn_id);
    }

    pub fn unregister(&self, conn_id: Uuid) {
        if let Some((_, info)) = self.agents.remove(&conn_id) {
            tracing::info!("Agent {} ({}) unregistered", info.agent_id, info.hostname);
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
        self.agents.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn get_conn_id_for_agent(&self, agent_id: Uuid) -> Option<Uuid> {
        self.agents.iter()
            .find(|entry| entry.value().agent_id == agent_id)
            .map(|entry| *entry.key())
    }
}
