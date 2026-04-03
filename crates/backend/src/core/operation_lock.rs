use tokio::sync::watch;
use uuid::Uuid;

use crate::repository::core_queries;

/// Persistent operation locks backed by PostgreSQL.
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

/// Stale lock threshold
const STALE_THRESHOLD_SECONDS: i64 = 30;

/// Heartbeat interval
const HEARTBEAT_INTERVAL_SECONDS: u64 = 5;

impl OperationLock {
    pub fn new(pool: crate::db::DbPool) -> Self {
        let instance_id = format!(
            "{}-{}",
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            std::process::id()
        );
        Self { pool, instance_id }
    }

    pub async fn try_lock(
        &self,
        app_id: impl Into<Uuid>,
        operation: &str,
        user_id: Uuid,
    ) -> Result<OperationGuard, LockError> {
        let app_id: Uuid = app_id.into();

        // First, clean up any stale lock for this app
        self.cleanup_stale_lock(app_id).await?;

        // Try to insert a new lock
        let acquired =
            core_queries::try_insert_operation_lock(&self.pool, app_id, operation, user_id, &self.instance_id)
                .await
                .map_err(|e| LockError::Database(e.to_string()))?;

        if acquired {
            tracing::info!(
                app_id = %app_id,
                operation = %operation,
                user_id = %user_id,
                instance = %self.instance_id,
                "Operation lock acquired"
            );

            let (shutdown_tx, shutdown_rx) = watch::channel(false);

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
                Err(LockError::Database(
                    "Lock acquisition race condition, please retry".to_string(),
                ))
            }
        }
    }

    pub async fn get_active(
        &self,
        app_id: impl Into<Uuid>,
    ) -> Result<Option<ActiveOperation>, LockError> {
        let app_id: Uuid = app_id.into();

        let row = core_queries::get_active_operation(&self.pool, app_id)
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

    pub fn is_cancelled(&self, app_id: impl Into<Uuid>) -> bool {
        let app_id: Uuid = app_id.into();
        let pool = self.pool.clone();

        let handle = tokio::task::spawn(async move {
            core_queries::get_lock_status(&pool, app_id)
                .await
                .map(|s| s == "cancelling" || s == "cancelled")
                .unwrap_or(false)
        });

        futures_util::FutureExt::now_or_never(handle)
            .and_then(|r| r.ok())
            .unwrap_or(false)
    }

    pub async fn is_cancelled_async(&self, app_id: impl Into<Uuid>) -> bool {
        let app_id: Uuid = app_id.into();

        core_queries::get_lock_status(&self.pool, app_id)
            .await
            .map(|s| s == "cancelling" || s == "cancelled")
            .unwrap_or(false)
    }

    pub async fn request_cancel(&self, app_id: impl Into<Uuid>) -> Result<bool, LockError> {
        let app_id: Uuid = app_id.into();

        let rows = core_queries::request_cancel_operation(&self.pool, app_id)
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        if rows > 0 {
            tracing::info!(app_id = %app_id, "Operation cancellation requested");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn force_unlock(&self, app_id: impl Into<Uuid>) -> Result<bool, LockError> {
        let app_id: Uuid = app_id.into();

        let _ = self.request_cancel(app_id).await;

        let rows = core_queries::delete_operation_lock(&self.pool, app_id)
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        if rows > 0 {
            tracing::warn!(app_id = %app_id, "Operation lock force-released");
            Ok(true)
        } else {
            tracing::debug!(app_id = %app_id, "No lock to release");
            Ok(false)
        }
    }

    async fn cleanup_stale_lock(&self, app_id: Uuid) -> Result<(), LockError> {
        let rows = core_queries::cleanup_stale_lock(&self.pool, app_id, STALE_THRESHOLD_SECONDS)
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        if rows > 0 {
            tracing::warn!(
                app_id = %app_id,
                threshold_seconds = STALE_THRESHOLD_SECONDS,
                "Cleaned up stale operation lock"
            );
        }

        Ok(())
    }

    pub async fn cleanup_all_stale_locks(&self) -> Result<u64, LockError> {
        let count = core_queries::cleanup_all_stale_locks(&self.pool, STALE_THRESHOLD_SECONDS)
            .await
            .map_err(|e| LockError::Database(e.to_string()))?;

        if count > 0 {
            tracing::warn!(
                count = count,
                threshold_seconds = STALE_THRESHOLD_SECONDS,
                "Cleaned up stale operation locks"
            );
        }

        Ok(count)
    }

    pub async fn list_all(&self) -> Result<Vec<(Uuid, ActiveOperation)>, LockError> {
        let rows = core_queries::list_all_operation_locks(&self.pool)
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
                if let Err(e) = core_queries::update_heartbeat(&pool, app_id).await {
                    tracing::warn!(app_id = %app_id, error = %e, "Failed to update operation heartbeat");
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
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }

        let pool = self.pool.clone();
        let app_id = self.app_id;

        tokio::spawn(async move {
            match core_queries::delete_operation_lock(&pool, app_id).await {
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
