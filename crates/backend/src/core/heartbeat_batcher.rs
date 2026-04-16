//! Batched heartbeat updates for high-scale agent deployments.
//!
//! Instead of executing one `UPDATE agents SET last_heartbeat_at = now() WHERE id = $1`
//! per heartbeat (which at 2500 agents = 2500 UPDATE/min), this batcher collects
//! agent_ids and flushes a single `UPDATE ... WHERE id = ANY($1)` every flush interval.
//!
//! At 10K+ agents this reduces PostgreSQL write load by ~99.6%.
//!
//! On SQLite, writes are routed through the WriteQueue to avoid contention with
//! FSM state transitions from the sequencer (SQLite = single writer).

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
    #[cfg(feature = "postgres")]
    pub async fn run(&self, db: crate::db::DbPool) {
        let mut interval = tokio::time::interval(Duration::from_secs(FLUSH_INTERVAL_SECS));

        loop {
            interval.tick().await;

            let agent_ids: Vec<Uuid> = {
                let mut pending = self.pending.lock().await;
                if pending.is_empty() {
                    continue;
                }
                pending.drain().collect()
            };

            let count = agent_ids.len();
            if let Err(e) = batch_update_heartbeats(&db, &agent_ids).await {
                tracing::warn!(count, "Failed to batch-update heartbeats: {}", e);
            } else {
                tracing::trace!(count, "Flushed heartbeat batch");
            }
        }
    }

    /// Background flush loop for SQLite — routes writes through the write_queue
    /// to avoid contention with FSM state transitions from the sequencer.
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub async fn run(&self, db: crate::db::DbPool, write_queue: Arc<crate::db::WriteQueue>) {
        let mut interval = tokio::time::interval(Duration::from_secs(FLUSH_INTERVAL_SECS));

        loop {
            interval.tick().await;

            let agent_ids: Vec<Uuid> = {
                let mut pending = self.pending.lock().await;
                if pending.is_empty() {
                    continue;
                }
                pending.drain().collect()
            };

            let count = agent_ids.len();
            let db_clone = db.clone();
            let result: Result<(), sqlx::Error> = write_queue
                .execute(
                    move |_| async move { batch_update_heartbeats(&db_clone, &agent_ids).await },
                )
                .await;

            if let Err(e) = result {
                tracing::warn!(count, "Failed to batch-update heartbeats: {}", e);
            } else {
                tracing::trace!(count, "Flushed heartbeat batch");
            }
        }
    }
}

async fn batch_update_heartbeats(
    db: &crate::db::DbPool,
    agent_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    crate::repository::queries::batch_update_agent_heartbeats(db, agent_ids).await
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
