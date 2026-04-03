//! Query functions for auth domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{self, DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Dev auth queries
// ============================================================================

/// Find a user by role for dev login (admin, operator, viewer).
pub async fn find_user_by_role(
    pool: &DbPool,
    role: &str,
) -> Result<Option<(Uuid, Uuid, String, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let q = "SELECT u.id, u.organization_id, u.email, u.role FROM users u WHERE u.role = $1 AND u.is_active = true LIMIT 1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let q = "SELECT u.id, u.organization_id, u.email, u.role FROM users u WHERE u.role = $1 AND u.is_active = 1 LIMIT 1";

    sqlx::query_as(q)
        .bind(role)
        .fetch_optional(pool)
        .await
}

/// Find a user by email with org name for dev email login.
pub async fn find_user_by_email_with_org(
    pool: &DbPool,
    email: &str,
) -> Result<Option<(Uuid, Uuid, String, String, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let q = "SELECT u.id, u.organization_id, u.display_name, u.role, o.name \
           FROM users u JOIN organizations o ON o.id = u.organization_id \
           WHERE u.email = $1 AND u.is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let q = "SELECT u.id, u.organization_id, u.display_name, u.role, o.name \
           FROM users u JOIN organizations o ON o.id = u.organization_id \
           WHERE u.email = $1 AND u.is_active = 1";

    sqlx::query_as(q)
        .bind(email)
        .fetch_optional(pool)
        .await
}

// ============================================================================
// auth/mod.rs login queries
// ============================================================================

/// Find user by email with password hash for local login (auth/mod.rs).
#[cfg(feature = "postgres")]
pub async fn find_user_for_password_login(
    pool: &DbPool,
    email: &str,
) -> Result<Option<(Uuid, Uuid, String, String, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, organization_id, email, role, password_hash \
         FROM users WHERE email = $1 AND is_active = true",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// Find user by email with password hash for local login (SQLite version).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn find_user_for_password_login(
    pool: &DbPool,
    email: &str,
) -> Result<Option<(Uuid, Uuid, String, String, Option<String>)>, sqlx::Error> {
    let r: Option<(String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, organization_id, email, role, password_hash \
         FROM users WHERE email = $1 AND is_active = 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|(id, org_id, email, role, pw)| {
        (
            Uuid::parse_str(&id).unwrap_or_default(),
            Uuid::parse_str(&org_id).unwrap_or_default(),
            email,
            role,
            pw,
        )
    }))
}

// ============================================================================
// API key queries
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: DbUuid,
    pub user_id: DbUuid,
    pub organization_id: DbUuid,
    pub email: String,
    pub role: String,
}

/// Validate an API key (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn find_api_key(
    pool: &DbPool,
    key: &str,
) -> Result<Option<ApiKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyRow>(
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
}

/// Validate an API key (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn find_api_key(
    pool: &DbPool,
    key: &str,
) -> Result<Option<ApiKeyRow>, sqlx::Error> {
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
}

/// Update last_used_at on an API key.
pub async fn update_api_key_last_used(pool: &DbPool, key_id: DbUuid) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE api_keys SET last_used_at = {} WHERE id = $1",
        db::sql::now()
    ))
    .bind(key_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// auth/local.rs queries
// ============================================================================

/// Find user by email with org name for demo login (no password check).
/// Same as find_user_by_email_with_org but kept for clarity.
pub async fn find_active_user_by_email_with_org(
    pool: &DbPool,
    email: &str,
) -> Result<Option<(Uuid, Uuid, String, String, String)>, sqlx::Error> {
    find_user_by_email_with_org(pool, email).await
}

/// Find user by email for local (password) login with org name + password hash.
pub async fn find_user_for_local_login(
    pool: &DbPool,
    email: &str,
) -> Result<Option<(Uuid, Uuid, String, String, String, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT u.id, u.organization_id, u.display_name, u.role, o.name as org_name, u.password_hash
        FROM users u
        JOIN organizations o ON o.id = u.organization_id
        WHERE u.email = $1 AND u.auth_provider = 'local'
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// List active users (email, role) for demo mode user listing.
pub async fn list_active_users(pool: &DbPool) -> Result<Vec<(String, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let q = "SELECT email, role FROM users WHERE is_active = true ORDER BY role";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let q = "SELECT email, role FROM users WHERE is_active = 1 ORDER BY role";

    sqlx::query_as(q).fetch_all(pool).await
}

// ============================================================================
// middleware/auth.rs queries (token revocation)
// ============================================================================

/// Check if a token fingerprint is revoked (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn is_token_revoked(pool: &DbPool, fingerprint: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM revoked_tokens WHERE fingerprint = $1 AND expires_at > now())",
    )
    .bind(fingerprint)
    .fetch_one(pool)
    .await
}

/// Check if a token fingerprint is revoked (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn is_token_revoked(pool: &DbPool, fingerprint: &str) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM revoked_tokens WHERE fingerprint = $1 AND expires_at > datetime('now')",
    )
    .bind(fingerprint)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Revoke a token by fingerprint (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn revoke_token_by_fingerprint(
    pool: &DbPool,
    fingerprint: &str,
    ttl_secs: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO revoked_tokens (fingerprint, expires_at) VALUES ($1, now() + $2 * interval '1 second') ON CONFLICT (fingerprint) DO NOTHING",
    )
    .bind(fingerprint)
    .bind(ttl_secs)
    .execute(pool)
    .await?;
    Ok(())
}

/// Revoke a token by fingerprint (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn revoke_token_by_fingerprint(
    pool: &DbPool,
    fingerprint: &str,
    ttl_secs: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO revoked_tokens (fingerprint, expires_at) VALUES ($1, datetime('now', '+' || $2 || ' seconds'))",
    )
    .bind(fingerprint)
    .bind(ttl_secs)
    .execute(pool)
    .await?;
    Ok(())
}

/// Cleanup expired token revocations (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn cleanup_expired_revocations(pool: &DbPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Cleanup expired token revocations (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn cleanup_expired_revocations(pool: &DbPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < datetime('now')")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// OIDC queries (auth/oidc.rs)
// ============================================================================

/// Find a user by email for OIDC login.
pub async fn find_user_by_email_for_oidc(
    pool: &DbPool,
    email: &str,
) -> Result<Option<(DbUuid, DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, String, String)>(
        "SELECT id, organization_id, email, role FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// Update OIDC subject on a user if not already set.
pub async fn update_oidc_sub_if_null(
    pool: &DbPool,
    user_id: DbUuid,
    oidc_sub: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET oidc_sub = $1 WHERE id = $2 AND oidc_sub IS NULL")
        .bind(oidc_sub)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get the first (default) organization ID.
pub async fn get_default_org_id(pool: &DbPool) -> Result<DbUuid, sqlx::Error> {
    sqlx::query_scalar::<_, DbUuid>("SELECT id FROM organizations LIMIT 1")
        .fetch_one(pool)
        .await
}

/// Create a new OIDC user in the database.
pub async fn create_oidc_user(
    pool: &DbPool,
    user_id: Uuid,
    org_id: DbUuid,
    external_id: &str,
    email: &str,
    display_name: &str,
    oidc_sub: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, oidc_sub)
         VALUES ($1, $2, $3, $4, $5, 'viewer', $6)",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(external_id)
    .bind(email)
    .bind(display_name)
    .bind(oidc_sub)
    .execute(pool)
    .await?;
    Ok(())
}
