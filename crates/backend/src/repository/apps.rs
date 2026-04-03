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

/// Component with agent and gateway connectivity info (for app detail view).
#[derive(Debug)]
pub struct ComponentWithAgentInfo {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    pub component_type: String,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub current_state: String,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Value>,
    pub referenced_app_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub agent_hostname: Option<String>,
    pub gateway_id: Option<Uuid>,
    pub gateway_name: Option<String>,
    pub last_check_metrics: Option<Value>,
}

/// Simplified dependency info (id, from, to).
#[derive(Debug, serde::Serialize)]
pub struct DependencyInfo {
    pub id: Uuid,
    pub from_component_id: Uuid,
    pub to_component_id: Uuid,
}

/// Application site info.
#[derive(Debug)]
pub struct AppSiteInfo {
    pub site_id: Uuid,
    pub site_name: String,
    pub site_code: String,
    pub site_type: String,
}

/// Site binding for a component.
#[derive(Debug)]
pub struct SiteBinding {
    pub component_id: Uuid,
    pub component_name: String,
    pub component_host: Option<String>,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub profile_type: String,
    pub is_active: bool,
    pub agent_id: Uuid,
    pub agent_hostname: String,
    pub site_id: Uuid,
    pub site_name: String,
    pub site_code: String,
    pub site_type: String,
}

/// Command override for a component on a site.
#[derive(Debug)]
pub struct CmdOverride {
    pub component_id: Uuid,
    pub site_id: Uuid,
    pub check_cmd_override: Option<String>,
    pub start_cmd_override: Option<String>,
    pub stop_cmd_override: Option<String>,
    pub rebuild_cmd_override: Option<String>,
    pub env_vars_override: Option<Value>,
}

/// Referenced app status counts.
#[derive(Debug)]
pub struct RefAppStatus {
    pub app_id: Uuid,
    pub app_name: String,
    pub running_count: i64,
    pub stopped_count: i64,
    pub failed_count: i64,
    pub starting_count: i64,
    pub stopping_count: i64,
    pub component_count: i64,
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

    /// Verify an app belongs to an organization, returning its ID.
    async fn verify_app_org(&self, app_id: Uuid, org_id: Uuid)
        -> Result<Option<Uuid>, sqlx::Error>;

    /// Get application name by ID.
    async fn get_app_name(&self, app_id: Uuid) -> Result<Option<String>, sqlx::Error>;

    /// Check if an application is suspended.
    async fn is_app_suspended(&self, app_id: Uuid) -> Result<bool, sqlx::Error>;

    /// Suspend an application.
    async fn suspend_app(&self, app_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error>;

    /// Resume a suspended application.
    async fn resume_app(&self, app_id: Uuid) -> Result<(), sqlx::Error>;

    /// Insert a config version snapshot.
    async fn insert_config_version(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        changed_by: Uuid,
        before_snapshot: &str,
        after_snapshot: &str,
    ) -> Result<(), sqlx::Error>;

    /// Fetch component IDs with FAILED state for an application.
    async fn get_failed_component_ids(&self, app_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error>;

    /// Get component name by ID.
    async fn get_component_name(&self, id: Uuid) -> Result<Option<String>, sqlx::Error>;

    /// Get components with agent and gateway info for an application (detail view).
    async fn get_components_with_agents(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<ComponentWithAgentInfo>, sqlx::Error>;

    /// Get dependencies for an app (simplified: id, from, to).
    async fn get_app_dependencies(&self, app_id: Uuid) -> Result<Vec<DependencyInfo>, sqlx::Error>;

    /// Get the application site info (site_id, site_name, site_code, site_type).
    async fn get_app_site_info(&self, app_id: Uuid) -> Result<Option<AppSiteInfo>, sqlx::Error>;

    /// Get binding profile mappings with site info for an app.
    async fn get_site_bindings(&self, app_id: Uuid) -> Result<Vec<SiteBinding>, sqlx::Error>;

    /// Get command overrides for components in an app.
    async fn get_cmd_overrides(&self, app_id: Uuid) -> Result<Vec<CmdOverride>, sqlx::Error>;

    /// Fetch referenced app status counts.
    async fn get_referenced_app_statuses(
        &self,
        app_ids: &[Uuid],
    ) -> Result<Vec<RefAppStatus>, sqlx::Error>;
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

    async fn verify_app_org(
        &self,
        app_id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM applications WHERE id = $1 AND organization_id = $2",
        )
        .bind(app_id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn get_app_name(&self, app_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
            .bind(app_id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn is_app_suspended(&self, app_id: Uuid) -> Result<bool, sqlx::Error> {
        let val: Option<bool> =
            sqlx::query_scalar("SELECT is_suspended FROM applications WHERE id = $1")
                .bind(app_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(val.unwrap_or(false))
    }

    async fn suspend_app(&self, app_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE applications SET is_suspended = true, suspended_at = now(), suspended_by = $2, updated_at = now() WHERE id = $1",
        )
        .bind(app_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resume_app(&self, app_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE applications SET is_suspended = false, suspended_at = NULL, suspended_by = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(app_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_config_version(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        changed_by: Uuid,
        before_snapshot: &str,
        after_snapshot: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot) \
             VALUES ($1, $2, $3, $4::jsonb, $5::jsonb)",
        )
        .bind(resource_type)
        .bind(resource_id)
        .bind(changed_by)
        .bind(before_snapshot)
        .bind(after_snapshot)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_failed_component_ids(&self, app_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM components WHERE application_id = $1 AND current_state = 'FAILED'",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn get_component_name(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT name FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn get_components_with_agents(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<ComponentWithAgentInfo>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            application_id: Uuid,
            name: String,
            display_name: Option<String>,
            description: Option<String>,
            icon: Option<String>,
            group_id: Option<Uuid>,
            component_type: String,
            host: Option<String>,
            agent_id: Option<Uuid>,
            check_cmd: Option<String>,
            start_cmd: Option<String>,
            stop_cmd: Option<String>,
            check_interval_seconds: i32,
            start_timeout_seconds: i32,
            stop_timeout_seconds: i32,
            is_optional: bool,
            current_state: String,
            position_x: Option<f32>,
            position_y: Option<f32>,
            cluster_size: Option<i32>,
            cluster_nodes: Option<Value>,
            referenced_app_id: Option<Uuid>,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
            agent_hostname: Option<String>,
            gateway_id: Option<Uuid>,
            gateway_name: Option<String>,
            last_check_metrics: Option<Value>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT c.id, c.application_id, c.name, c.display_name, c.description, c.icon, c.group_id,
                      c.component_type, c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd,
                      c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                      c.is_optional, c.current_state, c.position_x, c.position_y,
                      c.cluster_size, c.cluster_nodes, c.referenced_app_id, c.created_at, c.updated_at,
                      a.hostname as agent_hostname, a.gateway_id, g.name as gateway_name,
                      (SELECT ce.metrics FROM check_events ce
                       WHERE ce.component_id = c.id AND ce.metrics IS NOT NULL
                       ORDER BY ce.created_at DESC LIMIT 1) as last_check_metrics
               FROM components c
               LEFT JOIN agents a ON c.agent_id = a.id
               LEFT JOIN gateways g ON a.gateway_id = g.id
               WHERE c.application_id = $1 ORDER BY c.name"#,
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ComponentWithAgentInfo {
                id: r.id,
                application_id: r.application_id,
                name: r.name,
                display_name: r.display_name,
                description: r.description,
                icon: r.icon,
                group_id: r.group_id,
                component_type: r.component_type,
                host: r.host,
                agent_id: r.agent_id,
                check_cmd: r.check_cmd,
                start_cmd: r.start_cmd,
                stop_cmd: r.stop_cmd,
                check_interval_seconds: r.check_interval_seconds,
                start_timeout_seconds: r.start_timeout_seconds,
                stop_timeout_seconds: r.stop_timeout_seconds,
                is_optional: r.is_optional,
                current_state: r.current_state,
                position_x: r.position_x,
                position_y: r.position_y,
                cluster_size: r.cluster_size,
                cluster_nodes: r.cluster_nodes,
                referenced_app_id: r.referenced_app_id,
                created_at: r.created_at,
                updated_at: r.updated_at,
                agent_hostname: r.agent_hostname,
                gateway_id: r.gateway_id,
                gateway_name: r.gateway_name,
                last_check_metrics: r.last_check_metrics,
            })
            .collect())
    }

    async fn get_app_dependencies(&self, app_id: Uuid) -> Result<Vec<DependencyInfo>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            from_component_id: Uuid,
            to_component_id: Uuid,
        }

        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, from_component_id, to_component_id FROM dependencies \
             WHERE from_component_id IN (SELECT id FROM components WHERE application_id = $1)",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DependencyInfo {
                id: r.id,
                from_component_id: r.from_component_id,
                to_component_id: r.to_component_id,
            })
            .collect())
    }

    async fn get_app_site_info(&self, app_id: Uuid) -> Result<Option<AppSiteInfo>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            site_id: Uuid,
            site_name: String,
            site_code: String,
            site_type: String,
        }

        let row = sqlx::query_as::<_, Row>(
            "SELECT a.site_id, s.name as site_name, s.code as site_code, s.site_type \
             FROM applications a JOIN sites s ON a.site_id = s.id WHERE a.id = $1",
        )
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| AppSiteInfo {
            site_id: r.site_id,
            site_name: r.site_name,
            site_code: r.site_code,
            site_type: r.site_type,
        }))
    }

    async fn get_site_bindings(&self, app_id: Uuid) -> Result<Vec<SiteBinding>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            component_id: Uuid,
            component_name: String,
            component_host: Option<String>,
            profile_id: Uuid,
            profile_name: String,
            profile_type: String,
            is_active: bool,
            agent_id: Uuid,
            agent_hostname: String,
            site_id: Uuid,
            site_name: String,
            site_code: String,
            site_type: String,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT DISTINCT ON (c.id, s.id)
                c.id as component_id, c.name as component_name, c.host as component_host,
                bp.id as profile_id, bp.name as profile_name, bp.profile_type,
                (c.agent_id = bpm.agent_id) as is_active,
                bpm.agent_id, a.hostname as agent_hostname,
                s.id as site_id, s.name as site_name, s.code as site_code, s.site_type
            FROM components c
            JOIN binding_profile_mappings bpm ON bpm.component_name = c.name
            JOIN binding_profiles bp ON bpm.profile_id = bp.id AND bp.application_id = c.application_id
            JOIN agents a ON bpm.agent_id = a.id
            JOIN gateways g ON a.gateway_id = g.id
            JOIN sites s ON g.site_id = s.id
            WHERE c.application_id = $1
            ORDER BY c.id, s.id, (c.agent_id = bpm.agent_id) DESC"#,
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SiteBinding {
                component_id: r.component_id,
                component_name: r.component_name,
                component_host: r.component_host,
                profile_id: r.profile_id,
                profile_name: r.profile_name,
                profile_type: r.profile_type,
                is_active: r.is_active,
                agent_id: r.agent_id,
                agent_hostname: r.agent_hostname,
                site_id: r.site_id,
                site_name: r.site_name,
                site_code: r.site_code,
                site_type: r.site_type,
            })
            .collect())
    }

    async fn get_cmd_overrides(&self, app_id: Uuid) -> Result<Vec<CmdOverride>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            component_id: Uuid,
            site_id: Uuid,
            check_cmd_override: Option<String>,
            start_cmd_override: Option<String>,
            stop_cmd_override: Option<String>,
            rebuild_cmd_override: Option<String>,
            env_vars_override: Option<Value>,
        }

        let rows = sqlx::query_as::<_, Row>(
            "SELECT component_id, site_id, check_cmd_override, start_cmd_override, \
             stop_cmd_override, rebuild_cmd_override, env_vars_override \
             FROM site_overrides \
             WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| CmdOverride {
                component_id: r.component_id,
                site_id: r.site_id,
                check_cmd_override: r.check_cmd_override,
                start_cmd_override: r.start_cmd_override,
                stop_cmd_override: r.stop_cmd_override,
                rebuild_cmd_override: r.rebuild_cmd_override,
                env_vars_override: r.env_vars_override,
            })
            .collect())
    }

    async fn get_referenced_app_statuses(
        &self,
        app_ids: &[Uuid],
    ) -> Result<Vec<RefAppStatus>, sqlx::Error> {
        if app_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(sqlx::FromRow)]
        struct Row {
            app_id: Uuid,
            app_name: String,
            component_count: Option<i64>,
            running_count: Option<i64>,
            starting_count: Option<i64>,
            stopping_count: Option<i64>,
            stopped_count: Option<i64>,
            failed_count: Option<i64>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT a.id as app_id, a.name as app_name,
                COUNT(c.id) as component_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'RUNNING') as running_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'STARTING') as starting_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPING') as stopping_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'STOPPED') as stopped_count,
                COUNT(c.id) FILTER (WHERE c.current_state = 'FAILED') as failed_count
            FROM applications a
            LEFT JOIN components c ON c.application_id = a.id
            WHERE a.id = ANY($1)
            GROUP BY a.id, a.name"#,
        )
        .bind(app_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| RefAppStatus {
                app_id: r.app_id,
                app_name: r.app_name,
                running_count: r.running_count.unwrap_or(0),
                stopped_count: r.stopped_count.unwrap_or(0),
                failed_count: r.failed_count.unwrap_or(0),
                starting_count: r.starting_count.unwrap_or(0),
                stopping_count: r.stopping_count.unwrap_or(0),
                component_count: r.component_count.unwrap_or(0),
            })
            .collect())
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

    async fn verify_app_org(
        &self,
        app_id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM applications WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }

    async fn get_app_name(&self, app_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
            .bind(DbUuid::from(app_id))
            .fetch_optional(&self.pool)
            .await
    }

    async fn is_app_suspended(&self, app_id: Uuid) -> Result<bool, sqlx::Error> {
        let val: Option<bool> =
            sqlx::query_scalar("SELECT is_suspended FROM applications WHERE id = $1")
                .bind(DbUuid::from(app_id))
                .fetch_optional(&self.pool)
                .await?;
        Ok(val.unwrap_or(false))
    }

    async fn suspend_app(&self, app_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        let sql = format!(
            "UPDATE applications SET is_suspended = 1, suspended_at = {now}, suspended_by = $2, updated_at = {now} WHERE id = $1",
            now = crate::db::sql::now()
        );
        sqlx::query(&sql)
            .bind(DbUuid::from(app_id))
            .bind(DbUuid::from(user_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn resume_app(&self, app_id: Uuid) -> Result<(), sqlx::Error> {
        let sql = format!(
            "UPDATE applications SET is_suspended = 0, suspended_at = NULL, suspended_by = NULL, updated_at = {} WHERE id = $1",
            crate::db::sql::now()
        );
        sqlx::query(&sql)
            .bind(DbUuid::from(app_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn insert_config_version(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        changed_by: Uuid,
        before_snapshot: &str,
        after_snapshot: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO config_versions (id, resource_type, resource_id, changed_by, before_snapshot, after_snapshot) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(DbUuid::new_v4())
        .bind(resource_type)
        .bind(DbUuid::from(resource_id))
        .bind(DbUuid::from(changed_by))
        .bind(before_snapshot)
        .bind(after_snapshot)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_failed_component_ids(&self, app_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM components WHERE application_id = $1 AND current_state = 'FAILED'",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|u| u.into_inner()).collect())
    }

    async fn get_component_name(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT name FROM components WHERE id = $1")
            .bind(DbUuid::from(id))
            .fetch_optional(&self.pool)
            .await
    }

    async fn get_components_with_agents(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<ComponentWithAgentInfo>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            application_id: DbUuid,
            name: String,
            display_name: Option<String>,
            description: Option<String>,
            icon: Option<String>,
            group_id: Option<DbUuid>,
            component_type: String,
            host: Option<String>,
            agent_id: Option<DbUuid>,
            check_cmd: Option<String>,
            start_cmd: Option<String>,
            stop_cmd: Option<String>,
            check_interval_seconds: i32,
            start_timeout_seconds: i32,
            stop_timeout_seconds: i32,
            is_optional: bool,
            current_state: String,
            position_x: Option<f32>,
            position_y: Option<f32>,
            cluster_size: Option<i32>,
            cluster_nodes: Option<Value>,
            referenced_app_id: Option<DbUuid>,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
            agent_hostname: Option<String>,
            gateway_id: Option<DbUuid>,
            gateway_name: Option<String>,
            last_check_metrics: Option<Value>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT c.id, c.application_id, c.name, c.display_name, c.description, c.icon, c.group_id,
                      c.component_type, c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd,
                      c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                      c.is_optional, c.current_state, c.position_x, c.position_y,
                      c.cluster_size, c.cluster_nodes, c.referenced_app_id, c.created_at, c.updated_at,
                      a.hostname as agent_hostname, a.gateway_id, g.name as gateway_name,
                      (SELECT ce.metrics FROM check_events ce
                       WHERE ce.component_id = c.id AND ce.metrics IS NOT NULL
                       ORDER BY ce.created_at DESC LIMIT 1) as last_check_metrics
               FROM components c
               LEFT JOIN agents a ON c.agent_id = a.id
               LEFT JOIN gateways g ON a.gateway_id = g.id
               WHERE c.application_id = $1 ORDER BY c.name"#,
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ComponentWithAgentInfo {
                id: r.id.into_inner(),
                application_id: r.application_id.into_inner(),
                name: r.name,
                display_name: r.display_name,
                description: r.description,
                icon: r.icon,
                group_id: r.group_id.map(|g| g.into_inner()),
                component_type: r.component_type,
                host: r.host,
                agent_id: r.agent_id.map(|a| a.into_inner()),
                check_cmd: r.check_cmd,
                start_cmd: r.start_cmd,
                stop_cmd: r.stop_cmd,
                check_interval_seconds: r.check_interval_seconds,
                start_timeout_seconds: r.start_timeout_seconds,
                stop_timeout_seconds: r.stop_timeout_seconds,
                is_optional: r.is_optional,
                current_state: r.current_state,
                position_x: r.position_x,
                position_y: r.position_y,
                cluster_size: r.cluster_size,
                cluster_nodes: r.cluster_nodes,
                referenced_app_id: r.referenced_app_id.map(|r| r.into_inner()),
                created_at: r.created_at,
                updated_at: r.updated_at,
                agent_hostname: r.agent_hostname,
                gateway_id: r.gateway_id.map(|g| g.into_inner()),
                gateway_name: r.gateway_name,
                last_check_metrics: r.last_check_metrics,
            })
            .collect())
    }

    async fn get_app_dependencies(&self, app_id: Uuid) -> Result<Vec<DependencyInfo>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            from_component_id: DbUuid,
            to_component_id: DbUuid,
        }

        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, from_component_id, to_component_id FROM dependencies \
             WHERE from_component_id IN (SELECT id FROM components WHERE application_id = $1)",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DependencyInfo {
                id: r.id.into_inner(),
                from_component_id: r.from_component_id.into_inner(),
                to_component_id: r.to_component_id.into_inner(),
            })
            .collect())
    }

    async fn get_app_site_info(&self, app_id: Uuid) -> Result<Option<AppSiteInfo>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            site_id: DbUuid,
            site_name: String,
            site_code: String,
            site_type: String,
        }

        let row = sqlx::query_as::<_, Row>(
            "SELECT a.site_id, s.name as site_name, s.code as site_code, s.site_type \
             FROM applications a JOIN sites s ON a.site_id = s.id WHERE a.id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| AppSiteInfo {
            site_id: r.site_id.into_inner(),
            site_name: r.site_name,
            site_code: r.site_code,
            site_type: r.site_type,
        }))
    }

    async fn get_site_bindings(&self, app_id: Uuid) -> Result<Vec<SiteBinding>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            component_id: DbUuid,
            component_name: String,
            component_host: Option<String>,
            profile_id: DbUuid,
            profile_name: String,
            profile_type: String,
            is_active: bool,
            agent_id: DbUuid,
            agent_hostname: String,
            site_id: DbUuid,
            site_name: String,
            site_code: String,
            site_type: String,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT
                c.id as component_id, c.name as component_name, c.host as component_host,
                bp.id as profile_id, bp.name as profile_name, bp.profile_type,
                (c.agent_id = bpm.agent_id) as is_active,
                bpm.agent_id, a.hostname as agent_hostname,
                s.id as site_id, s.name as site_name, s.code as site_code, s.site_type
            FROM components c
            JOIN binding_profile_mappings bpm ON bpm.component_name = c.name
            JOIN binding_profiles bp ON bpm.profile_id = bp.id AND bp.application_id = c.application_id
            JOIN agents a ON bpm.agent_id = a.id
            JOIN gateways g ON a.gateway_id = g.id
            JOIN sites s ON g.site_id = s.id
            WHERE c.application_id = $1
            GROUP BY c.id, s.id
            HAVING (c.agent_id = bpm.agent_id) = MAX(c.agent_id = bpm.agent_id)
            ORDER BY c.id, s.id"#,
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SiteBinding {
                component_id: r.component_id.into_inner(),
                component_name: r.component_name,
                component_host: r.component_host,
                profile_id: r.profile_id.into_inner(),
                profile_name: r.profile_name,
                profile_type: r.profile_type,
                is_active: r.is_active,
                agent_id: r.agent_id.into_inner(),
                agent_hostname: r.agent_hostname,
                site_id: r.site_id.into_inner(),
                site_name: r.site_name,
                site_code: r.site_code,
                site_type: r.site_type,
            })
            .collect())
    }

    async fn get_cmd_overrides(&self, app_id: Uuid) -> Result<Vec<CmdOverride>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            component_id: DbUuid,
            site_id: DbUuid,
            check_cmd_override: Option<String>,
            start_cmd_override: Option<String>,
            stop_cmd_override: Option<String>,
            rebuild_cmd_override: Option<String>,
            env_vars_override: Option<Value>,
        }

        let rows = sqlx::query_as::<_, Row>(
            "SELECT component_id, site_id, check_cmd_override, start_cmd_override, \
             stop_cmd_override, rebuild_cmd_override, env_vars_override \
             FROM site_overrides \
             WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| CmdOverride {
                component_id: r.component_id.into_inner(),
                site_id: r.site_id.into_inner(),
                check_cmd_override: r.check_cmd_override,
                start_cmd_override: r.start_cmd_override,
                stop_cmd_override: r.stop_cmd_override,
                rebuild_cmd_override: r.rebuild_cmd_override,
                env_vars_override: r.env_vars_override,
            })
            .collect())
    }

    async fn get_referenced_app_statuses(
        &self,
        app_ids: &[Uuid],
    ) -> Result<Vec<RefAppStatus>, sqlx::Error> {
        if app_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(sqlx::FromRow)]
        struct Row {
            app_id: String,
            app_name: String,
            component_count: i64,
            running_count: i64,
            starting_count: i64,
            stopping_count: i64,
            stopped_count: i64,
            failed_count: i64,
        }

        let placeholders: Vec<String> = (1..=app_ids.len()).map(|i| format!("${}", i)).collect();
        let query = format!(
            r#"SELECT a.id as app_id, a.name as app_name,
                COUNT(c.id) as component_count,
                SUM(CASE WHEN c.current_state = 'RUNNING' THEN 1 ELSE 0 END) as running_count,
                SUM(CASE WHEN c.current_state = 'STARTING' THEN 1 ELSE 0 END) as starting_count,
                SUM(CASE WHEN c.current_state = 'STOPPING' THEN 1 ELSE 0 END) as stopping_count,
                SUM(CASE WHEN c.current_state = 'STOPPED' THEN 1 ELSE 0 END) as stopped_count,
                SUM(CASE WHEN c.current_state = 'FAILED' THEN 1 ELSE 0 END) as failed_count
            FROM applications a
            LEFT JOIN components c ON c.application_id = a.id
            WHERE a.id IN ({})
            GROUP BY a.id, a.name"#,
            placeholders.join(", ")
        );

        let mut q = sqlx::query_as::<_, Row>(&query);
        for id in app_ids {
            q = q.bind(id.to_string());
        }
        let rows = q.fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                uuid::Uuid::parse_str(&r.app_id)
                    .ok()
                    .map(|id| RefAppStatus {
                        app_id: id,
                        app_name: r.app_name,
                        running_count: r.running_count,
                        stopped_count: r.stopped_count,
                        failed_count: r.failed_count,
                        starting_count: r.starting_count,
                        stopping_count: r.stopping_count,
                        component_count: r.component_count,
                    })
            })
            .collect())
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
