use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Tracks in-flight operations per application to prevent concurrent
/// start/stop/restart on the same application.
///
/// Without this, rapid duplicate POST requests (e.g., from a flaky network)
/// would spawn multiple background tasks that race to transition the same
/// components, leading to unpredictable FSM states.
#[derive(Debug, Clone)]
pub struct OperationLock {
    /// Maps application_id → active operation info.
    active: Arc<DashMap<Uuid, ActiveOperation>>,
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
    #[error("Application {app_id} already has an active '{operation}' operation (started at {started_at} by user {user_id})")]
    Conflict {
        app_id: Uuid,
        operation: String,
        started_at: chrono::DateTime<chrono::Utc>,
        user_id: Uuid,
    },
}

impl Default for OperationLock {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationLock {
    pub fn new() -> Self {
        Self {
            active: Arc::new(DashMap::new()),
        }
    }

    /// Try to acquire the lock for an operation on an application.
    /// Returns Ok(guard) if acquired, or Err(LockError::Conflict) if another operation is active.
    pub fn try_lock(
        &self,
        app_id: Uuid,
        operation: &str,
        user_id: Uuid,
    ) -> Result<OperationGuard, LockError> {
        use dashmap::mapref::entry::Entry;

        match self.active.entry(app_id) {
            Entry::Vacant(entry) => {
                let op = ActiveOperation {
                    operation: operation.to_string(),
                    started_at: chrono::Utc::now(),
                    user_id,
                };
                entry.insert(op);
                Ok(OperationGuard {
                    active: self.active.clone(),
                    app_id,
                })
            }
            Entry::Occupied(entry) => {
                let existing = entry.get();
                // Check for stale locks (> 30 minutes = probably a leaked lock)
                let age = chrono::Utc::now() - existing.started_at;
                if age.num_minutes() > 30 {
                    tracing::warn!(
                        app_id = %app_id,
                        operation = %existing.operation,
                        age_minutes = age.num_minutes(),
                        "Stale operation lock detected — force-releasing"
                    );
                    drop(entry);
                    self.active.remove(&app_id);
                    // Retry acquisition
                    let op = ActiveOperation {
                        operation: operation.to_string(),
                        started_at: chrono::Utc::now(),
                        user_id,
                    };
                    self.active.insert(app_id, op);
                    Ok(OperationGuard {
                        active: self.active.clone(),
                        app_id,
                    })
                } else {
                    Err(LockError::Conflict {
                        app_id,
                        operation: existing.operation.clone(),
                        started_at: existing.started_at,
                        user_id: existing.user_id,
                    })
                }
            }
        }
    }

    /// Check if an application has an active operation (for status reporting).
    #[allow(dead_code)]
    pub fn get_active(&self, app_id: Uuid) -> Option<ActiveOperation> {
        self.active.get(&app_id).map(|entry| entry.value().clone())
    }
}

/// RAII guard that releases the lock when the operation completes (or fails).
#[derive(Debug)]
pub struct OperationGuard {
    active: Arc<DashMap<Uuid, ActiveOperation>>,
    app_id: Uuid,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.active.remove(&self.app_id);
        tracing::debug!(app_id = %self.app_id, "Operation lock released");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_acquire_and_release() {
        let lock = OperationLock::new();
        let app_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Acquire lock
        let guard = lock.try_lock(app_id, "start", user_id).unwrap();
        assert!(lock.get_active(app_id).is_some());

        // Second acquire should fail
        let result = lock.try_lock(app_id, "stop", user_id);
        assert!(result.is_err());

        // Release lock
        drop(guard);
        assert!(lock.get_active(app_id).is_none());

        // Now should succeed
        let _guard2 = lock.try_lock(app_id, "stop", user_id).unwrap();
    }

    #[test]
    fn test_different_apps_can_lock_simultaneously() {
        let lock = OperationLock::new();
        let app1 = Uuid::new_v4();
        let app2 = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let _g1 = lock.try_lock(app1, "start", user_id).unwrap();
        let _g2 = lock.try_lock(app2, "start", user_id).unwrap();
    }

    #[test]
    fn test_conflict_error_message() {
        let lock = OperationLock::new();
        let app_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let _guard = lock.try_lock(app_id, "start", user_id).unwrap();
        let err = lock.try_lock(app_id, "stop", user_id).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("start"));
        assert!(msg.contains(&app_id.to_string()));
    }
}
