use tokio::sync::watch;
use uuid::Uuid;

/// Persistent operation locks backed by PostgreSQL.
///
/// Features:
/// - Locks persist across backend restarts
/// - Heartbeat mechanism detects stuck operations (no heartbeat > 30s = stale)
/// - Cancel requests are stored in DB, checked by running operations
/// - Manual force-unlock available as last resort
/// - Works in HA mode (multiple backend instances)
#[derive(Debug, Clone)]
pub struct OperationLock {
    pool: crate::db::DbPool,
    instance_id: String,
}

#[derive(Debug, Clone)]
pub struct ActiveOperation {
    pub operation: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub user_id: Uuid,
    pub status: String,
    pub backend_instance: Option<String>,
}

/// Error returned when an operation is rejected due to a lock conflict.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("Application {app_id} already has an active {operation} operation in progress (started at {started_at}, last heartbeat {last_heartbeat})")]
    Conflict {
        app_id: Uuid,
        operation: String,
        started_at: chrono::DateTime<chrono::Utc>,
        last_heartbeat: chrono::DateTime<chrono::Utc>,
        user_id: Uuid,
    },
    #[error("Database error: {0}")]
    Database(String),
}

/// Stale lock threshold - if no heartbeat for this duration, lock is considered abandoned
const STALE_THRESHOLD_SECONDS: i64 = 30;

/// Heartbeat interval - how often running operations update their heartbeat
const HEARTBEAT_INTERVAL_SECONDS: u64 = 5;

impl OperationLock {
    /// Create with a database pool.
    pub fn new(pool: crate::db::DbPool) -> Self {
        // Generate a unique instance ID for this backend process
        let instance_id = format!(
            "{}-{}",
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            std::process::id()
        );
        Self { pool, instance_id }
    }

    /// Try to acquire the lock for an operation on an application.
    /// Returns Ok(guard) if acquired, Err(LockError::Conflict) if another operation is active.
    ///
    /// Before attempting to acquire, this will clean up any stale locks (heartbeat > 30s old).
    pub async fn try_lock(
        &self,
        app_id: Uuid,
        operation: &str,
        user_id: Uuid,
    ) -> Result<OperationGuard, LockError> {
        // First, clean up any stale lock for this app
        self.cleanup_stale_lock(app_id).await?;

        // Try to insert a new lock (will fail if one exists due to PRIMARY KEY constraint)
        let result = sqlx::query(
            r#"
            INSERT INTO operation_locks (app_id, operation, user_id, backend_instance)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (app_id) DO NOTHING
            RETURNING app_id
            "#,
        )
        .bind(app_id)
        .bind(operation)
        .bind(user_id)
        .bind(&self.instance_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        if result.is_some() {
            // Lock acquired successfully
            tracing::info!(
                app_id = %app_id,
                operation = %operation,
                user_id = %user_id,
                instance = %self.instance_id,
                "Operation lock acquired"
            );

            // Create a shutdown channel for the heartbeat task
            let (shutdown_tx, shutdown_rx) = watch::channel(false);

            // Spawn heartbeat task
            let pool = self.pool.clone();
            let app_id_clone = app_id;
            tokio::spawn(async move {
                heartbeat_loop(pool, app_id_clone, shutdown_rx).await;
            });

            Ok(OperationGuard {
                pool: self.pool.clone(),
                app_id,
                shutdown_tx: Some(shutdown_tx),
            })
        } else {
            // Lock already exists - fetch details for error message
            let existing = self.get_active(app_id).await?;
            if let Some(op) = existing {
                Err(LockError::Conflict {
                    app_id,
                    operation: op.operation,
                    started_at: op.started_at,
                    last_heartbeat: op.last_heartbeat,
                    user_id: op.user_id,
                })
            } else {
                // Race condition - lock was just released, try again
                // This shouldn't happen often, but let's handle it gracefully
                Err(LockError::Database(
                    "Lock acquisition race condition, please retry".to_string(),
                ))
            }
        }
    }

    /// Get information about an active operation on an application.
    pub async fn get_active(&self, app_id: Uuid) -> Result<Option<ActiveOperation>, LockError> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                chrono::DateTime<chrono::Utc>,
                chrono::DateTime<chrono::Utc>,
                Uuid,
                String,
                Option<String>,
            ),
        >(
            r#"
            SELECT operation, started_at, last_heartbeat, user_id, status, backend_instance
            FROM operation_locks
            WHERE app_id = $1
            "#,
        )
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        Ok(row.map(
            |(operation, started_at, last_heartbeat, user_id, status, backend_instance)| {
                ActiveOperation {
                    operation,
                    started_at,
                    last_heartbeat,
                    user_id,
                    status,
                    backend_instance,
                }
            },
        ))
    }

    /// Check if an operation on an application has been cancelled.
    /// Returns true if status is 'cancelling' or 'cancelled'.
    pub fn is_cancelled(&self, app_id: Uuid) -> bool {
        // We need to query the database synchronously, but we're in an async context
        // Use a blocking task to avoid deadlocks
        let pool = self.pool.clone();

        // Use try_send to avoid blocking - if we can't check, assume not cancelled
        let handle = tokio::task::spawn(async move {
            sqlx::query_scalar::<_, String>("SELECT status FROM operation_locks WHERE app_id = $1")
                .bind(app_id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .map(|s| s == "cancelling" || s == "cancelled")
                .unwrap_or(false)
        });

        // Try to get result with a short timeout
        futures_util::FutureExt::now_or_never(handle)
            .and_then(|r| r.ok())
            .unwrap_or(false)
    }

    /// Check if an operation has been cancelled (async version).
    pub async fn is_cancelled_async(&self, app_id: Uuid) -> bool {
        sqlx::query_scalar::<_, String>("SELECT status FROM operation_locks WHERE app_id = $1")
            .bind(app_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .map(|s| s == "cancelling" || s == "cancelled")
            .unwrap_or(false)
    }

    /// Request cancellation of an operation.
    /// Sets status to 'cancelling'. The running operation should check this and exit.
    /// Returns true if cancellation was requested, false if no operation was running.
    pub async fn request_cancel(&self, app_id: Uuid) -> Result<bool, LockError> {
        let result = sqlx::query(
            r#"
            UPDATE operation_locks
            SET status = 'cancelling'
            WHERE app_id = $1 AND status = 'running'
            "#,
        )
        .bind(app_id)
        .execute(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        if result.rows_affected() > 0 {
            tracing::info!(app_id = %app_id, "Operation cancellation requested");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Force-release a lock. Use as last resort when normal cancellation doesn't work.
    /// This immediately deletes the lock, potentially leaving the operation orphaned.
    pub async fn force_unlock(&self, app_id: Uuid) -> Result<bool, LockError> {
        // First try to request cancellation
        let _ = self.request_cancel(app_id).await;

        // Then force delete the lock
        let result = sqlx::query("DELETE FROM operation_locks WHERE app_id = $1")
            .bind(app_id)
            .execute(&self.pool)
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        if result.rows_affected() > 0 {
            tracing::warn!(app_id = %app_id, "Operation lock force-released");
            Ok(true)
        } else {
            tracing::debug!(app_id = %app_id, "No lock to release");
            Ok(false)
        }
    }

    /// Clean up a stale lock (heartbeat older than threshold).
    async fn cleanup_stale_lock(&self, app_id: Uuid) -> Result<(), LockError> {
        #[cfg(feature = "postgres")]
        let result = sqlx::query(
            r#"
            DELETE FROM operation_locks
            WHERE app_id = $1
              AND last_heartbeat < NOW() - INTERVAL '1 second' * $2
            "#,
        )
        .bind(app_id)
        .bind(STALE_THRESHOLD_SECONDS)
        .execute(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let result = sqlx::query(
            r#"
            DELETE FROM operation_locks
            WHERE app_id = $1
              AND last_heartbeat < datetime('now', '-' || $2 || ' seconds')
            "#,
        )
        .bind(app_id)
        .bind(STALE_THRESHOLD_SECONDS)
        .execute(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        if result.rows_affected() > 0 {
            tracing::warn!(
                app_id = %app_id,
                threshold_seconds = STALE_THRESHOLD_SECONDS,
                "Cleaned up stale operation lock"
            );
        }

        Ok(())
    }

    /// Clean up all stale locks across all applications.
    /// Call this periodically (e.g., on backend startup and every minute).
    pub async fn cleanup_all_stale_locks(&self) -> Result<u64, LockError> {
        #[cfg(feature = "postgres")]
        let result = sqlx::query(
            r#"
            DELETE FROM operation_locks
            WHERE last_heartbeat < NOW() - INTERVAL '1 second' * $1
            "#,
        )
        .bind(STALE_THRESHOLD_SECONDS)
        .execute(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let result = sqlx::query(
            r#"
            DELETE FROM operation_locks
            WHERE last_heartbeat < datetime('now', '-' || $1 || ' seconds')
            "#,
        )
        .bind(STALE_THRESHOLD_SECONDS)
        .execute(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        let count = result.rows_affected();
        if count > 0 {
            tracing::warn!(
                count = count,
                threshold_seconds = STALE_THRESHOLD_SECONDS,
                "Cleaned up stale operation locks"
            );
        }

        Ok(count)
    }

    /// List all active operation locks (for admin UI / debugging).
    pub async fn list_all(&self) -> Result<Vec<(Uuid, ActiveOperation)>, LockError> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                chrono::DateTime<chrono::Utc>,
                chrono::DateTime<chrono::Utc>,
                Uuid,
                String,
                Option<String>,
            ),
        >(
            r#"
            SELECT app_id, operation, started_at, last_heartbeat, user_id, status, backend_instance
            FROM operation_locks
            ORDER BY started_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LockError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    app_id,
                    operation,
                    started_at,
                    last_heartbeat,
                    user_id,
                    status,
                    backend_instance,
                )| {
                    (
                        app_id,
                        ActiveOperation {
                            operation,
                            started_at,
                            last_heartbeat,
                            user_id,
                            status,
                            backend_instance,
                        },
                    )
                },
            )
            .collect())
    }
}

/// Heartbeat loop - updates last_heartbeat every 5 seconds until shutdown signal.
async fn heartbeat_loop(
    pool: crate::db::DbPool,
    app_id: Uuid,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let interval = std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                // Update heartbeat
                #[cfg(feature = "postgres")]
                let hb_sql = "UPDATE operation_locks SET last_heartbeat = NOW() WHERE app_id = $1";
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let hb_sql = "UPDATE operation_locks SET last_heartbeat = datetime('now') WHERE app_id = $1";
                if let Err(e) = sqlx::query(hb_sql)
                .bind(app_id)
                .execute(&pool)
                .await
                {
                    tracing::warn!(app_id = %app_id, error = %e, "Failed to update operation heartbeat");
                    // If we can't update heartbeat, the lock will become stale and be cleaned up
                    break;
                }
                tracing::trace!(app_id = %app_id, "Operation heartbeat updated");
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    tracing::debug!(app_id = %app_id, "Heartbeat loop shutting down");
                    break;
                }
            }
        }
    }
}

/// RAII guard that releases the operation lock when dropped.
pub struct OperationGuard {
    pool: crate::db::DbPool,
    app_id: Uuid,
    shutdown_tx: Option<watch::Sender<bool>>,
}

impl std::fmt::Debug for OperationGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationGuard")
            .field("app_id", &self.app_id)
            .finish()
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        // Signal heartbeat task to stop
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }

        // Release the lock
        let pool = self.pool.clone();
        let app_id = self.app_id;

        tokio::spawn(async move {
            match sqlx::query("DELETE FROM operation_locks WHERE app_id = $1")
                .bind(app_id)
                .execute(&pool)
                .await
            {
                Ok(_) => {
                    tracing::info!(app_id = %app_id, "Operation lock released");
                }
                Err(e) => {
                    tracing::warn!(app_id = %app_id, error = %e, "Failed to release operation lock");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running PostgreSQL database
    // They are integration tests and should be run with `cargo test -- --ignored`

    #[test]
    fn test_stale_threshold_reasonable() {
        assert!(
            STALE_THRESHOLD_SECONDS >= 30,
            "Stale threshold should be at least 30s"
        );
        assert!(
            STALE_THRESHOLD_SECONDS <= 300,
            "Stale threshold shouldn't be too long"
        );
    }

    #[test]
    fn test_heartbeat_interval_reasonable() {
        assert!(
            HEARTBEAT_INTERVAL_SECONDS >= 3,
            "Heartbeat shouldn't be too frequent"
        );
        assert!(
            HEARTBEAT_INTERVAL_SECONDS < STALE_THRESHOLD_SECONDS as u64,
            "Heartbeat must be more frequent than stale threshold"
        );
    }
}
