//! Query functions for auth domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{self, DbJson, DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Dev auth queries
// ============================================================================

/// Find a user by role for dev login (admin, operator, viewer).
#[allow(clippy::too_many_arguments)]
pub async fn find_user_by_role(
    pool: &DbPool,
    role: &str,
) -> Result<Option<(Uuid, Uuid, String, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let q = "SELECT u.id, u.organization_id, u.email, u.role FROM users u WHERE u.role = $1 AND u.is_active = true LIMIT 1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let q = "SELECT u.id, u.organization_id, u.email, u.role FROM users u WHERE u.role = $1 AND u.is_active = 1 LIMIT 1";

    sqlx::query_as(q).bind(role).fetch_optional(pool).await
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

    sqlx::query_as(q).bind(email).fetch_optional(pool).await
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
pub async fn find_api_key(pool: &DbPool, key: &str) -> Result<Option<ApiKeyRow>, sqlx::Error> {
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
pub async fn find_api_key(pool: &DbPool, key: &str) -> Result<Option<ApiKeyRow>, sqlx::Error> {
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
        .bind(crate::db::bind_id(user_id))
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
    .bind(crate::db::bind_id(user_id))
    .bind(crate::db::bind_id(org_id))
    .bind(external_id)
    .bind(email)
    .bind(display_name)
    .bind(oidc_sub)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// SAML queries (auth/saml.rs)
// ============================================================================

/// Row type for SAML group mappings.
#[derive(Debug, sqlx::FromRow)]
pub struct SamlGroupMapping {
    pub id: DbUuid,
    pub saml_group: String,
    pub team_id: DbUuid,
    pub default_role: String,
}

/// Get all SAML group mappings.
pub async fn get_all_saml_group_mappings(
    pool: &DbPool,
) -> Result<Vec<SamlGroupMapping>, sqlx::Error> {
    sqlx::query_as::<_, SamlGroupMapping>(
        "SELECT id, saml_group, team_id, default_role FROM saml_group_mappings",
    )
    .fetch_all(pool)
    .await
}

/// Check if a user is a member of a team.
pub async fn is_team_member(
    pool: &DbPool,
    team_id: DbUuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let count: i32 =
        sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(crate::db::bind_id(team_id))
            .bind(crate::db::bind_id(user_id))
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// Add a user to a team (idempotent).
pub async fn add_team_member(
    pool: &DbPool,
    team_id: DbUuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(crate::db::bind_id(team_id))
    .bind(crate::db::bind_id(user_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a team name by ID.
pub async fn get_team_name(pool: &DbPool, team_id: DbUuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT name FROM teams WHERE id = $1")
        .bind(crate::db::bind_id(team_id))
        .fetch_optional(pool)
        .await
}

/// Remove a user from a team.
pub async fn remove_team_member(
    pool: &DbPool,
    team_id: DbUuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(crate::db::bind_id(team_id))
        .bind(crate::db::bind_id(user_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Find or update a SAML user.
pub async fn update_saml_user(
    pool: &DbPool,
    user_id: DbUuid,
    name_id: &str,
    role: &str,
    display_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET saml_name_id = $1, role = $2, display_name = $3 WHERE id = $4")
        .bind(name_id)
        .bind(role)
        .bind(display_name)
        .bind(crate::db::bind_id(user_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Create a SAML user.
pub async fn create_saml_user(
    pool: &DbPool,
    user_id: Uuid,
    org_id: DbUuid,
    external_id: &str,
    email: &str,
    display_name: &str,
    role: &str,
    name_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, saml_name_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(crate::db::bind_id(user_id))
    .bind(crate::db::bind_id(org_id))
    .bind(external_id)
    .bind(email)
    .bind(display_name)
    .bind(role)
    .bind(name_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// List SAML group mappings with team names.
pub async fn list_saml_group_mappings_with_names(
    pool: &DbPool,
) -> Result<Vec<(DbUuid, String, DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, DbUuid, String, String)>(
        r#"SELECT sgm.id, sgm.saml_group, sgm.team_id, t.name, sgm.default_role
           FROM saml_group_mappings sgm
           JOIN teams t ON t.id = sgm.team_id
           ORDER BY sgm.saml_group"#,
    )
    .fetch_all(pool)
    .await
}

/// Create a SAML group mapping.
pub async fn create_saml_group_mapping(
    pool: &DbPool,
    id: Uuid,
    saml_group: &str,
    team_id: DbUuid,
    default_role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO saml_group_mappings (id, saml_group, team_id, default_role)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(id))
    .bind(saml_group)
    .bind(crate::db::bind_id(team_id))
    .bind(default_role)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a SAML group mapping.
pub async fn delete_saml_group_mapping(pool: &DbPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM saml_group_mappings WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .execute(pool)
        .await?;
    Ok(())
}
