//! Application repository — all app-related database queries.
//!
//! The handler calls `repo.list_apps(org_id, ...)` instead of writing SQL.
//! The repository handles all database-specific encoding (DbUuid, DbJson, etc.).

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbJson, DbPool, DbUuid};

// ============================================================================
// Domain types (database-agnostic)
// ============================================================================

/// Application with component state counts (for list view).
#[derive(Debug)]
pub struct AppSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Uuid,
    pub site_id: Uuid,
    pub tags: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub component_count: i64,
    pub running_count: i64,
    pub starting_count: i64,
    pub stopping_count: i64,
    pub stopped_count: i64,
    pub failed_count: i64,
    pub unreachable_count: i64,
}

/// Full application row.
#[derive(Debug)]
pub struct App {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Uuid,
    pub site_id: Uuid,
    pub tags: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Parameters for listing apps.
#[derive(Debug, Default)]
pub struct ListAppsParams {
    pub organization_id: Uuid,
    pub search: Option<String>,
    pub site_id: Option<Uuid>,
    pub limit: i64,
    pub offset: i64,
}

/// Parameters for creating an app.
#[derive(Debug)]
pub struct CreateApp {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Uuid,
    pub site_id: Uuid,
    pub tags: Value,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait AppRepository: Send + Sync {
    /// List applications with component state counts.
    async fn list_apps(&self, params: ListAppsParams) -> Result<Vec<AppSummary>, sqlx::Error>;

    /// Get a single application by ID.
    async fn get_app(&self, id: Uuid, org_id: Uuid) -> Result<Option<App>, sqlx::Error>;

    /// Create a new application.
    async fn create_app(&self, app: CreateApp) -> Result<App, sqlx::Error>;

    /// Update an application.
    async fn update_app(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&Value>,
        site_id: Option<Uuid>,
    ) -> Result<Option<App>, sqlx::Error>;

    /// Delete an application.
    async fn delete_app(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error>;

    /// Find default site for an organization.
    async fn find_default_site(&self, org_id: Uuid) -> Result<Option<Uuid>, sqlx::Error>;

    /// Create a default site for an organization.
    async fn create_default_site(&self, id: Uuid, org_id: Uuid) -> Result<(), sqlx::Error>;

    /// Grant owner permission to a user on an app.
    async fn grant_owner_permission(&self, app_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgAppRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgAppRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgAppSummaryRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    organization_id: Uuid,
    site_id: Uuid,
    tags: Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    component_count: Option<i64>,
    running_count: Option<i64>,
    starting_count: Option<i64>,
    stopping_count: Option<i64>,
    stopped_count: Option<i64>,
    failed_count: Option<i64>,
    unreachable_count: Option<i64>,
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgAppRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    organization_id: Uuid,
    site_id: Uuid,
    tags: Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
#[async_trait]
impl AppRepository for PgAppRepository {
    async fn list_apps(&self, params: ListAppsParams) -> Result<Vec<AppSummary>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgAppSummaryRow>(
            r#"SELECT
                a.id, a.name, a.description, a.organization_id, a.site_id, a.tags,
                a.created_at, a.updated_at,
                COUNT(c.id) as component_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'RUNNING') as running_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'STARTING') as starting_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPING') as stopping_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPED') as stopped_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'FAILED') as failed_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'UNREACHABLE') as unreachable_count
            FROM applications a
            LEFT JOIN components c ON c.application_id = a.id
            WHERE a.organization_id = $1
              AND ($2::text IS NULL OR a.name ILIKE '%' || $2 || '%')
              AND ($3::uuid IS NULL OR a.site_id = $3)
            GROUP BY a.id
            ORDER BY a.name
            LIMIT $4 OFFSET $5"#,
        )
        .bind(params.organization_id)
        .bind(&params.search)
        .bind(params.site_id)
        .bind(params.limit)
        .bind(params.offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AppSummary {
                id: r.id,
                name: r.name,
                description: r.description,
                organization_id: r.organization_id,
                site_id: r.site_id,
                tags: r.tags,
                created_at: r.created_at,
                updated_at: r.updated_at,
                component_count: r.component_count.unwrap_or(0),
                running_count: r.running_count.unwrap_or(0),
                starting_count: r.starting_count.unwrap_or(0),
                stopping_count: r.stopping_count.unwrap_or(0),
                stopped_count: r.stopped_count.unwrap_or(0),
                failed_count: r.failed_count.unwrap_or(0),
                unreachable_count: r.unreachable_count.unwrap_or(0),
            })
            .collect())
    }

    async fn get_app(&self, id: Uuid, org_id: Uuid) -> Result<Option<App>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgAppRow>(
            "SELECT id, name, description, organization_id, site_id, tags, created_at, updated_at \
             FROM applications WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| App {
            id: r.id,
            name: r.name,
            description: r.description,
            organization_id: r.organization_id,
            site_id: r.site_id,
            tags: r.tags,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn create_app(&self, app: CreateApp) -> Result<App, sqlx::Error> {
        let row = sqlx::query_as::<_, PgAppRow>(
            "INSERT INTO applications (id, name, description, organization_id, site_id, tags) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, name, description, organization_id, site_id, tags, created_at, updated_at",
        )
        .bind(app.id)
        .bind(&app.name)
        .bind(&app.description)
        .bind(app.organization_id)
        .bind(app.site_id)
        .bind(&app.tags)
        .fetch_one(&self.pool)
        .await?;

        Ok(App {
            id: row.id,
            name: row.name,
            description: row.description,
            organization_id: row.organization_id,
            site_id: row.site_id,
            tags: row.tags,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn update_app(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&Value>,
        site_id: Option<Uuid>,
    ) -> Result<Option<App>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgAppRow>(
            "UPDATE applications SET \
                name = COALESCE($3, name), \
                description = COALESCE($4, description), \
                tags = COALESCE($5, tags), \
                site_id = COALESCE($6, site_id), \
                updated_at = now() \
             WHERE id = $1 AND organization_id = $2 \
             RETURNING id, name, description, organization_id, site_id, tags, created_at, updated_at",
        )
        .bind(id)
        .bind(org_id)
        .bind(name)
        .bind(description)
        .bind(tags)
        .bind(site_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| App {
            id: r.id,
            name: r.name,
            description: r.description,
            organization_id: r.organization_id,
            site_id: r.site_id,
            tags: r.tags,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn delete_app(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM applications WHERE id = $1 AND organization_id = $2")
            .bind(id)
            .bind(org_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_default_site(&self, org_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM sites WHERE organization_id = $1 AND is_active = true \
             ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
        )
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id,)| id))
    }

    async fn create_default_site(&self, id: Uuid, org_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code, site_type) \
             VALUES ($1, $2, 'Default Site', 'DEFAULT', 'primary')",
        )
        .bind(id)
        .bind(org_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn grant_owner_permission(&self, app_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) \
             VALUES ($1, $2, 'owner', $2)",
        )
        .bind(app_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteAppRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteAppRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteAppSummaryRow {
    id: DbUuid,
    name: String,
    description: Option<String>,
    organization_id: DbUuid,
    site_id: DbUuid,
    tags: DbJson,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    component_count: Option<i64>,
    running_count: Option<i64>,
    starting_count: Option<i64>,
    stopping_count: Option<i64>,
    stopped_count: Option<i64>,
    failed_count: Option<i64>,
    unreachable_count: Option<i64>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteAppRow {
    id: DbUuid,
    name: String,
    description: Option<String>,
    organization_id: DbUuid,
    site_id: DbUuid,
    tags: DbJson,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl AppRepository for SqliteAppRepository {
    async fn list_apps(&self, params: ListAppsParams) -> Result<Vec<AppSummary>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteAppSummaryRow>(
            r#"SELECT
                a.id, a.name, a.description, a.organization_id, a.site_id, a.tags,
                a.created_at, a.updated_at,
                COUNT(c.id) as component_count,
                SUM(CASE WHEN c.current_state = 'RUNNING' THEN 1 ELSE 0 END) as running_count,
                SUM(CASE WHEN c.current_state = 'STARTING' THEN 1 ELSE 0 END) as starting_count,
                SUM(CASE WHEN c.current_state = 'STOPPING' THEN 1 ELSE 0 END) as stopping_count,
                SUM(CASE WHEN c.current_state = 'STOPPED' THEN 1 ELSE 0 END) as stopped_count,
                SUM(CASE WHEN c.current_state = 'FAILED' THEN 1 ELSE 0 END) as failed_count,
                SUM(CASE WHEN c.current_state = 'UNREACHABLE' THEN 1 ELSE 0 END) as unreachable_count
            FROM applications a
            LEFT JOIN components c ON c.application_id = a.id
            WHERE a.organization_id = $1
              AND ($2 IS NULL OR a.name LIKE '%' || $2 || '%')
              AND ($3 IS NULL OR a.site_id = $3)
            GROUP BY a.id
            ORDER BY a.name
            LIMIT $4 OFFSET $5"#,
        )
        .bind(DbUuid::from(params.organization_id))
        .bind(&params.search)
        .bind(params.site_id.map(DbUuid::from))
        .bind(params.limit)
        .bind(params.offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AppSummary {
                id: r.id.into_inner(),
                name: r.name,
                description: r.description,
                organization_id: r.organization_id.into_inner(),
                site_id: r.site_id.into_inner(),
                tags: r.tags.into(),
                created_at: r.created_at,
                updated_at: r.updated_at,
                component_count: r.component_count.unwrap_or(0),
                running_count: r.running_count.unwrap_or(0),
                starting_count: r.starting_count.unwrap_or(0),
                stopping_count: r.stopping_count.unwrap_or(0),
                stopped_count: r.stopped_count.unwrap_or(0),
                failed_count: r.failed_count.unwrap_or(0),
                unreachable_count: r.unreachable_count.unwrap_or(0),
            })
            .collect())
    }

    async fn get_app(&self, id: Uuid, org_id: Uuid) -> Result<Option<App>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteAppRow>(
            "SELECT id, name, description, organization_id, site_id, tags, created_at, updated_at \
             FROM applications WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| App {
            id: r.id.into_inner(),
            name: r.name,
            description: r.description,
            organization_id: r.organization_id.into_inner(),
            site_id: r.site_id.into_inner(),
            tags: r.tags.into(),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn create_app(&self, app: CreateApp) -> Result<App, sqlx::Error> {
        let tags_str = serde_json::to_string(&app.tags).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(
            "INSERT INTO applications (id, name, description, organization_id, site_id, tags) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(DbUuid::from(app.id))
        .bind(&app.name)
        .bind(&app.description)
        .bind(DbUuid::from(app.organization_id))
        .bind(DbUuid::from(app.site_id))
        .bind(&tags_str)
        .execute(&self.pool)
        .await?;

        // SQLite doesn't support RETURNING — fetch the inserted row
        self.get_app(app.id, app.organization_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    async fn update_app(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<&Value>,
        site_id: Option<Uuid>,
    ) -> Result<Option<App>, sqlx::Error> {
        let tags_str = tags.map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
        sqlx::query(&format!(
            "UPDATE applications SET \
                name = COALESCE($3, name), \
                description = COALESCE($4, description), \
                tags = COALESCE($5, tags), \
                site_id = COALESCE($6, site_id), \
                updated_at = {} \
             WHERE id = $1 AND organization_id = $2",
            crate::db::sql::now()
        ))
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(description)
        .bind(tags_str.as_deref())
        .bind(site_id.map(DbUuid::from))
        .execute(&self.pool)
        .await?;

        self.get_app(id, org_id).await
    }

    async fn delete_app(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM applications WHERE id = $1 AND organization_id = $2")
            .bind(DbUuid::from(id))
            .bind(DbUuid::from(org_id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_default_site(&self, org_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        let row: Option<(DbUuid,)> = sqlx::query_as(
            "SELECT id FROM sites WHERE organization_id = $1 AND is_active = 1 \
             ORDER BY CASE site_type WHEN 'primary' THEN 0 ELSE 1 END, created_at LIMIT 1",
        )
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id,)| id.into_inner()))
    }

    async fn create_default_site(&self, id: Uuid, org_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code, site_type) \
             VALUES ($1, $2, 'Default Site', 'DEFAULT', 'primary')",
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn grant_owner_permission(&self, app_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by) \
             VALUES ($1, $2, 'owner', $2)",
        )
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(user_id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// ============================================================================
// Factory function
// ============================================================================

/// Create the appropriate AppRepository for the current database backend.
pub fn create_app_repository(pool: DbPool) -> Box<dyn AppRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgAppRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteAppRepository::new(pool))
    }
}
