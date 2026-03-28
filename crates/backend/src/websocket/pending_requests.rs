//! Pending request manager for async agent responses.
//!
//! When the backend sends a request to an agent (e.g., GetProcessLogs), the agent
//! responds asynchronously. This manager tracks pending requests and allows the
//! WebSocket handler to complete them when responses arrive.

use dashmap::DashMap;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use uuid::Uuid;

/// Result type for pending requests.
pub type LogRequestResult = Result<serde_json::Value, String>;

/// A pending request with its completion channel.
struct PendingRequest {
    tx: oneshot::Sender<LogRequestResult>,
    created_at: Instant,
}

/// Manager for pending log retrieval requests.
pub struct PendingLogRequests {
    requests: DashMap<Uuid, PendingRequest>,
    timeout: Duration,
}

impl PendingLogRequests {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Register a new pending request, returns a receiver for the result.
    pub fn register(&self, request_id: Uuid) -> oneshot::Receiver<LogRequestResult> {
        let (tx, rx) = oneshot::channel();
        self.requests.insert(
            request_id,
            PendingRequest {
                tx,
                created_at: Instant::now(),
            },
        );
        rx
    }

    /// Complete a pending request with a result.
    pub fn complete(&self, request_id: Uuid, result: LogRequestResult) {
        if let Some((_, pending)) = self.requests.remove(&request_id) {
            let _ = pending.tx.send(result);
        } else {
            tracing::debug!(
                request_id = %request_id,
                "Received response for unknown request (already timed out?)"
            );
        }
    }

    /// Clean up expired requests (should be called periodically).
    pub fn cleanup_expired(&self) {
        let now = Instant::now();
        self.requests
            .retain(|_, req| now.duration_since(req.created_at) < self.timeout);
    }

    /// Get the timeout duration for waiting on responses.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl Default for PendingLogRequests {
    fn default() -> Self {
        Self::new()
    }
}
