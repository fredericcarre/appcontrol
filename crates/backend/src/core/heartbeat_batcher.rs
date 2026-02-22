//! Batched heartbeat updates for high-scale agent deployments.
//!
//! Instead of executing one `UPDATE agents SET last_heartbeat_at = now() WHERE id = $1`
//! per heartbeat (which at 2500 agents = 2500 UPDATE/min), this batcher collects
//! agent_ids and flushes a single `UPDATE ... WHERE id = ANY($1)` every flush interval.
//!
//! At 10K+ agents this reduces PostgreSQL write load by ~99.6%.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

/// How often to flush batched heartbeats to the database.
const FLUSH_INTERVAL_SECS: u64 = 5;

/// Collects agent heartbeat timestamps and flushes them in bulk.
pub struct HeartbeatBatcher {
    pending: Arc<Mutex<HashSet<Uuid>>>,
}

impl HeartbeatBatcher {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Record an agent heartbeat. This is O(1) and non-blocking (just inserts into a set).
    pub async fn record(&self, agent_id: Uuid) {
        self.pending.lock().await.insert(agent_id);
    }

    /// Background flush loop. Call via `tokio::spawn`.
    pub async fn run(&self, db: sqlx::PgPool) {
        let mut interval = tokio::time::interval(Duration::from_secs(FLUSH_INTERVAL_SECS));

        loop {
            interval.tick().await;

            // Swap out the pending set atomically
            let agent_ids: Vec<Uuid> = {
                let mut pending = self.pending.lock().await;
                if pending.is_empty() {
                    continue;
                }
                let ids: Vec<Uuid> = pending.drain().collect();
                ids
            };

            let count = agent_ids.len();

            // Single UPDATE for all agents in the batch
            if let Err(e) = sqlx::query(
                "UPDATE agents SET last_heartbeat_at = now() WHERE id = ANY($1)",
            )
            .bind(&agent_ids)
            .execute(&db)
            .await
            {
                tracing::warn!(count, "Failed to batch-update heartbeats: {}", e);
            } else {
                tracing::trace!(count, "Flushed heartbeat batch");
            }
        }
    }
}

impl Default for HeartbeatBatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_collects_unique_ids() {
        let batcher = HeartbeatBatcher::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        batcher.record(id1).await;
        batcher.record(id2).await;
        batcher.record(id1).await; // duplicate

        let pending = batcher.pending.lock().await;
        assert_eq!(pending.len(), 2);
        assert!(pending.contains(&id1));
        assert!(pending.contains(&id2));
    }

    #[tokio::test]
    async fn test_pending_can_be_drained() {
        let batcher = HeartbeatBatcher::new();
        let id1 = Uuid::new_v4();
        batcher.record(id1).await;

        {
            let mut pending = batcher.pending.lock().await;
            let ids: Vec<Uuid> = pending.drain().collect();
            assert_eq!(ids.len(), 1);
        }

        let pending = batcher.pending.lock().await;
        assert!(pending.is_empty());
    }
}
