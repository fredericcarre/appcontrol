use crate::db::DbPool;
use uuid::Uuid;

/// Log an action to the action_log table BEFORE the action executes.
/// This is a critical rule: log before execute.
/// Returns the action_log ID which can be used to update the result later.
pub async fn log_action(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    action: &str,
    resource_type: &str,
    resource_id: impl Into<Uuid>,
    details: serde_json::Value,
) -> Result<Uuid, sqlx::Error> {
    crate::repository::misc_queries::log_action(pool, user_id, action, resource_type, resource_id, details).await
}

/// Mark an action as successfully completed.
pub async fn complete_action_success(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
) -> Result<(), sqlx::Error> {
    crate::repository::misc_queries::complete_action_success(pool, action_id).await
}

/// Mark an action as failed with an error message.
pub async fn complete_action_failed(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    crate::repository::misc_queries::complete_action_failed(pool, action_id, error_message).await
}

/// Mark an action as cancelled.
pub async fn complete_action_cancelled(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
) -> Result<(), sqlx::Error> {
    crate::repository::misc_queries::complete_action_cancelled(pool, action_id).await
}
