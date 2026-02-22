use std::collections::VecDeque;
use std::sync::{Mutex, RwLock};

use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Maximum total bytes buffered when the backend is disconnected.
const MAX_BUFFER_BYTES: usize = 10 * 1024 * 1024; // 10 MB

/// Channel capacity for the backend sender (router → backend forwarder).
pub const CHANNEL_CAPACITY: usize = 4096;

/// Channel capacity for agent senders (router → agent WebSocket writer).
pub const AGENT_CHANNEL_CAPACITY: usize = 1024;

/// Routes messages between agents and backend with targeted delivery.
pub struct MessageRouter {
    /// agent_id → sender channel (one per connected agent)
    agent_senders: DashMap<Uuid, mpsc::Sender<String>>,
    /// Backend sender (single upstream connection, may be absent)
    backend_sender: RwLock<Option<mpsc::Sender<String>>>,
    /// Messages buffered while backend is disconnected, with total size tracking
    buffer: Mutex<MessageBuffer>,
}

struct MessageBuffer {
    messages: VecDeque<String>,
    total_bytes: usize,
}

impl MessageBuffer {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            total_bytes: 0,
        }
    }

    fn push(&mut self, msg: String) {
        // Drop oldest messages if we'd exceed the limit
        while self.total_bytes + msg.len() > MAX_BUFFER_BYTES && !self.messages.is_empty() {
            if let Some(dropped) = self.messages.pop_front() {
                self.total_bytes -= dropped.len();
                tracing::warn!(
                    dropped_bytes = dropped.len(),
                    "Buffer full — dropped oldest message"
                );
            }
        }
        self.total_bytes += msg.len();
        self.messages.push_back(msg);
    }

    fn drain(&mut self) -> Vec<String> {
        self.total_bytes = 0;
        self.messages.drain(..).collect()
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            agent_senders: DashMap::new(),
            backend_sender: RwLock::new(None),
            buffer: Mutex::new(MessageBuffer::new()),
        }
    }

    /// Register an agent's send channel, keyed by agent_id.
    pub fn add_agent(&self, agent_id: Uuid, sender: mpsc::Sender<String>) {
        self.agent_senders.insert(agent_id, sender);
    }

    /// Remove an agent's send channel.
    pub fn remove_agent(&self, agent_id: Uuid) {
        self.agent_senders.remove(&agent_id);
    }

    /// Set (or replace) the backend sender. Replays any buffered messages.
    pub fn set_backend_sender(&self, sender: mpsc::Sender<String>) {
        // Replay buffered messages first
        let buffered = {
            let mut buf = self.buffer.lock().unwrap();
            let count = buf.len();
            let bytes = buf.total_bytes();
            let msgs = buf.drain();
            if count > 0 {
                tracing::info!(count, bytes, "Replaying buffered messages to backend");
            }
            msgs
        };

        for msg in buffered {
            if sender.try_send(msg).is_err() {
                tracing::warn!("Backend channel full during replay — message dropped");
            }
        }

        *self.backend_sender.write().unwrap() = Some(sender);
    }

    /// Clear the backend sender (on disconnect). Future messages will be buffered.
    pub fn clear_backend_sender(&self) {
        *self.backend_sender.write().unwrap() = None;
    }

    /// Forward a message from an agent to the backend.
    /// If the backend is disconnected, the message is buffered (up to 10 MB).
    pub fn forward_to_backend(&self, message: &str) {
        let guard = self.backend_sender.read().unwrap();
        if let Some(sender) = guard.as_ref() {
            match sender.try_send(message.to_string()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!("Backend channel full — dropping message (backpressure)");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    drop(guard);
                    // Sender failed — backend just disconnected. Buffer.
                    self.clear_backend_sender();
                    self.buffer.lock().unwrap().push(message.to_string());
                }
            }
        } else {
            drop(guard);
            self.buffer.lock().unwrap().push(message.to_string());
        }
    }

    /// Forward a message from the backend to a specific agent by agent_id.
    /// Returns true if the message was delivered, false if the agent is unknown.
    pub fn forward_to_agent(&self, target_agent_id: Uuid, message: &str) -> bool {
        if let Some(sender) = self.agent_senders.get(&target_agent_id) {
            match sender.try_send(message.to_string()) {
                Ok(()) => return true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(
                        agent_id = %target_agent_id,
                        "Agent channel full — dropping message (backpressure)"
                    );
                    return false;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    tracing::warn!(
                        agent_id = %target_agent_id,
                        "Agent channel closed, removing stale sender"
                    );
                    drop(sender);
                    self.agent_senders.remove(&target_agent_id);
                }
            }
        } else {
            tracing::warn!(
                agent_id = %target_agent_id,
                "No agent with this ID connected to this gateway"
            );
        }
        false
    }

    pub fn connected_agent_count(&self) -> usize {
        self.agent_senders.len()
    }

    pub fn has_backend(&self) -> bool {
        self.backend_sender.read().unwrap().is_some()
    }

    pub fn buffer_stats(&self) -> (usize, usize) {
        let buf = self.buffer.lock().unwrap();
        (buf.len(), buf.total_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_targeted_routing_delivers_to_correct_agent() {
        let router = MessageRouter::new();
        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();

        let (tx_a, mut rx_a) = mpsc::channel(AGENT_CHANNEL_CAPACITY);
        let (tx_b, mut rx_b) = mpsc::channel(AGENT_CHANNEL_CAPACITY);
        router.add_agent(agent_a, tx_a);
        router.add_agent(agent_b, tx_b);

        // Send to agent_a only
        let delivered = router.forward_to_agent(agent_a, r#"{"cmd":"start"}"#);
        assert!(delivered);

        // agent_a should have the message
        assert_eq!(rx_a.try_recv().unwrap(), r#"{"cmd":"start"}"#);
        // agent_b should have nothing
        assert!(rx_b.try_recv().is_err());
    }

    #[test]
    fn test_routing_to_unknown_agent_returns_false() {
        let router = MessageRouter::new();
        let unknown = Uuid::new_v4();
        assert!(!router.forward_to_agent(unknown, "hello"));
    }

    #[test]
    fn test_backend_message_buffered_when_disconnected() {
        let router = MessageRouter::new();

        router.forward_to_backend("msg1");
        router.forward_to_backend("msg2");

        let (count, bytes) = router.buffer_stats();
        assert_eq!(count, 2);
        assert_eq!(bytes, 8); // "msg1" + "msg2"
    }

    #[test]
    fn test_buffer_replayed_on_backend_connect() {
        let router = MessageRouter::new();

        router.forward_to_backend("buffered1");
        router.forward_to_backend("buffered2");

        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        router.set_backend_sender(tx);

        // Buffered messages replayed
        assert_eq!(rx.try_recv().unwrap(), "buffered1");
        assert_eq!(rx.try_recv().unwrap(), "buffered2");

        // New messages go directly
        router.forward_to_backend("live");
        assert_eq!(rx.try_recv().unwrap(), "live");

        // Buffer should be empty
        let (count, _) = router.buffer_stats();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_buffer_evicts_oldest_when_full() {
        let router = MessageRouter::new();

        // Fill buffer to near capacity with a large message
        let big_msg = "x".repeat(MAX_BUFFER_BYTES - 10);
        router.forward_to_backend(&big_msg);
        assert_eq!(router.buffer_stats().0, 1);

        // Adding another message should evict the first
        let small_msg = "y".repeat(100);
        router.forward_to_backend(&small_msg);

        let (count, bytes) = router.buffer_stats();
        assert_eq!(count, 1);
        assert_eq!(bytes, 100);
    }

    #[test]
    fn test_remove_agent_prevents_delivery() {
        let router = MessageRouter::new();
        let agent_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(AGENT_CHANNEL_CAPACITY);
        router.add_agent(agent_id, tx);

        assert!(router.forward_to_agent(agent_id, "before"));
        router.remove_agent(agent_id);
        assert!(!router.forward_to_agent(agent_id, "after"));
    }
}
