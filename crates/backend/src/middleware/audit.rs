use sqlx::PgPool;
use uuid::Uuid;

/// Log an action to the action_log table BEFORE the action executes.
/// This is a critical rule: log before execute.
pub async fn log_action(
    pool: &PgPool,
    user_id: Uuid,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    details: serde_json::Value,
) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO action_log (user_id, action, resource_type, resource_id, details)
        VALUES ($1, $2, $3, $4, $5)
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
