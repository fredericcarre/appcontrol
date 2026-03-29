use crate::db::{self, DbPool, DbUuid};
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
    let user_id: Uuid = user_id.into();
    let resource_id: Uuid = resource_id.into();

    #[cfg(feature = "postgres")]
    let row = sqlx::query_scalar::<_, DbUuid>(
        "INSERT INTO action_log (user_id, action, resource_type, resource_id, details, status) \
         VALUES ($1, $2, $3, $4, $5, 'in_progress') RETURNING id",
    )
    .bind(user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(&details)
    .fetch_one(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = {
        let id = DbUuid::from(Uuid::new_v4());
        sqlx::query(
            "INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details, status) \
             VALUES ($1, $2, $3, $4, $5, $6, 'in_progress')",
        )
        .bind(id)
        .bind(DbUuid::from(user_id))
        .bind(action)
        .bind(resource_type)
        .bind(DbUuid::from(resource_id))
        .bind(serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string()))
        .execute(pool)
        .await?;
        id
    };

    Ok(row.into_inner())
}

/// Mark an action as successfully completed.
pub async fn complete_action_success(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
) -> Result<(), sqlx::Error> {
    let action_id: Uuid = action_id.into();
    let sql = format!(
        "UPDATE action_log SET status = 'success', completed_at = {} WHERE id = $1",
        db::sql::now()
    );
    sqlx::query(&sql)
        .bind(DbUuid::from(action_id))
        .execute(pool)
        .await?;

    Ok(())
}

/// Mark an action as failed with an error message.
pub async fn complete_action_failed(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    let action_id: Uuid = action_id.into();
    let sql = format!(
        "UPDATE action_log SET status = 'failed', error_message = $2, completed_at = {} WHERE id = $1",
        db::sql::now()
    );
    sqlx::query(&sql)
        .bind(DbUuid::from(action_id))
        .bind(error_message)
        .execute(pool)
        .await?;

    Ok(())
}

/// Mark an action as cancelled.
pub async fn complete_action_cancelled(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
) -> Result<(), sqlx::Error> {
    let action_id: Uuid = action_id.into();
    let sql = format!(
        "UPDATE action_log SET status = 'cancelled', completed_at = {} WHERE id = $1",
        db::sql::now()
    );
    sqlx::query(&sql)
        .bind(DbUuid::from(action_id))
        .execute(pool)
        .await?;

    Ok(())
}
