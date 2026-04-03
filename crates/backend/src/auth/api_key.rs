use crate::db::DbPool;

use super::AuthUser;

/// Validate an API key (format: ac_XXXXX) and return the associated user.
pub async fn validate_api_key(pool: &DbPool, key: &str) -> Result<AuthUser, ApiKeyError> {
    if !key.starts_with("ac_") {
        return Err(ApiKeyError::InvalidFormat);
    }

    let row = crate::repository::auth_queries::find_api_key(pool, key)
        .await
        .map_err(|e| ApiKeyError::Database(e.to_string()))?
        .ok_or(ApiKeyError::NotFound)?;

    // Update last_used_at (fire-and-forget)
    let _ = crate::repository::auth_queries::update_api_key_last_used(pool, row.id).await;

    Ok(AuthUser {
        user_id: row.user_id,
        organization_id: row.organization_id,
        email: row.email,
        role: row.role,
    })
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
