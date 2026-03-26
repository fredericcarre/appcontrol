use crate::db::DbPool;
use uuid::Uuid;

/// Log an action to the action_log table BEFORE the action executes.
/// This is a critical rule: log before execute.
/// Returns the action_log ID which can be used to update the result later.
pub async fn log_action(
    pool: &DbPool,
    user_id: Uuid,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    details: serde_json::Value,
) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO action_log (user_id, action, resource_type, resource_id, details, status)
        VALUES ($1, $2, $3, $4, $5, 'in_progress')
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Mark an action as successfully completed.
pub async fn complete_action_success(pool: &DbPool, action_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE action_log
        SET status = 'success', completed_at = now()
        WHERE id = $1
        "#,
    )
    .bind(action_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark an action as failed with an error message.
pub async fn complete_action_failed(
    pool: &DbPool,
    action_id: Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE action_log
        SET status = 'failed', error_message = $2, completed_at = now()
        WHERE id = $1
        "#,
    )
    .bind(action_id)
    .bind(error_message)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark an action as cancelled.
pub async fn complete_action_cancelled(pool: &DbPool, action_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE action_log
        SET status = 'cancelled', completed_at = now()
        WHERE id = $1
        "#,
    )
    .bind(action_id)
    .execute(pool)
    .await?;

    Ok(())
}
