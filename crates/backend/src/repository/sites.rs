//! Site repository — all site-related database queries.

use async_trait::async_trait;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct Site {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub code: String,
    pub site_type: String,
    pub location: Option<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait SiteRepository: Send + Sync {
    /// List sites for an organization with optional filters.
    async fn list_sites(
        &self,
        org_id: Uuid,
        site_type: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<Site>, sqlx::Error>;

    /// Get a single site by ID.
    async fn get_site(&self, id: Uuid, org_id: Uuid) -> Result<Option<Site>, sqlx::Error>;

    /// Create a new site.
    async fn create_site(
        &self,
        org_id: Uuid,
        name: &str,
        code: &str,
        site_type: &str,
        location: Option<&str>,
    ) -> Result<Site, sqlx::Error>;

    /// Update a site.
    async fn update_site(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        location: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Option<Site>, sqlx::Error>;

    /// Delete a site. Returns true if deleted.
    async fn delete_site(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error>;

    /// Count applications linked to a site.
    async fn count_apps_in_site(&self, site_id: Uuid) -> Result<i64, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgSiteRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgSiteRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgSiteRow {
    id: Uuid,
    organization_id: Uuid,
    name: String,
    code: String,
    site_type: String,
    location: Option<String>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
impl From<PgSiteRow> for Site {
    fn from(r: PgSiteRow) -> Self {
        Site {
            id: r.id,
            organization_id: r.organization_id,
            name: r.name,
            code: r.code,
            site_type: r.site_type,
            location: r.location,
            is_active: r.is_active,
            created_at: r.created_at,
        }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl SiteRepository for PgSiteRepository {
    async fn list_sites(
        &self,
        org_id: Uuid,
        site_type: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<Site>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgSiteRow>(
            r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
               FROM sites
               WHERE organization_id = $1
                 AND ($2::text IS NULL OR site_type = $2)
                 AND ($3::bool IS NULL OR is_active = $3)
               ORDER BY code"#,
        )
        .bind(org_id)
        .bind(site_type)
        .bind(is_active)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_site(&self, id: Uuid, org_id: Uuid) -> Result<Option<Site>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgSiteRow>(
            r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
               FROM sites WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn create_site(
        &self,
        org_id: Uuid,
        name: &str,
        code: &str,
        site_type: &str,
        location: Option<&str>,
    ) -> Result<Site, sqlx::Error> {
        let row = sqlx::query_as::<_, PgSiteRow>(
            r#"INSERT INTO sites (organization_id, name, code, site_type, location)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
        )
        .bind(org_id)
        .bind(name)
        .bind(code)
        .bind(site_type)
        .bind(location)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    async fn update_site(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        location: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Option<Site>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgSiteRow>(
            r#"UPDATE sites SET
                   name = COALESCE($3, name),
                   location = COALESCE($4, location),
                   is_active = COALESCE($5, is_active)
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
        )
        .bind(id)
        .bind(org_id)
        .bind(name)
        .bind(location)
        .bind(is_active)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_site(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM sites WHERE id = $1 AND organization_id = $2")
            .bind(id)
            .bind(org_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_apps_in_site(&self, site_id: Uuid) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE site_id = $1")
            .bind(site_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteSiteRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteSiteRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteSiteRow {
    id: DbUuid,
    organization_id: DbUuid,
    name: String,
    code: String,
    site_type: String,
    location: Option<String>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl From<SqliteSiteRow> for Site {
    fn from(r: SqliteSiteRow) -> Self {
        Site {
            id: r.id.into_inner(),
            organization_id: r.organization_id.into_inner(),
            name: r.name,
            code: r.code,
            site_type: r.site_type,
            location: r.location,
            is_active: r.is_active,
            created_at: r.created_at,
        }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl SiteRepository for SqliteSiteRepository {
    async fn list_sites(
        &self,
        org_id: Uuid,
        site_type: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<Site>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteSiteRow>(
            r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
               FROM sites
               WHERE organization_id = $1
                 AND ($2 IS NULL OR site_type = $2)
                 AND ($3 IS NULL OR is_active = $3)
               ORDER BY code"#,
        )
        .bind(DbUuid::from(org_id))
        .bind(site_type)
        .bind(is_active)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_site(&self, id: Uuid, org_id: Uuid) -> Result<Option<Site>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteSiteRow>(
            r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
               FROM sites WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn create_site(
        &self,
        org_id: Uuid,
        name: &str,
        code: &str,
        site_type: &str,
        location: Option<&str>,
    ) -> Result<Site, sqlx::Error> {
        let new_id = DbUuid::new_v4();
        let row = sqlx::query_as::<_, SqliteSiteRow>(
            r#"INSERT INTO sites (id, organization_id, name, code, site_type, location)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
        )
        .bind(new_id)
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(code)
        .bind(site_type)
        .bind(location)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    async fn update_site(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        location: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Option<Site>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteSiteRow>(
            r#"UPDATE sites SET
                   name = COALESCE($3, name),
                   location = COALESCE($4, location),
                   is_active = COALESCE($5, is_active)
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(location)
        .bind(is_active)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_site(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM sites WHERE id = $1 AND organization_id = $2")
            .bind(DbUuid::from(id))
            .bind(DbUuid::from(org_id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_apps_in_site(&self, site_id: Uuid) -> Result<i64, sqlx::Error> {
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE site_id = $1")
            .bind(DbUuid::from(site_id))
            .fetch_one(&self.pool)
            .await?;
        Ok(count as i64)
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_site_repository(pool: DbPool) -> Box<dyn SiteRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgSiteRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteSiteRepository::new(pool))
    }
}
