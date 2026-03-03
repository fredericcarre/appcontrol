//! Terminal session management for agent interactive access.
//!
//! This module tracks active terminal sessions between frontend clients and agents.
//! Sessions have a 30-minute idle timeout and are admin-only.

use dashmap::DashMap;
use std::time::Instant;
use uuid::Uuid;

/// Metadata for an active terminal session.
pub struct TerminalSession {
    /// The session ID (used by frontend and for routing).
    pub session_id: Uuid,
    /// The request_id sent to the agent (for routing agent responses back).
    pub request_id: Uuid,
    /// The agent hosting this terminal.
    pub agent_id: Uuid,
    /// The frontend WebSocket connection ID.
    pub conn_id: Uuid,
    /// The user who started the session.
    pub user_id: Uuid,
    /// When the session was created.
    pub created_at: Instant,
    /// Last activity timestamp (updated on input/output).
    pub last_activity: Instant,
}

/// Manages terminal sessions across all agents.
pub struct TerminalSessionManager {
    /// Active sessions keyed by session_id.
    sessions: DashMap<Uuid, TerminalSession>,
    /// Reverse mapping: request_id -> session_id (for routing agent responses).
    request_to_session: DashMap<Uuid, Uuid>,
    /// Reverse mapping: agent_id -> set of session_ids (for cleanup on agent disconnect).
    agent_sessions: DashMap<Uuid, Vec<Uuid>>,
}

impl TerminalSessionManager {
    /// Create a new terminal session manager.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            request_to_session: DashMap::new(),
            agent_sessions: DashMap::new(),
        }
    }

    /// Create a new terminal session.
    ///
    /// Returns the session_id and request_id (for sending to agent).
    pub fn create_session(
        &self,
        agent_id: Uuid,
        conn_id: Uuid,
        user_id: Uuid,
    ) -> (Uuid, Uuid) {
        let session_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let now = Instant::now();

        let session = TerminalSession {
            session_id,
            request_id,
            agent_id,
            conn_id,
            user_id,
            created_at: now,
            last_activity: now,
        };

        self.sessions.insert(session_id, session);
        self.request_to_session.insert(request_id, session_id);

        // Track session by agent
        self.agent_sessions
            .entry(agent_id)
            .or_default()
            .push(session_id);

        tracing::info!(
            session_id = %session_id,
            request_id = %request_id,
            agent_id = %agent_id,
            user_id = %user_id,
            "Terminal session created"
        );

        (session_id, request_id)
    }

    /// Get session by session_id.
    pub fn get_session(&self, session_id: Uuid) -> Option<dashmap::mapref::one::Ref<'_, Uuid, TerminalSession>> {
        self.sessions.get(&session_id)
    }

    /// Get session_id from request_id (for routing agent responses).
    pub fn get_session_id_by_request(&self, request_id: Uuid) -> Option<Uuid> {
        self.request_to_session.get(&request_id).map(|r| *r)
    }

    /// Update last_activity for a session.
    pub fn touch_session(&self, session_id: Uuid) {
        if let Some(mut session) = self.sessions.get_mut(&session_id) {
            session.last_activity = Instant::now();
        }
    }

    /// Remove a session.
    pub fn remove_session(&self, session_id: Uuid) -> Option<TerminalSession> {
        if let Some((_, session)) = self.sessions.remove(&session_id) {
            self.request_to_session.remove(&session.request_id);

            // Remove from agent sessions list
            if let Some(mut agent_sessions) = self.agent_sessions.get_mut(&session.agent_id) {
                agent_sessions.retain(|&s| s != session_id);
            }

            tracing::info!(
                session_id = %session_id,
                agent_id = %session.agent_id,
                "Terminal session removed"
            );

            Some(session)
        } else {
            None
        }
    }

    /// Get sessions that have been idle longer than the timeout.
    pub fn get_idle_sessions(&self, timeout: std::time::Duration) -> Vec<Uuid> {
        let now = Instant::now();
        self.sessions
            .iter()
            .filter(|entry| now.duration_since(entry.last_activity) > timeout)
            .map(|entry| entry.session_id)
            .collect()
    }

    /// Remove all sessions for a disconnected agent.
    pub fn remove_agent_sessions(&self, agent_id: Uuid) -> Vec<Uuid> {
        if let Some((_, session_ids)) = self.agent_sessions.remove(&agent_id) {
            for session_id in &session_ids {
                if let Some((_, session)) = self.sessions.remove(session_id) {
                    self.request_to_session.remove(&session.request_id);
                }
            }
            tracing::info!(
                agent_id = %agent_id,
                sessions = session_ids.len(),
                "Removed terminal sessions for disconnected agent"
            );
            session_ids
        } else {
            Vec::new()
        }
    }

    /// Get active session count.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// List all active sessions (for admin API).
    pub fn list_sessions(&self) -> Vec<TerminalSessionInfo> {
        self.sessions
            .iter()
            .map(|entry| TerminalSessionInfo {
                session_id: entry.session_id,
                agent_id: entry.agent_id,
                user_id: entry.user_id,
                created_at: entry.created_at,
                last_activity: entry.last_activity,
                idle_seconds: entry.last_activity.elapsed().as_secs(),
            })
            .collect()
    }

    /// List sessions for a specific agent.
    pub fn list_agent_sessions(&self, agent_id: Uuid) -> Vec<TerminalSessionInfo> {
        self.sessions
            .iter()
            .filter(|entry| entry.agent_id == agent_id)
            .map(|entry| TerminalSessionInfo {
                session_id: entry.session_id,
                agent_id: entry.agent_id,
                user_id: entry.user_id,
                created_at: entry.created_at,
                last_activity: entry.last_activity,
                idle_seconds: entry.last_activity.elapsed().as_secs(),
            })
            .collect()
    }
}

impl Default for TerminalSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Public session info for API responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TerminalSessionInfo {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub user_id: Uuid,
    #[serde(skip)]
    pub created_at: Instant,
    #[serde(skip)]
    pub last_activity: Instant,
    pub idle_seconds: u64,
}

/// Idle session cleanup task.
/// Runs in the background and closes sessions idle for more than 30 minutes.
pub async fn run_idle_session_cleanup(
    manager: std::sync::Arc<TerminalSessionManager>,
    ws_hub: std::sync::Arc<crate::websocket::Hub>,
) {
    let timeout = std::time::Duration::from_secs(30 * 60); // 30 minutes
    let check_interval = std::time::Duration::from_secs(60); // Check every minute

    loop {
        tokio::time::sleep(check_interval).await;

        let idle_sessions = manager.get_idle_sessions(timeout);
        for session_id in idle_sessions {
            if let Some(session) = manager.remove_session(session_id) {
                tracing::info!(
                    session_id = %session_id,
                    agent_id = %session.agent_id,
                    "Closing idle terminal session (30 min timeout)"
                );

                // Send TerminalClose to agent
                let close_msg = appcontrol_common::BackendMessage::TerminalClose {
                    request_id: session.request_id,
                };
                ws_hub.send_to_agent(session.agent_id, close_msg);

                // Notify frontend
                // Note: We can't easily send to a specific connection here.
                // The frontend will notice when it stops receiving output.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_session() {
        let manager = TerminalSessionManager::new();
        let agent_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let (session_id, request_id) = manager.create_session(agent_id, conn_id, user_id);

        assert!(manager.get_session(session_id).is_some());
        assert_eq!(manager.get_session_id_by_request(request_id), Some(session_id));
        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_remove_session() {
        let manager = TerminalSessionManager::new();
        let agent_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let (session_id, request_id) = manager.create_session(agent_id, conn_id, user_id);

        let removed = manager.remove_session(session_id);
        assert!(removed.is_some());
        assert!(manager.get_session(session_id).is_none());
        assert_eq!(manager.get_session_id_by_request(request_id), None);
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_remove_agent_sessions() {
        let manager = TerminalSessionManager::new();
        let agent_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create multiple sessions for the same agent
        let (session_id1, _) = manager.create_session(agent_id, conn_id, user_id);
        let (session_id2, _) = manager.create_session(agent_id, conn_id, user_id);

        assert_eq!(manager.session_count(), 2);

        let removed = manager.remove_agent_sessions(agent_id);
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&session_id1));
        assert!(removed.contains(&session_id2));
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_touch_session() {
        let manager = TerminalSessionManager::new();
        let agent_id = Uuid::new_v4();
        let conn_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let (session_id, _) = manager.create_session(agent_id, conn_id, user_id);

        let initial_activity = manager.get_session(session_id).unwrap().last_activity;

        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.touch_session(session_id);

        let new_activity = manager.get_session(session_id).unwrap().last_activity;
        assert!(new_activity > initial_activity);
    }
}
