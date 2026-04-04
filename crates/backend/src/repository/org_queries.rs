//! Query functions for organizations domain.

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid};
use uuid::Uuid;

#[cfg(feature = "postgres")]
type DbAcquire = sqlx::PgPool;

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
type DbAcquire = sqlx::SqlitePool;

/// Organization row returned from queries.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct OrgRow {
    pub id: DbUuid,
    pub name: String,
    pub slug: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Fetch the platform_role for a user from the database.
pub async fn get_platform_role(db: &DbPool, user_id: DbUuid) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT platform_role FROM users WHERE id = $1")
        .bind(crate::db::bind_id(user_id))
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten()
}

/// List all organizations.
pub async fn list_organizations(pool: &DbPool) -> Result<Vec<OrgRow>, sqlx::Error> {
    sqlx::query_as::<_, OrgRow>(
        "SELECT id, name, slug, created_at, updated_at FROM organizations ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

/// Get a single organization by ID.
pub async fn get_organization(pool: &DbPool, id: Uuid) -> Result<Option<OrgRow>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, OrgRow>(
        "SELECT id, name, slug, created_at, updated_at FROM organizations WHERE id = $1",
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, OrgRow>(
        "SELECT id, name, slug, created_at, updated_at FROM organizations WHERE id = $1",
    )
    .bind(DbUuid::from(id))
    .fetch_optional(pool)
    .await;
}

/// Update an organization's name.
pub async fn update_organization(
    pool: &DbPool,
    id: Uuid,
    name: &Option<String>,
) -> Result<Option<OrgRow>, sqlx::Error> {
    let update_org_sql = format!(
        "UPDATE organizations SET
                 name = COALESCE($2, name),
                 updated_at = {}
             WHERE id = $1
             RETURNING id, name, slug, created_at, updated_at",
        crate::db::sql::now()
    );

    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, OrgRow>(&update_org_sql)
        .bind(crate::db::bind_id(id))
        .bind(name)
        .fetch_optional(pool)
        .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, OrgRow>(&update_org_sql)
        .bind(DbUuid::from(id))
        .bind(name)
        .fetch_optional(pool)
        .await;
}

/// Result of creating an organization with its initial admin user.
pub struct CreateOrgResult {
    pub org: OrgRow,
    pub admin_id: DbUuid,
}

/// Create an organization with its initial admin user in a single transaction.
/// Also auto-initializes PKI for the new org.
pub async fn create_organization_with_admin(
    pool: &DbPool,
    name: &str,
    slug: &str,
    admin_email: &str,
    admin_display_name: &str,
) -> Result<CreateOrgResult, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let org = sqlx::query_as::<_, OrgRow>(
        r#"INSERT INTO organizations (id, name, slug)
           VALUES ($1, $2, $3)
           RETURNING id, name, slug, created_at, updated_at"#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(name)
    .bind(slug)
    .fetch_one(&mut *tx)
    .await?;

    let admin_id = sqlx::query_scalar::<_, DbUuid>(
        r#"INSERT INTO users (id, organization_id, external_id, email, display_name, role, auth_provider)
           VALUES ($1, $2, $3, $4, $5, 'admin', 'local')
           RETURNING id"#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(org.id)
    .bind(format!("local-admin-{}", org.slug))
    .bind(admin_email)
    .bind(admin_display_name)
    .fetch_one(&mut *tx)
    .await?;

    // Auto-initialize PKI for the new org
    match appcontrol_common::generate_ca(name, 3650) {
        Ok(ca) => {
            sqlx::query("UPDATE organizations SET ca_cert_pem = $2, ca_key_pem = $3 WHERE id = $1")
                .bind(org.id)
                .bind(&ca.cert_pem)
                .bind(&ca.key_pem)
                .execute(&mut *tx)
                .await?;
        }
        Err(e) => {
            tracing::warn!(org = %name, "Failed to auto-generate CA during org creation: {}", e);
        }
    }

    tx.commit().await?;

    Ok(CreateOrgResult { org, admin_id })
}
