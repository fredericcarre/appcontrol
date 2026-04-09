//! Hosting repository — all hosting-related database queries.
//! A hosting is a logical grouping of sites (e.g., a datacenter or cloud region).

use async_trait::async_trait;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct Hosting {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait HostingRepository: Send + Sync {
    /// List hostings for an organization.
    async fn list_hostings(&self, org_id: Uuid) -> Result<Vec<Hosting>, sqlx::Error>;

    /// Get a single hosting by ID.
    async fn get_hosting(&self, id: Uuid, org_id: Uuid) -> Result<Option<Hosting>, sqlx::Error>;

    /// Create a new hosting.
    async fn create_hosting(
        &self,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Hosting, sqlx::Error>;

    /// Update a hosting.
    async fn update_hosting(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<Hosting>, sqlx::Error>;

    /// Delete a hosting. Returns true if deleted.
    async fn delete_hosting(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error>;

    /// Count sites linked to a hosting.
    async fn count_sites_in_hosting(&self, hosting_id: Uuid) -> Result<i64, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgHostingRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgHostingRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgHostingRow {
    id: Uuid,
    organization_id: Uuid,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
impl From<PgHostingRow> for Hosting {
    fn from(r: PgHostingRow) -> Self {
        Hosting {
            id: r.id,
            organization_id: r.organization_id,
            name: r.name,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl HostingRepository for PgHostingRepository {
    async fn list_hostings(&self, org_id: Uuid) -> Result<Vec<Hosting>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgHostingRow>(
            r#"SELECT id, organization_id, name, description, created_at, updated_at
               FROM hostings
               WHERE organization_id = $1
               ORDER BY name"#,
        )
        .bind(crate::db::bind_id(org_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_hosting(&self, id: Uuid, org_id: Uuid) -> Result<Option<Hosting>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgHostingRow>(
            r#"SELECT id, organization_id, name, description, created_at, updated_at
               FROM hostings WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn create_hosting(
        &self,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Hosting, sqlx::Error> {
        let row = sqlx::query_as::<_, PgHostingRow>(
            r#"INSERT INTO hostings (organization_id, name, description)
               VALUES ($1, $2, $3)
               RETURNING id, organization_id, name, description, created_at, updated_at"#,
        )
        .bind(crate::db::bind_id(org_id))
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    async fn update_hosting(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<Hosting>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgHostingRow>(
            r#"UPDATE hostings SET
                   name = COALESCE($3, name),
                   description = COALESCE($4, description),
                   updated_at = now()
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, description, created_at, updated_at"#,
        )
        .bind(id)
        .bind(crate::db::bind_id(org_id))
        .bind(name)
        .bind(description)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_hosting(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM hostings WHERE id = $1 AND organization_id = $2")
                .bind(id)
                .bind(crate::db::bind_id(org_id))
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_sites_in_hosting(&self, hosting_id: Uuid) -> Result<i64, sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sites WHERE hosting_id = $1")
                .bind(crate::db::bind_id(hosting_id))
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteHostingRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteHostingRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteHostingRow {
    id: DbUuid,
    organization_id: DbUuid,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl From<SqliteHostingRow> for Hosting {
    fn from(r: SqliteHostingRow) -> Self {
        Hosting {
            id: r.id.into_inner(),
            organization_id: r.organization_id.into_inner(),
            name: r.name,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl HostingRepository for SqliteHostingRepository {
    async fn list_hostings(&self, org_id: Uuid) -> Result<Vec<Hosting>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteHostingRow>(
            r#"SELECT id, organization_id, name, description, created_at, updated_at
               FROM hostings
               WHERE organization_id = $1
               ORDER BY name"#,
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_hosting(&self, id: Uuid, org_id: Uuid) -> Result<Option<Hosting>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteHostingRow>(
            r#"SELECT id, organization_id, name, description, created_at, updated_at
               FROM hostings WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn create_hosting(
        &self,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Hosting, sqlx::Error> {
        let new_id = DbUuid::new_v4();
        let row = sqlx::query_as::<_, SqliteHostingRow>(
            r#"INSERT INTO hostings (id, organization_id, name, description)
               VALUES ($1, $2, $3, $4)
               RETURNING id, organization_id, name, description, created_at, updated_at"#,
        )
        .bind(new_id)
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    async fn update_hosting(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<Hosting>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteHostingRow>(
            r#"UPDATE hostings SET
                   name = COALESCE($3, name),
                   description = COALESCE($4, description),
                   updated_at = datetime('now')
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, description, created_at, updated_at"#,
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(description)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_hosting(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM hostings WHERE id = $1 AND organization_id = $2")
                .bind(DbUuid::from(id))
                .bind(DbUuid::from(org_id))
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_sites_in_hosting(&self, hosting_id: Uuid) -> Result<i64, sqlx::Error> {
        let count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sites WHERE hosting_id = $1")
                .bind(DbUuid::from(hosting_id))
                .fetch_one(&self.pool)
                .await?;
        Ok(count as i64)
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_hosting_repository(pool: DbPool) -> Box<dyn HostingRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgHostingRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteHostingRepository::new(pool))
    }
}
