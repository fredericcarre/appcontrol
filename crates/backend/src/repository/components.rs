//! Component repository — all component-related database queries.

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

/// Core component data returned from CRUD operations.
#[derive(Debug)]
pub struct Component {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Value>,
    pub referenced_app_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Dependency between two components.
#[derive(Debug)]
pub struct Dependency {
    pub id: Uuid,
    pub application_id: Uuid,
    pub from_component_id: Uuid,
    pub to_component_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Parameters for creating a component.
#[derive(Debug)]
pub struct CreateComponent {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: String,
    pub group_id: Option<Uuid>,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub position_x: f32,
    pub position_y: f32,
    pub env_vars: Value,
    pub tags: Value,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Value>,
    pub referenced_app_id: Option<Uuid>,
}

/// Parameters for updating a component.
#[derive(Debug)]
pub struct UpdateComponent {
    pub name: Option<String>,
    pub component_type: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: Option<i32>,
    pub start_timeout_seconds: Option<i32>,
    pub stop_timeout_seconds: Option<i32>,
    pub is_optional: Option<bool>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Value>,
    pub referenced_app_id: Option<Uuid>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait ComponentRepository: Send + Sync {
    /// List components for an application.
    async fn list_components(&self, app_id: Uuid) -> Result<Vec<Component>, sqlx::Error>;

    /// Get a component by ID with organization check.
    async fn get_component(&self, id: Uuid, org_id: Uuid) -> Result<Option<Component>, sqlx::Error>;

    /// Get the application_id for a component.
    async fn get_component_app_id(&self, id: Uuid) -> Result<Option<Uuid>, sqlx::Error>;

    /// Get application_id and agent_id for a component.
    async fn get_component_app_and_agent(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, sqlx::Error>;

    /// Create a new component.
    async fn create_component(&self, comp: CreateComponent) -> Result<Component, sqlx::Error>;

    /// Update a component.
    async fn update_component(
        &self,
        id: Uuid,
        update: &UpdateComponent,
    ) -> Result<Option<Component>, sqlx::Error>;

    /// Delete a component.
    async fn delete_component(&self, id: Uuid) -> Result<(), sqlx::Error>;

    /// Update a component's position.
    async fn update_position(&self, id: Uuid, x: f32, y: f32) -> Result<(), sqlx::Error>;

    /// List dependencies for components in an application.
    async fn list_dependencies(&self, app_id: Uuid) -> Result<Vec<Dependency>, sqlx::Error>;

    /// Create a dependency.
    async fn create_dependency(
        &self,
        app_id: Uuid,
        from_id: Uuid,
        to_id: Uuid,
    ) -> Result<Dependency, sqlx::Error>;

    /// Delete a dependency.
    async fn delete_dependency(&self, id: Uuid) -> Result<bool, sqlx::Error>;
}

// ============================================================================
// Shared row types and conversions
// ============================================================================

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgComponentRow {
    id: Uuid,
    application_id: Uuid,
    name: String,
    component_type: String,
    display_name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    group_id: Option<Uuid>,
    host: Option<String>,
    agent_id: Option<Uuid>,
    check_cmd: Option<String>,
    start_cmd: Option<String>,
    stop_cmd: Option<String>,
    check_interval_seconds: i32,
    start_timeout_seconds: i32,
    stop_timeout_seconds: i32,
    is_optional: bool,
    position_x: Option<f32>,
    position_y: Option<f32>,
    cluster_size: Option<i32>,
    cluster_nodes: Option<Value>,
    referenced_app_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
impl From<PgComponentRow> for Component {
    fn from(r: PgComponentRow) -> Self {
        Component {
            id: r.id,
            application_id: r.application_id,
            name: r.name,
            component_type: r.component_type,
            display_name: r.display_name,
            description: r.description,
            icon: r.icon,
            group_id: r.group_id,
            host: r.host,
            agent_id: r.agent_id,
            check_cmd: r.check_cmd,
            start_cmd: r.start_cmd,
            stop_cmd: r.stop_cmd,
            check_interval_seconds: r.check_interval_seconds,
            start_timeout_seconds: r.start_timeout_seconds,
            stop_timeout_seconds: r.stop_timeout_seconds,
            is_optional: r.is_optional,
            position_x: r.position_x,
            position_y: r.position_y,
            cluster_size: r.cluster_size,
            cluster_nodes: r.cluster_nodes,
            referenced_app_id: r.referenced_app_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgDependencyRow {
    id: Uuid,
    application_id: Uuid,
    from_component_id: Uuid,
    to_component_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgComponentRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgComponentRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

const COMPONENT_COLS: &str = "id, application_id, name, component_type, display_name, description, icon, group_id, \
    host, agent_id, check_cmd, start_cmd, stop_cmd, \
    check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional, \
    position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at";

#[cfg(feature = "postgres")]
#[async_trait]
impl ComponentRepository for PgComponentRepository {
    async fn list_components(&self, app_id: Uuid) -> Result<Vec<Component>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM components WHERE application_id = $1 ORDER BY name",
            COMPONENT_COLS
        );
        let rows = sqlx::query_as::<_, PgComponentRow>(&sql)
            .bind(app_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_component(&self, id: Uuid, org_id: Uuid) -> Result<Option<Component>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgComponentRow>(
            "SELECT c.id, c.application_id, c.name, c.component_type, c.display_name, c.description, c.icon, c.group_id, \
                c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd, \
                c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds, c.is_optional, \
                c.position_x, c.position_y, c.cluster_size, c.cluster_nodes, c.referenced_app_id, c.created_at, c.updated_at \
                FROM components c JOIN applications a ON c.application_id = a.id \
                WHERE c.id = $1 AND a.organization_id = $2"
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn get_component_app_id(&self, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn get_component_app_and_agent(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, sqlx::Error> {
        let row: Option<(Uuid, Option<Uuid>)> =
            sqlx::query_as("SELECT application_id, agent_id FROM components WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    async fn create_component(&self, comp: CreateComponent) -> Result<Component, sqlx::Error> {
        let row = sqlx::query_as::<_, PgComponentRow>(
            &format!(
                "INSERT INTO components (id, application_id, name, component_type, display_name, description, icon, group_id, \
                    host, agent_id, check_cmd, start_cmd, stop_cmd, \
                    check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional, \
                    position_x, position_y, env_vars, tags, cluster_size, cluster_nodes, referenced_app_id, current_state) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, 'STOPPED') \
                 RETURNING {}", COMPONENT_COLS),
        )
        .bind(comp.id)
        .bind(comp.application_id)
        .bind(&comp.name)
        .bind(&comp.component_type)
        .bind(&comp.display_name)
        .bind(&comp.description)
        .bind(&comp.icon)
        .bind(comp.group_id)
        .bind(&comp.host)
        .bind(comp.agent_id)
        .bind(&comp.check_cmd)
        .bind(&comp.start_cmd)
        .bind(&comp.stop_cmd)
        .bind(comp.check_interval_seconds)
        .bind(comp.start_timeout_seconds)
        .bind(comp.stop_timeout_seconds)
        .bind(comp.is_optional)
        .bind(comp.position_x)
        .bind(comp.position_y)
        .bind(&comp.env_vars)
        .bind(&comp.tags)
        .bind(comp.cluster_size)
        .bind(&comp.cluster_nodes)
        .bind(comp.referenced_app_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    async fn update_component(
        &self,
        id: Uuid,
        u: &UpdateComponent,
    ) -> Result<Option<Component>, sqlx::Error> {
        let sql = format!(
            "UPDATE components SET \
                name = COALESCE($2, name), \
                component_type = COALESCE($3, component_type), \
                display_name = $4, \
                description = $5, \
                icon = COALESCE($6, icon), \
                group_id = $7, \
                host = COALESCE($8, host), \
                agent_id = COALESCE($9, agent_id), \
                check_cmd = $10, \
                start_cmd = $11, \
                stop_cmd = $12, \
                check_interval_seconds = COALESCE($13, check_interval_seconds), \
                start_timeout_seconds = COALESCE($14, start_timeout_seconds), \
                stop_timeout_seconds = COALESCE($15, stop_timeout_seconds), \
                is_optional = COALESCE($16, is_optional), \
                position_x = COALESCE($17, position_x), \
                position_y = COALESCE($18, position_y), \
                cluster_size = $19, \
                cluster_nodes = $20, \
                referenced_app_id = $21, \
                updated_at = {} \
             WHERE id = $1 \
             RETURNING {}",
            crate::db::sql::now(),
            COMPONENT_COLS,
        );
        let row = sqlx::query_as::<_, PgComponentRow>(&sql)
            .bind(id)
            .bind(&u.name)
            .bind(&u.component_type)
            .bind(&u.display_name)
            .bind(&u.description)
            .bind(&u.icon)
            .bind(u.group_id)
            .bind(&u.host)
            .bind(u.agent_id)
            .bind(&u.check_cmd)
            .bind(&u.start_cmd)
            .bind(&u.stop_cmd)
            .bind(u.check_interval_seconds)
            .bind(u.start_timeout_seconds)
            .bind(u.stop_timeout_seconds)
            .bind(u.is_optional)
            .bind(u.position_x)
            .bind(u.position_y)
            .bind(u.cluster_size)
            .bind(&u.cluster_nodes)
            .bind(u.referenced_app_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_component(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM components WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_position(&self, id: Uuid, x: f32, y: f32) -> Result<(), sqlx::Error> {
        let sql = format!(
            "UPDATE components SET position_x = $2, position_y = $3, updated_at = {} WHERE id = $1",
            crate::db::sql::now()
        );
        sqlx::query(&sql)
            .bind(id)
            .bind(x)
            .bind(y)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_dependencies(&self, app_id: Uuid) -> Result<Vec<Dependency>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgDependencyRow>(
            "SELECT id, application_id, from_component_id, to_component_id, created_at \
             FROM dependencies WHERE application_id = $1",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Dependency {
                id: r.id,
                application_id: r.application_id,
                from_component_id: r.from_component_id,
                to_component_id: r.to_component_id,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn create_dependency(
        &self,
        app_id: Uuid,
        from_id: Uuid,
        to_id: Uuid,
    ) -> Result<Dependency, sqlx::Error> {
        let row = sqlx::query_as::<_, PgDependencyRow>(
            "INSERT INTO dependencies (application_id, from_component_id, to_component_id) \
             VALUES ($1, $2, $3) \
             RETURNING id, application_id, from_component_id, to_component_id, created_at",
        )
        .bind(app_id)
        .bind(from_id)
        .bind(to_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(Dependency {
            id: row.id,
            application_id: row.application_id,
            from_component_id: row.from_component_id,
            to_component_id: row.to_component_id,
            created_at: row.created_at,
        })
    }

    async fn delete_dependency(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM dependencies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteComponentRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteComponentRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteComponentRow {
    id: DbUuid,
    application_id: DbUuid,
    name: String,
    component_type: String,
    display_name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    group_id: Option<DbUuid>,
    host: Option<String>,
    agent_id: Option<DbUuid>,
    check_cmd: Option<String>,
    start_cmd: Option<String>,
    stop_cmd: Option<String>,
    check_interval_seconds: i32,
    start_timeout_seconds: i32,
    stop_timeout_seconds: i32,
    is_optional: bool,
    position_x: Option<f32>,
    position_y: Option<f32>,
    cluster_size: Option<i32>,
    cluster_nodes: Option<Value>,
    referenced_app_id: Option<DbUuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl From<SqliteComponentRow> for Component {
    fn from(r: SqliteComponentRow) -> Self {
        Component {
            id: r.id.into_inner(),
            application_id: r.application_id.into_inner(),
            name: r.name,
            component_type: r.component_type,
            display_name: r.display_name,
            description: r.description,
            icon: r.icon,
            group_id: r.group_id.map(|g| g.into_inner()),
            host: r.host,
            agent_id: r.agent_id.map(|a| a.into_inner()),
            check_cmd: r.check_cmd,
            start_cmd: r.start_cmd,
            stop_cmd: r.stop_cmd,
            check_interval_seconds: r.check_interval_seconds,
            start_timeout_seconds: r.start_timeout_seconds,
            stop_timeout_seconds: r.stop_timeout_seconds,
            is_optional: r.is_optional,
            position_x: r.position_x,
            position_y: r.position_y,
            cluster_size: r.cluster_size,
            cluster_nodes: r.cluster_nodes,
            referenced_app_id: r.referenced_app_id.map(|r| r.into_inner()),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteDependencyRow {
    id: DbUuid,
    application_id: DbUuid,
    from_component_id: DbUuid,
    to_component_id: DbUuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl ComponentRepository for SqliteComponentRepository {
    async fn list_components(&self, app_id: Uuid) -> Result<Vec<Component>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM components WHERE application_id = $1 ORDER BY name",
            COMPONENT_COLS
        );
        let rows = sqlx::query_as::<_, SqliteComponentRow>(&sql)
            .bind(DbUuid::from(app_id))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_component(&self, id: Uuid, org_id: Uuid) -> Result<Option<Component>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteComponentRow>(
            "SELECT c.id, c.application_id, c.name, c.component_type, c.display_name, c.description, c.icon, c.group_id, \
                c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd, \
                c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds, c.is_optional, \
                c.position_x, c.position_y, c.cluster_size, c.cluster_nodes, c.referenced_app_id, c.created_at, c.updated_at \
                FROM components c JOIN applications a ON c.application_id = a.id \
                WHERE c.id = $1 AND a.organization_id = $2"
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn get_component_app_id(&self, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT application_id FROM components WHERE id = $1",
        )
        .bind(DbUuid::from(id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }

    async fn get_component_app_and_agent(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, sqlx::Error> {
        let row: Option<(DbUuid, Option<DbUuid>)> =
            sqlx::query_as("SELECT application_id, agent_id FROM components WHERE id = $1")
                .bind(DbUuid::from(id))
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(app, agent)| (app.into_inner(), agent.map(|a| a.into_inner()))))
    }

    async fn create_component(&self, comp: CreateComponent) -> Result<Component, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteComponentRow>(
            &format!(
                "INSERT INTO components (id, application_id, name, component_type, display_name, description, icon, group_id, \
                    host, agent_id, check_cmd, start_cmd, stop_cmd, \
                    check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional, \
                    position_x, position_y, env_vars, tags, cluster_size, cluster_nodes, referenced_app_id, current_state) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, 'STOPPED') \
                 RETURNING {}", COMPONENT_COLS),
        )
        .bind(DbUuid::from(comp.id))
        .bind(DbUuid::from(comp.application_id))
        .bind(&comp.name)
        .bind(&comp.component_type)
        .bind(&comp.display_name)
        .bind(&comp.description)
        .bind(&comp.icon)
        .bind(comp.group_id.map(DbUuid::from))
        .bind(&comp.host)
        .bind(comp.agent_id.map(DbUuid::from))
        .bind(&comp.check_cmd)
        .bind(&comp.start_cmd)
        .bind(&comp.stop_cmd)
        .bind(comp.check_interval_seconds)
        .bind(comp.start_timeout_seconds)
        .bind(comp.stop_timeout_seconds)
        .bind(comp.is_optional)
        .bind(comp.position_x)
        .bind(comp.position_y)
        .bind(&comp.env_vars)
        .bind(&comp.tags)
        .bind(comp.cluster_size)
        .bind(&comp.cluster_nodes)
        .bind(comp.referenced_app_id.map(DbUuid::from))
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    async fn update_component(
        &self,
        id: Uuid,
        u: &UpdateComponent,
    ) -> Result<Option<Component>, sqlx::Error> {
        let sql = format!(
            "UPDATE components SET \
                name = COALESCE($2, name), \
                component_type = COALESCE($3, component_type), \
                display_name = $4, \
                description = $5, \
                icon = COALESCE($6, icon), \
                group_id = $7, \
                host = COALESCE($8, host), \
                agent_id = COALESCE($9, agent_id), \
                check_cmd = $10, \
                start_cmd = $11, \
                stop_cmd = $12, \
                check_interval_seconds = COALESCE($13, check_interval_seconds), \
                start_timeout_seconds = COALESCE($14, start_timeout_seconds), \
                stop_timeout_seconds = COALESCE($15, stop_timeout_seconds), \
                is_optional = COALESCE($16, is_optional), \
                position_x = COALESCE($17, position_x), \
                position_y = COALESCE($18, position_y), \
                cluster_size = $19, \
                cluster_nodes = $20, \
                referenced_app_id = $21, \
                updated_at = {} \
             WHERE id = $1 \
             RETURNING {}",
            crate::db::sql::now(),
            COMPONENT_COLS,
        );
        let row = sqlx::query_as::<_, SqliteComponentRow>(&sql)
            .bind(DbUuid::from(id))
            .bind(&u.name)
            .bind(&u.component_type)
            .bind(&u.display_name)
            .bind(&u.description)
            .bind(&u.icon)
            .bind(u.group_id.map(DbUuid::from))
            .bind(&u.host)
            .bind(u.agent_id.map(DbUuid::from))
            .bind(&u.check_cmd)
            .bind(&u.start_cmd)
            .bind(&u.stop_cmd)
            .bind(u.check_interval_seconds)
            .bind(u.start_timeout_seconds)
            .bind(u.stop_timeout_seconds)
            .bind(u.is_optional)
            .bind(u.position_x)
            .bind(u.position_y)
            .bind(u.cluster_size)
            .bind(&u.cluster_nodes)
            .bind(u.referenced_app_id.map(DbUuid::from))
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_component(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM components WHERE id = $1")
            .bind(DbUuid::from(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_position(&self, id: Uuid, x: f32, y: f32) -> Result<(), sqlx::Error> {
        let sql = format!(
            "UPDATE components SET position_x = $2, position_y = $3, updated_at = {} WHERE id = $1",
            crate::db::sql::now()
        );
        sqlx::query(&sql)
            .bind(DbUuid::from(id))
            .bind(x)
            .bind(y)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_dependencies(&self, app_id: Uuid) -> Result<Vec<Dependency>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteDependencyRow>(
            "SELECT id, application_id, from_component_id, to_component_id, created_at \
             FROM dependencies WHERE application_id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Dependency {
                id: r.id.into_inner(),
                application_id: r.application_id.into_inner(),
                from_component_id: r.from_component_id.into_inner(),
                to_component_id: r.to_component_id.into_inner(),
                created_at: r.created_at,
            })
            .collect())
    }

    async fn create_dependency(
        &self,
        app_id: Uuid,
        from_id: Uuid,
        to_id: Uuid,
    ) -> Result<Dependency, sqlx::Error> {
        let new_id = DbUuid::new_v4();
        let row = sqlx::query_as::<_, SqliteDependencyRow>(
            "INSERT INTO dependencies (id, application_id, from_component_id, to_component_id) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, application_id, from_component_id, to_component_id, created_at",
        )
        .bind(new_id)
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(from_id))
        .bind(DbUuid::from(to_id))
        .fetch_one(&self.pool)
        .await?;
        Ok(Dependency {
            id: row.id.into_inner(),
            application_id: row.application_id.into_inner(),
            from_component_id: row.from_component_id.into_inner(),
            to_component_id: row.to_component_id.into_inner(),
            created_at: row.created_at,
        })
    }

    async fn delete_dependency(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM dependencies WHERE id = $1")
            .bind(DbUuid::from(id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_component_repository(pool: DbPool) -> Box<dyn ComponentRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgComponentRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteComponentRepository::new(pool))
    }
}
