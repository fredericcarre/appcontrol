use crate::db::{self, DbPool, DbUuid};


use super::AuthUser;

/// Validate an API key (format: ac_XXXXX) and return the associated user.
pub async fn validate_api_key(pool: &DbPool, key: &str) -> Result<AuthUser, ApiKeyError> {
    if !key.starts_with("ac_") {
        return Err(ApiKeyError::InvalidFormat);
    }

    #[cfg(feature = "postgres")]
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

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = {
        // SQLite: compute SHA-256 hash in Rust since SQLite doesn't have sha256()
        use sha2::{Digest, Sha256};
        let hash = hex::encode(Sha256::digest(key.as_bytes()));
        sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT ak.id, ak.user_id, u.organization_id, u.email, u.role \
             FROM api_keys ak \
             JOIN users u ON u.id = ak.user_id \
             WHERE ak.key_hash = $1 \
               AND ak.is_active = 1 \
               AND (ak.expires_at IS NULL OR ak.expires_at > {})",
            db::sql::now()
        ))
        .bind(hash)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiKeyError::Database(e.to_string()))?
        .ok_or(ApiKeyError::NotFound)?
    };

    // Update last_used_at
    let _ = sqlx::query(&format!(
        "UPDATE api_keys SET last_used_at = {} WHERE id = $1",
        db::sql::now()
    ))
    .bind(row.id)
    .execute(pool)
    .await;

    Ok(AuthUser {
        user_id: *row.user_id,
        organization_id: *row.organization_id,
        email: row.email,
        role: row.role,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct ApiKeyRow {
    id: DbUuid,
    user_id: DbUuid,
    organization_id: DbUuid,
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
