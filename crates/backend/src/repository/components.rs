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
    pub cluster_mode: Option<String>,
    pub cluster_health_policy: Option<String>,
    pub cluster_min_healthy_pct: Option<i16>,
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

/// Component command columns (for execute_command).
#[derive(Debug)]
pub struct ComponentCommands {
    pub application_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
}

/// Custom command definition.
#[derive(Debug)]
pub struct CustomCommand {
    pub id: Uuid,
    pub command: String,
    pub requires_confirmation: bool,
}

/// Command input parameter.
#[derive(Debug, serde::Serialize)]
pub struct CommandParam {
    pub id: Uuid,
    pub command_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
    pub required: bool,
    pub display_order: i32,
    pub param_type: String,
    pub enum_values: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Raw custom command row (serializable).
#[derive(Debug, serde::Serialize)]
pub struct CustomCommandRaw {
    pub id: Uuid,
    pub component_id: Uuid,
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub requires_confirmation: bool,
    pub min_permission_level: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Raw command execution row (serializable).
#[derive(Debug, serde::Serialize)]
pub struct CommandExecutionRaw {
    pub id: Uuid,
    pub request_id: Uuid,
    pub component_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub command_type: String,
    pub exit_code: Option<i16>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i32>,
    pub status: String,
    pub dispatched_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Raw state transition row (serializable).
#[derive(Debug, serde::Serialize)]
pub struct StateTransitionRaw {
    pub id: Uuid,
    pub component_id: Uuid,
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Raw check event row (serializable).
#[derive(Debug, serde::Serialize)]
pub struct CheckEventRaw {
    pub id: i64,
    pub component_id: Uuid,
    pub check_type: String,
    pub exit_code: i16,
    pub stdout: Option<String>,
    pub duration_ms: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait ComponentRepository: Send + Sync {
    /// List components for an application.
    async fn list_components(&self, app_id: Uuid) -> Result<Vec<Component>, sqlx::Error>;

    /// Get a component by ID with organization check.
    async fn get_component(&self, id: Uuid, org_id: Uuid)
        -> Result<Option<Component>, sqlx::Error>;

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

    /// Get component app_id and referenced_app_id.
    async fn get_component_refs(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, sqlx::Error>;

    /// Get component command columns (for execute_command).
    async fn get_component_commands(
        &self,
        id: Uuid,
    ) -> Result<Option<ComponentCommands>, sqlx::Error>;

    /// Look up a custom command definition by component_id and name.
    async fn get_custom_command(
        &self,
        component_id: Uuid,
        name: &str,
    ) -> Result<Option<CustomCommand>, sqlx::Error>;

    /// List input params for a command.
    async fn list_command_params(&self, command_id: Uuid)
        -> Result<Vec<CommandParam>, sqlx::Error>;

    /// Insert a command execution record.
    async fn insert_command_execution(
        &self,
        request_id: Uuid,
        component_id: Uuid,
        agent_id: Uuid,
        command_type: &str,
        user_id: Uuid,
        command_text: &str,
    ) -> Result<(), sqlx::Error>;

    /// Get dependency app_id.
    async fn get_dependency_app_id(&self, id: Uuid) -> Result<Option<Uuid>, sqlx::Error>;

    /// List custom commands for a component.
    async fn list_custom_commands_raw(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<CustomCommandRaw>, sqlx::Error>;

    /// List command executions for a component.
    async fn list_command_executions(
        &self,
        component_id: Uuid,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CommandExecutionRaw>, sqlx::Error>;

    /// List state transitions for a component.
    async fn list_state_transitions(
        &self,
        component_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StateTransitionRaw>, sqlx::Error>;

    /// List check events for a component.
    async fn list_check_events(
        &self,
        component_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CheckEventRaw>, sqlx::Error>;

    /// Resolve host to agent_id.
    async fn resolve_host_to_agent(&self, host: &str) -> Result<Option<Uuid>, sqlx::Error>;

    /// Batch update positions in a transaction.
    async fn batch_update_positions(
        &self,
        app_id: Uuid,
        positions: &[(Uuid, f32, f32)],
    ) -> Result<(), sqlx::Error>;

    /// Insert a config version snapshot.
    async fn insert_config_version(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        changed_by: Uuid,
        before_snapshot: &str,
        after_snapshot: &str,
    ) -> Result<(), sqlx::Error>;

    /// Auto-bind components that reference a host to the given agent.
    async fn auto_bind_agent(
        &self,
        agent_id: Uuid,
        hostname: &str,
        ip_addresses: &[String],
    ) -> Result<u64, sqlx::Error>;
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
    cluster_mode: Option<String>,
    cluster_health_policy: Option<String>,
    cluster_min_healthy_pct: Option<i16>,
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
            cluster_mode: r.cluster_mode,
            cluster_health_policy: r.cluster_health_policy,
            cluster_min_healthy_pct: r.cluster_min_healthy_pct,
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

const COMPONENT_COLS: &str =
    "id, application_id, name, component_type, display_name, description, icon, group_id, \
    host, agent_id, check_cmd, start_cmd, stop_cmd, \
    check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional, \
    position_x, position_y, cluster_size, cluster_nodes, cluster_mode, cluster_health_policy, cluster_min_healthy_pct, referenced_app_id, created_at, updated_at";

#[cfg(feature = "postgres")]
#[async_trait]
impl ComponentRepository for PgComponentRepository {
    async fn list_components(&self, app_id: Uuid) -> Result<Vec<Component>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM components WHERE application_id = $1 ORDER BY name",
            COMPONENT_COLS
        );
        let rows = sqlx::query_as::<_, PgComponentRow>(&sql)
            .bind(crate::db::bind_id(app_id))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_component(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<Component>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgComponentRow>(
            "SELECT c.id, c.application_id, c.name, c.component_type, c.display_name, c.description, c.icon, c.group_id, \
                c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd, \
                c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds, c.is_optional, \
                c.position_x, c.position_y, c.cluster_size, c.cluster_nodes, c.cluster_mode, c.cluster_health_policy, c.cluster_min_healthy_pct, c.referenced_app_id, c.created_at, c.updated_at \
                FROM components c JOIN applications a ON c.application_id = a.id \
                WHERE c.id = $1 AND a.organization_id = $2"
        )
        .bind(id)
        .bind(crate::db::bind_id(org_id))
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
        .bind(crate::db::bind_id(app_id))
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
        .bind(crate::db::bind_id(app_id))
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

    async fn get_component_refs(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, sqlx::Error> {
        let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "SELECT application_id, referenced_app_id FROM components WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn get_component_commands(
        &self,
        id: Uuid,
    ) -> Result<Option<ComponentCommands>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            application_id: Uuid,
            agent_id: Option<Uuid>,
            check_cmd: Option<String>,
            start_cmd: Option<String>,
            stop_cmd: Option<String>,
            integrity_check_cmd: Option<String>,
            infra_check_cmd: Option<String>,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT application_id, agent_id, check_cmd, start_cmd, stop_cmd, integrity_check_cmd, infra_check_cmd FROM components WHERE id = $1"
        ).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| ComponentCommands {
            application_id: r.application_id,
            agent_id: r.agent_id,
            check_cmd: r.check_cmd,
            start_cmd: r.start_cmd,
            stop_cmd: r.stop_cmd,
            integrity_check_cmd: r.integrity_check_cmd,
            infra_check_cmd: r.infra_check_cmd,
        }))
    }

    async fn get_custom_command(
        &self,
        component_id: Uuid,
        name: &str,
    ) -> Result<Option<CustomCommand>, sqlx::Error> {
        let row: Option<(Uuid, String, bool)> = sqlx::query_as(
            "SELECT id, command, requires_confirmation FROM component_commands WHERE component_id = $1 AND name = $2"
        ).bind(crate::db::bind_id(component_id)).bind(name).fetch_optional(&self.pool).await?;
        Ok(row.map(|(id, command, req)| CustomCommand {
            id,
            command,
            requires_confirmation: req,
        }))
    }

    async fn list_command_params(
        &self,
        command_id: Uuid,
    ) -> Result<Vec<CommandParam>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            command_id: Uuid,
            name: String,
            description: Option<String>,
            default_value: Option<String>,
            validation_regex: Option<String>,
            required: bool,
            display_order: i32,
            param_type: String,
            enum_values: Option<serde_json::Value>,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, command_id, name, description, default_value, validation_regex, required, display_order, param_type, enum_values, created_at FROM command_input_params WHERE command_id = $1 ORDER BY display_order, name"
        ).bind(command_id).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CommandParam {
                id: r.id,
                command_id: r.command_id,
                name: r.name,
                description: r.description,
                default_value: r.default_value,
                validation_regex: r.validation_regex,
                required: r.required,
                display_order: r.display_order,
                param_type: r.param_type,
                enum_values: r.enum_values,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn insert_command_execution(
        &self,
        request_id: Uuid,
        component_id: Uuid,
        agent_id: Uuid,
        command_type: &str,
        user_id: Uuid,
        command_text: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO command_executions (request_id, component_id, agent_id, command_type, status, user_id, command_text) VALUES ($1, $2, $3, $4, 'dispatched', $5, $6) ON CONFLICT (request_id) DO NOTHING")
            .bind(crate::db::bind_id(request_id)).bind(crate::db::bind_id(component_id)).bind(crate::db::bind_id(agent_id)).bind(command_type).bind(crate::db::bind_id(user_id)).bind(command_text)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_dependency_app_id(&self, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM dependencies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn list_custom_commands_raw(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<CustomCommandRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            component_id: Uuid,
            name: String,
            command: String,
            description: Option<String>,
            requires_confirmation: bool,
            min_permission_level: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, component_id, name, command, description, requires_confirmation, min_permission_level, created_at FROM component_commands WHERE component_id = $1 ORDER BY name"
        ).bind(crate::db::bind_id(component_id)).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CustomCommandRaw {
                id: r.id,
                component_id: r.component_id,
                name: r.name,
                command: r.command,
                description: r.description,
                requires_confirmation: r.requires_confirmation,
                min_permission_level: r.min_permission_level,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn list_command_executions(
        &self,
        component_id: Uuid,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CommandExecutionRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            request_id: Uuid,
            component_id: Uuid,
            agent_id: Option<Uuid>,
            command_type: String,
            exit_code: Option<i16>,
            stdout: Option<String>,
            stderr: Option<String>,
            duration_ms: Option<i32>,
            status: String,
            dispatched_at: chrono::DateTime<chrono::Utc>,
            completed_at: Option<chrono::DateTime<chrono::Utc>>,
        }
        let rows = if let Some(st) = status {
            sqlx::query_as::<_, Row>("SELECT id, request_id, component_id, agent_id, command_type, exit_code, stdout, stderr, duration_ms, status, dispatched_at, completed_at FROM command_executions WHERE component_id = $1 AND status = $2 ORDER BY dispatched_at DESC LIMIT $3 OFFSET $4")
                .bind(crate::db::bind_id(component_id)).bind(st).bind(limit).bind(offset).fetch_all(&self.pool).await?
        } else {
            sqlx::query_as::<_, Row>("SELECT id, request_id, component_id, agent_id, command_type, exit_code, stdout, stderr, duration_ms, status, dispatched_at, completed_at FROM command_executions WHERE component_id = $1 ORDER BY dispatched_at DESC LIMIT $2 OFFSET $3")
                .bind(crate::db::bind_id(component_id)).bind(limit).bind(offset).fetch_all(&self.pool).await?
        };
        Ok(rows
            .into_iter()
            .map(|r| CommandExecutionRaw {
                id: r.id,
                request_id: r.request_id,
                component_id: r.component_id,
                agent_id: r.agent_id,
                command_type: r.command_type,
                exit_code: r.exit_code,
                stdout: r.stdout,
                stderr: r.stderr,
                duration_ms: r.duration_ms,
                status: r.status,
                dispatched_at: r.dispatched_at,
                completed_at: r.completed_at,
            })
            .collect())
    }

    async fn list_state_transitions(
        &self,
        component_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StateTransitionRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            component_id: Uuid,
            from_state: String,
            to_state: String,
            trigger: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, component_id, from_state, to_state, trigger, created_at FROM state_transitions WHERE component_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(crate::db::bind_id(component_id)).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| StateTransitionRaw {
                id: r.id,
                component_id: r.component_id,
                from_state: r.from_state,
                to_state: r.to_state,
                trigger: r.trigger,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn list_check_events(
        &self,
        component_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CheckEventRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            component_id: Uuid,
            check_type: String,
            exit_code: i16,
            stdout: Option<String>,
            duration_ms: i32,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, component_id, check_type, exit_code, stdout, duration_ms, created_at FROM check_events WHERE component_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(crate::db::bind_id(component_id)).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CheckEventRaw {
                id: r.id,
                component_id: r.component_id,
                check_type: r.check_type,
                exit_code: r.exit_code,
                stdout: r.stdout,
                duration_ms: r.duration_ms,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn resolve_host_to_agent(&self, host: &str) -> Result<Option<Uuid>, sqlx::Error> {
        let by_hostname: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agents WHERE hostname = $1 AND is_active = true ORDER BY created_at LIMIT 1"
        ).bind(host).fetch_optional(&self.pool).await?;
        if by_hostname.is_some() {
            return Ok(by_hostname);
        }
        sqlx::query_scalar(
            "SELECT id FROM agents WHERE ip_addresses ? $1 AND is_active = true ORDER BY created_at LIMIT 1"
        ).bind(host).fetch_optional(&self.pool).await
    }

    async fn batch_update_positions(
        &self,
        app_id: Uuid,
        positions: &[(Uuid, f32, f32)],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let sql = format!("UPDATE components SET position_x = $2, position_y = $3, updated_at = {} WHERE id = $1 AND application_id = $4", crate::db::sql::now());
        for &(id, x, y) in positions {
            sqlx::query(&sql)
                .bind(id)
                .bind(x)
                .bind(y)
                .bind(crate::db::bind_id(app_id))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
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
        sqlx::query("INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot) VALUES ($1, $2, $3, $4::jsonb, $5::jsonb)")
            .bind(resource_type).bind(crate::db::bind_id(resource_id)).bind(changed_by).bind(before_snapshot).bind(after_snapshot)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn auto_bind_agent(
        &self,
        agent_id: Uuid,
        hostname: &str,
        ip_addresses: &[String],
    ) -> Result<u64, sqlx::Error> {
        let mut total = 0u64;
        let result =
            sqlx::query("UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL")
                .bind(crate::db::bind_id(agent_id))
                .bind(hostname)
                .execute(&self.pool)
                .await?;
        total += result.rows_affected();
        for ip in ip_addresses {
            let result = sqlx::query(
                "UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL",
            )
            .bind(crate::db::bind_id(agent_id))
            .bind(ip)
            .execute(&self.pool)
            .await?;
            total += result.rows_affected();
        }
        Ok(total)
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
    cluster_mode: Option<String>,
    cluster_health_policy: Option<String>,
    cluster_min_healthy_pct: Option<i16>,
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
            cluster_mode: r.cluster_mode,
            cluster_health_policy: r.cluster_health_policy,
            cluster_min_healthy_pct: r.cluster_min_healthy_pct,
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

    async fn get_component(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<Component>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteComponentRow>(
            "SELECT c.id, c.application_id, c.name, c.component_type, c.display_name, c.description, c.icon, c.group_id, \
                c.host, c.agent_id, c.check_cmd, c.start_cmd, c.stop_cmd, \
                c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds, c.is_optional, \
                c.position_x, c.position_y, c.cluster_size, c.cluster_nodes, c.cluster_mode, c.cluster_health_policy, c.cluster_min_healthy_pct, c.referenced_app_id, c.created_at, c.updated_at \
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
        let row =
            sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
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

    async fn get_component_refs(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, Option<Uuid>)>, sqlx::Error> {
        let row: Option<(DbUuid, Option<DbUuid>)> = sqlx::query_as(
            "SELECT application_id, referenced_app_id FROM components WHERE id = $1",
        )
        .bind(DbUuid::from(id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(app, r)| (app.into_inner(), r.map(|x| x.into_inner()))))
    }

    async fn get_component_commands(
        &self,
        id: Uuid,
    ) -> Result<Option<ComponentCommands>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            application_id: DbUuid,
            agent_id: Option<DbUuid>,
            check_cmd: Option<String>,
            start_cmd: Option<String>,
            stop_cmd: Option<String>,
            integrity_check_cmd: Option<String>,
            infra_check_cmd: Option<String>,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT application_id, agent_id, check_cmd, start_cmd, stop_cmd, integrity_check_cmd, infra_check_cmd FROM components WHERE id = $1"
        ).bind(DbUuid::from(id)).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| ComponentCommands {
            application_id: r.application_id.into_inner(),
            agent_id: r.agent_id.map(|a| a.into_inner()),
            check_cmd: r.check_cmd,
            start_cmd: r.start_cmd,
            stop_cmd: r.stop_cmd,
            integrity_check_cmd: r.integrity_check_cmd,
            infra_check_cmd: r.infra_check_cmd,
        }))
    }

    async fn get_custom_command(
        &self,
        component_id: Uuid,
        name: &str,
    ) -> Result<Option<CustomCommand>, sqlx::Error> {
        let row: Option<(DbUuid, String, bool)> = sqlx::query_as(
            "SELECT id, command, requires_confirmation FROM component_commands WHERE component_id = $1 AND name = $2"
        ).bind(DbUuid::from(component_id)).bind(name).fetch_optional(&self.pool).await?;
        Ok(row.map(|(id, command, req)| CustomCommand {
            id: id.into_inner(),
            command,
            requires_confirmation: req,
        }))
    }

    async fn list_command_params(
        &self,
        command_id: Uuid,
    ) -> Result<Vec<CommandParam>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            command_id: DbUuid,
            name: String,
            description: Option<String>,
            default_value: Option<String>,
            validation_regex: Option<String>,
            required: bool,
            display_order: i32,
            param_type: String,
            enum_values: Option<serde_json::Value>,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, command_id, name, description, default_value, validation_regex, required, display_order, param_type, enum_values, created_at FROM command_input_params WHERE command_id = $1 ORDER BY display_order, name"
        ).bind(DbUuid::from(command_id)).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CommandParam {
                id: r.id.into_inner(),
                command_id: r.command_id.into_inner(),
                name: r.name,
                description: r.description,
                default_value: r.default_value,
                validation_regex: r.validation_regex,
                required: r.required,
                display_order: r.display_order,
                param_type: r.param_type,
                enum_values: r.enum_values,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn insert_command_execution(
        &self,
        request_id: Uuid,
        component_id: Uuid,
        agent_id: Uuid,
        command_type: &str,
        user_id: Uuid,
        command_text: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO command_executions (id, request_id, component_id, agent_id, command_type, status, user_id, command_text) VALUES ($1, $2, $3, $4, $5, 'dispatched', $6, $7) ON CONFLICT (request_id) DO NOTHING")
            .bind(DbUuid::from(Uuid::new_v4())).bind(DbUuid::from(request_id)).bind(DbUuid::from(component_id)).bind(DbUuid::from(agent_id)).bind(command_type).bind(DbUuid::from(user_id)).bind(command_text)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_dependency_app_id(&self, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT application_id FROM dependencies WHERE id = $1",
        )
        .bind(DbUuid::from(id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }

    async fn list_custom_commands_raw(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<CustomCommandRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            component_id: DbUuid,
            name: String,
            command: String,
            description: Option<String>,
            requires_confirmation: bool,
            min_permission_level: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, component_id, name, command, description, requires_confirmation, min_permission_level, created_at FROM component_commands WHERE component_id = $1 ORDER BY name"
        ).bind(DbUuid::from(component_id)).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CustomCommandRaw {
                id: r.id.into_inner(),
                component_id: r.component_id.into_inner(),
                name: r.name,
                command: r.command,
                description: r.description,
                requires_confirmation: r.requires_confirmation,
                min_permission_level: r.min_permission_level,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn list_command_executions(
        &self,
        component_id: Uuid,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CommandExecutionRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            request_id: DbUuid,
            component_id: DbUuid,
            agent_id: Option<DbUuid>,
            command_type: String,
            exit_code: Option<i16>,
            stdout: Option<String>,
            stderr: Option<String>,
            duration_ms: Option<i32>,
            status: String,
            dispatched_at: chrono::DateTime<chrono::Utc>,
            completed_at: Option<chrono::DateTime<chrono::Utc>>,
        }
        let rows = if let Some(st) = status {
            sqlx::query_as::<_, Row>("SELECT id, request_id, component_id, agent_id, command_type, exit_code, stdout, stderr, duration_ms, status, dispatched_at, completed_at FROM command_executions WHERE component_id = $1 AND status = $2 ORDER BY dispatched_at DESC LIMIT $3 OFFSET $4")
                .bind(DbUuid::from(component_id)).bind(st).bind(limit).bind(offset).fetch_all(&self.pool).await?
        } else {
            sqlx::query_as::<_, Row>("SELECT id, request_id, component_id, agent_id, command_type, exit_code, stdout, stderr, duration_ms, status, dispatched_at, completed_at FROM command_executions WHERE component_id = $1 ORDER BY dispatched_at DESC LIMIT $2 OFFSET $3")
                .bind(DbUuid::from(component_id)).bind(limit).bind(offset).fetch_all(&self.pool).await?
        };
        Ok(rows
            .into_iter()
            .map(|r| CommandExecutionRaw {
                id: r.id.into_inner(),
                request_id: r.request_id.into_inner(),
                component_id: r.component_id.into_inner(),
                agent_id: r.agent_id.map(|a| a.into_inner()),
                command_type: r.command_type,
                exit_code: r.exit_code,
                stdout: r.stdout,
                stderr: r.stderr,
                duration_ms: r.duration_ms,
                status: r.status,
                dispatched_at: r.dispatched_at,
                completed_at: r.completed_at,
            })
            .collect())
    }

    async fn list_state_transitions(
        &self,
        component_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StateTransitionRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            component_id: DbUuid,
            from_state: String,
            to_state: String,
            trigger: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, component_id, from_state, to_state, trigger, created_at FROM state_transitions WHERE component_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(DbUuid::from(component_id)).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| StateTransitionRaw {
                id: r.id.into_inner(),
                component_id: r.component_id.into_inner(),
                from_state: r.from_state,
                to_state: r.to_state,
                trigger: r.trigger,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn list_check_events(
        &self,
        component_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CheckEventRaw>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            component_id: DbUuid,
            check_type: String,
            exit_code: i16,
            stdout: Option<String>,
            duration_ms: i32,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, component_id, check_type, exit_code, stdout, duration_ms, created_at FROM check_events WHERE component_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(DbUuid::from(component_id)).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| CheckEventRaw {
                id: r.id,
                component_id: r.component_id.into_inner(),
                check_type: r.check_type,
                exit_code: r.exit_code,
                stdout: r.stdout,
                duration_ms: r.duration_ms,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn resolve_host_to_agent(&self, host: &str) -> Result<Option<Uuid>, sqlx::Error> {
        let by_hostname = sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM agents WHERE hostname = $1 AND is_active = 1 ORDER BY created_at LIMIT 1"
        ).bind(host).fetch_optional(&self.pool).await?;
        if let Some(id) = by_hostname {
            return Ok(Some(id.into_inner()));
        }
        let by_ip = sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM agents WHERE EXISTS(SELECT 1 FROM json_each(ip_addresses) WHERE value = $1) AND is_active = 1 ORDER BY created_at LIMIT 1"
        ).bind(host).fetch_optional(&self.pool).await?;
        Ok(by_ip.map(|x| x.into_inner()))
    }

    async fn batch_update_positions(
        &self,
        app_id: Uuid,
        positions: &[(Uuid, f32, f32)],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let sql = format!("UPDATE components SET position_x = $2, position_y = $3, updated_at = {} WHERE id = $1 AND application_id = $4", crate::db::sql::now());
        for &(id, x, y) in positions {
            sqlx::query(&sql)
                .bind(DbUuid::from(id))
                .bind(x)
                .bind(y)
                .bind(DbUuid::from(app_id))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
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
        sqlx::query("INSERT INTO config_versions (id, resource_type, resource_id, changed_by, before_snapshot, after_snapshot) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(DbUuid::new_v4()).bind(resource_type).bind(DbUuid::from(resource_id)).bind(DbUuid::from(changed_by)).bind(before_snapshot).bind(after_snapshot)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn auto_bind_agent(
        &self,
        agent_id: Uuid,
        hostname: &str,
        ip_addresses: &[String],
    ) -> Result<u64, sqlx::Error> {
        let mut total = 0u64;
        let result =
            sqlx::query("UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL")
                .bind(DbUuid::from(agent_id))
                .bind(hostname)
                .execute(&self.pool)
                .await?;
        total += result.rows_affected();
        for ip in ip_addresses {
            let result = sqlx::query(
                "UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL",
            )
            .bind(DbUuid::from(agent_id))
            .bind(ip)
            .execute(&self.pool)
            .await?;
            total += result.rows_affected();
        }
        Ok(total)
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
