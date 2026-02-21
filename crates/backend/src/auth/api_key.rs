use sqlx::PgPool;
use uuid::Uuid;

use super::AuthUser;

/// Validate an API key (format: ac_XXXXX) and return the associated user.
pub async fn validate_api_key(pool: &PgPool, key: &str) -> Result<AuthUser, ApiKeyError> {
    if !key.starts_with("ac_") {
        return Err(ApiKeyError::InvalidFormat);
    }

    let row = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT ak.id, ak.user_id, u.organization_id, u.email, u.role
        FROM api_keys ak
        JOIN users u ON u.id = ak.user_id
        WHERE ak.key_hash = encode(sha256($1::bytea), 'hex')
          AND ak.is_active = true
          AND (ak.expires_at IS NULL OR ak.expires_at > now())
        "#,
    )
    .bind(key.as_bytes())
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiKeyError::Database(e.to_string()))?
    .ok_or(ApiKeyError::NotFound)?;

    // Update last_used_at
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
        .bind(row.id)
        .execute(pool)
        .await;

    Ok(AuthUser {
        user_id: row.user_id,
        organization_id: row.organization_id,
        email: row.email,
        role: row.role,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    user_id: Uuid,
    organization_id: Uuid,
    email: String,
    role: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiKeyError {
    #[error("Invalid API key format")]
    InvalidFormat,
    #[error("API key not found")]
    NotFound,
    #[error("Database error: {0}")]
    Database(String),
}
