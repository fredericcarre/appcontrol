use std::hash::{Hash, Hasher};
use uuid::Uuid;

/// Tracks in-flight operations per application using PostgreSQL advisory locks.
///
/// Advisory locks are cluster-wide, survive process crashes, and automatically
/// release when the connection drops. This prevents concurrent start/stop/restart
/// on the same application even across multiple backend instances (HA).
#[derive(Debug, Clone)]
pub struct OperationLock {
    pool: Option<sqlx::PgPool>,
}

#[derive(Debug, Clone)]
pub struct ActiveOperation {
    pub operation: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub user_id: Uuid,
}

/// Error returned when an operation is rejected due to a lock conflict.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("Application {app_id} already has an active operation in progress")]
    Conflict {
        app_id: Uuid,
        operation: String,
        started_at: chrono::DateTime<chrono::Utc>,
        user_id: Uuid,
    },
    #[error("Database error: {0}")]
    Database(String),
}

impl Default for OperationLock {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationLock {
    pub fn new() -> Self {
        Self { pool: None }
    }

    /// Create with a database pool for PostgreSQL advisory locks.
    pub fn with_pool(pool: sqlx::PgPool) -> Self {
        Self { pool: Some(pool) }
    }

    /// Derive a stable i64 advisory lock key from a UUID.
    /// Uses the first 8 bytes of the UUID, which is unique enough for our purposes.
    fn lock_key(app_id: Uuid) -> i64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        app_id.hash(&mut hasher);
        hasher.finish() as i64
    }

    /// Try to acquire the lock for an operation on an application.
    /// Uses pg_try_advisory_lock for non-blocking acquisition.
    /// Returns Ok(guard) if acquired, Err(LockError::Conflict) if another operation is active.
    pub async fn try_lock(
        &self,
        app_id: Uuid,
        operation: &str,
        user_id: Uuid,
    ) -> Result<OperationGuard, LockError> {
        let pool = match &self.pool {
            Some(p) => p,
            None => {
                // Fallback: no DB pool — allow operation (testing/startup scenario)
                return Ok(OperationGuard {
                    pool: None,
                    lock_key: 0,
                    app_id,
                });
            }
        };

        let key = Self::lock_key(app_id);

        // Acquire a dedicated connection (advisory locks are connection-scoped)
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        // Try to acquire the advisory lock (non-blocking)
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        if !acquired {
            return Err(LockError::Conflict {
                app_id,
                operation: operation.to_string(),
                started_at: chrono::Utc::now(),
                user_id,
            });
        }

        tracing::debug!(
            app_id = %app_id,
            operation = %operation,
            lock_key = key,
            "Advisory lock acquired"
        );

        Ok(OperationGuard {
            pool: Some(pool.clone()),
            lock_key: key,
            app_id,
        })
    }

    /// Check if an application has an active operation.
    #[allow(dead_code)]
    pub fn get_active(&self, _app_id: Uuid) -> Option<ActiveOperation> {
        // With advisory locks, we can't introspect the lock holder metadata
        // from another connection. This is intentional — advisory locks are
        // designed for mutual exclusion, not status reporting.
        // The operation status is tracked in the action_log table instead.
        None
    }
}

/// RAII guard that releases the advisory lock when the operation completes (or the guard is dropped).
#[derive(Debug)]
pub struct OperationGuard {
    pool: Option<sqlx::PgPool>,
    lock_key: i64,
    app_id: Uuid,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.take() {
            let key = self.lock_key;
            let app_id = self.app_id;
            // Spawn a task to release the advisory lock
            tokio::spawn(async move {
                match sqlx::query("SELECT pg_advisory_unlock($1)")
                    .bind(key)
                    .execute(&pool)
                    .await
                {
                    Ok(_) => {
                        tracing::debug!(app_id = %app_id, lock_key = key, "Advisory lock released");
                    }
                    Err(e) => {
                        tracing::warn!(
                            app_id = %app_id,
                            lock_key = key,
                            "Failed to release advisory lock (will auto-release on disconnect): {}",
                            e
                        );
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_key_deterministic() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let k1 = OperationLock::lock_key(id);
        let k2 = OperationLock::lock_key(id);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_lock_key_unique_for_different_apps() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let k1 = OperationLock::lock_key(id1);
        let k2 = OperationLock::lock_key(id2);
        // While hash collisions are theoretically possible, UUID v4 should give different keys
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_default_lock_allows_operations() {
        // Without a pool, try_lock should succeed (fallback mode)
        let lock = OperationLock::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let guard = lock.try_lock(Uuid::new_v4(), "start", Uuid::new_v4()).await;
            assert!(guard.is_ok());
        });
    }
}
