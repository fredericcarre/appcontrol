//! Enrollment repository — enrollment token management.

use async_trait::async_trait;
use uuid::Uuid;

use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct EnrollmentToken {
    pub id: Uuid,
    pub token_prefix: String,
    pub name: String,
    pub max_uses: Option<i32>,
    pub current_uses: i32,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scope: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait EnrollmentRepository: Send + Sync {
    /// Create an enrollment token. Returns the ID.
    async fn create_token(
        &self,
        org_id: Uuid,
        token_hash: &str,
        token_prefix: &str,
        name: &str,
        max_uses: Option<i32>,
        expires_at: chrono::DateTime<chrono::Utc>,
        scope: &str,
        created_by: Uuid,
    ) -> Result<Uuid, sqlx::Error>;

    /// List enrollment tokens for an organization (excludes revoked > 24h).
    async fn list_tokens(&self, org_id: Uuid) -> Result<Vec<EnrollmentToken>, sqlx::Error>;

    /// Revoke an enrollment token. Returns true if revoked.
    async fn revoke_token(
        &self,
        id: Uuid,
        org_id: Uuid,
        revoked_by: Uuid,
    ) -> Result<bool, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgEnrollmentRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgEnrollmentRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgEnrollmentTokenRow {
    id: Uuid,
    token_prefix: String,
    name: String,
    max_uses: Option<i32>,
    current_uses: i32,
    expires_at: chrono::DateTime<chrono::Utc>,
    scope: String,
    created_at: chrono::DateTime<chrono::Utc>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(feature = "postgres")]
#[async_trait]
impl EnrollmentRepository for PgEnrollmentRepository {
    async fn create_token(
        &self,
        org_id: Uuid,
        token_hash: &str,
        token_prefix: &str,
        name: &str,
        max_uses: Option<i32>,
        expires_at: chrono::DateTime<chrono::Utc>,
        scope: &str,
        created_by: Uuid,
    ) -> Result<Uuid, sqlx::Error> {
        let id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO enrollment_tokens \
             (organization_id, token_hash, token_prefix, name, max_uses, expires_at, scope, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             RETURNING id",
        )
        .bind(org_id)
        .bind(token_hash)
        .bind(token_prefix)
        .bind(name)
        .bind(max_uses)
        .bind(expires_at)
        .bind(scope)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn list_tokens(&self, org_id: Uuid) -> Result<Vec<EnrollmentToken>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgEnrollmentTokenRow>(
            &format!(
                "SELECT id, token_prefix, name, max_uses, current_uses, expires_at, scope, created_at, revoked_at \
                 FROM enrollment_tokens \
                 WHERE organization_id = $1 \
                   AND (revoked_at IS NULL OR revoked_at > {} - interval '24 hours') \
                 ORDER BY created_at DESC",
                crate::db::sql::now()
            ),
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EnrollmentToken {
                id: r.id,
                token_prefix: r.token_prefix,
                name: r.name,
                max_uses: r.max_uses,
                current_uses: r.current_uses,
                expires_at: r.expires_at,
                scope: r.scope,
                created_at: r.created_at,
                revoked_at: r.revoked_at,
            })
            .collect())
    }

    async fn revoke_token(
        &self,
        id: Uuid,
        org_id: Uuid,
        revoked_by: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(&format!(
            "UPDATE enrollment_tokens SET revoked_at = {}, revoked_by = $3 \
             WHERE id = $1 AND organization_id = $2 AND revoked_at IS NULL",
            crate::db::sql::now()
        ))
        .bind(id)
        .bind(org_id)
        .bind(revoked_by)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteEnrollmentRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteEnrollmentRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteEnrollmentTokenRow {
    id: DbUuid,
    token_prefix: String,
    name: String,
    max_uses: Option<i32>,
    current_uses: i32,
    expires_at: chrono::DateTime<chrono::Utc>,
    scope: String,
    created_at: chrono::DateTime<chrono::Utc>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl EnrollmentRepository for SqliteEnrollmentRepository {
    async fn create_token(
        &self,
        org_id: Uuid,
        token_hash: &str,
        token_prefix: &str,
        name: &str,
        max_uses: Option<i32>,
        expires_at: chrono::DateTime<chrono::Utc>,
        scope: &str,
        created_by: Uuid,
    ) -> Result<Uuid, sqlx::Error> {
        let new_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO enrollment_tokens \
             (id, organization_id, token_hash, token_prefix, name, max_uses, expires_at, scope, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(DbUuid::from(new_id))
        .bind(DbUuid::from(org_id))
        .bind(token_hash)
        .bind(token_prefix)
        .bind(name)
        .bind(max_uses)
        .bind(expires_at.to_rfc3339())
        .bind(scope)
        .bind(DbUuid::from(created_by))
        .execute(&self.pool)
        .await?;
        Ok(new_id)
    }

    async fn list_tokens(&self, org_id: Uuid) -> Result<Vec<EnrollmentToken>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteEnrollmentTokenRow>(
            &format!(
                "SELECT id, token_prefix, name, max_uses, current_uses, expires_at, scope, created_at, revoked_at \
                 FROM enrollment_tokens \
                 WHERE organization_id = $1 \
                   AND (revoked_at IS NULL OR revoked_at > datetime({}, '-24 hours')) \
                 ORDER BY created_at DESC",
                crate::db::sql::now()
            ),
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EnrollmentToken {
                id: r.id.into_inner(),
                token_prefix: r.token_prefix,
                name: r.name,
                max_uses: r.max_uses,
                current_uses: r.current_uses,
                expires_at: r.expires_at,
                scope: r.scope,
                created_at: r.created_at,
                revoked_at: r.revoked_at,
            })
            .collect())
    }

    async fn revoke_token(
        &self,
        id: Uuid,
        org_id: Uuid,
        revoked_by: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(&format!(
            "UPDATE enrollment_tokens SET revoked_at = {}, revoked_by = $3 \
             WHERE id = $1 AND organization_id = $2 AND revoked_at IS NULL",
            crate::db::sql::now()
        ))
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(DbUuid::from(revoked_by))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_enrollment_repository(pool: DbPool) -> Box<dyn EnrollmentRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgEnrollmentRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteEnrollmentRepository::new(pool))
    }
}
