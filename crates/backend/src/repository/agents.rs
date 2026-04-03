//! Agent repository — all agent-related database queries.

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct Agent {
    pub id: Uuid,
    pub hostname: String,
    pub organization_id: Uuid,
    pub gateway_id: Option<Uuid>,
    pub labels: Value,
    pub ip_addresses: Value,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct AgentListItem {
    pub id: Uuid,
    pub hostname: String,
    pub organization_id: Uuid,
    pub gateway_id: Option<Uuid>,
    pub labels: Value,
    pub ip_addresses: Value,
    pub version: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub cpu_arch: Option<String>,
    pub cpu_cores: Option<i32>,
    pub total_memory_mb: Option<i64>,
    pub disk_total_gb: Option<i64>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub gateway_name: Option<String>,
    pub gateway_zone: Option<String>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait AgentRepository: Send + Sync {
    /// List agents with gateway info for an organization.
    async fn list_agents(&self, org_id: Uuid) -> Result<Vec<AgentListItem>, sqlx::Error>;

    /// Get a single agent by ID.
    async fn get_agent(&self, id: Uuid, org_id: Uuid) -> Result<Option<Agent>, sqlx::Error>;

    /// Get agent hostname and gateway_id (for block operations).
    async fn get_agent_info(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<(String, Option<Uuid>)>, sqlx::Error>;

    /// Block an agent (deactivate + clear gateway).
    async fn block_agent(&self, id: Uuid) -> Result<(), sqlx::Error>;

    /// Unblock an agent (reactivate).
    async fn unblock_agent(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgAgentRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgAgentRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgAgentListRow {
    id: Uuid,
    hostname: String,
    organization_id: Uuid,
    gateway_id: Option<Uuid>,
    labels: Value,
    ip_addresses: Value,
    version: Option<String>,
    os_name: Option<String>,
    os_version: Option<String>,
    cpu_arch: Option<String>,
    cpu_cores: Option<i32>,
    total_memory_mb: Option<i64>,
    disk_total_gb: Option<i64>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    gateway_name: Option<String>,
    gateway_zone: Option<String>,
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgAgentRow {
    id: Uuid,
    hostname: String,
    organization_id: Uuid,
    gateway_id: Option<Uuid>,
    labels: Value,
    ip_addresses: Value,
    version: Option<String>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
#[async_trait]
impl AgentRepository for PgAgentRepository {
    async fn list_agents(&self, org_id: Uuid) -> Result<Vec<AgentListItem>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgAgentListRow>(
            r#"SELECT a.id, a.hostname, a.organization_id, a.gateway_id, a.labels, a.ip_addresses,
                   a.version, a.os_name, a.os_version, a.cpu_arch, a.cpu_cores,
                   a.total_memory_mb, a.disk_total_gb,
                   a.last_heartbeat_at, a.is_active, a.created_at,
                   g.name as gateway_name, g.zone as gateway_zone
            FROM agents a
            LEFT JOIN gateways g ON a.gateway_id = g.id
            WHERE a.organization_id = $1
            ORDER BY a.hostname"#,
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AgentListItem {
                id: r.id,
                hostname: r.hostname,
                organization_id: r.organization_id,
                gateway_id: r.gateway_id,
                labels: r.labels,
                ip_addresses: r.ip_addresses,
                version: r.version,
                os_name: r.os_name,
                os_version: r.os_version,
                cpu_arch: r.cpu_arch,
                cpu_cores: r.cpu_cores,
                total_memory_mb: r.total_memory_mb,
                disk_total_gb: r.disk_total_gb,
                last_heartbeat_at: r.last_heartbeat_at,
                is_active: r.is_active,
                created_at: r.created_at,
                gateway_name: r.gateway_name,
                gateway_zone: r.gateway_zone,
            })
            .collect())
    }

    async fn get_agent(&self, id: Uuid, org_id: Uuid) -> Result<Option<Agent>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgAgentRow>(
            "SELECT id, hostname, organization_id, gateway_id, labels, ip_addresses, \
             version, last_heartbeat_at, is_active, created_at \
             FROM agents WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Agent {
            id: r.id,
            hostname: r.hostname,
            organization_id: r.organization_id,
            gateway_id: r.gateway_id,
            labels: r.labels,
            ip_addresses: r.ip_addresses,
            version: r.version,
            last_heartbeat_at: r.last_heartbeat_at,
            is_active: r.is_active,
            created_at: r.created_at,
        }))
    }

    async fn get_agent_info(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<(String, Option<Uuid>)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT hostname, gateway_id FROM agents WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn block_agent(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE agents SET is_active = false, gateway_id = NULL WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn unblock_agent(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE agents SET is_active = true WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteAgentRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteAgentRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteAgentListRow {
    id: DbUuid,
    hostname: String,
    organization_id: DbUuid,
    gateway_id: Option<DbUuid>,
    labels: Value,
    ip_addresses: Value,
    version: Option<String>,
    os_name: Option<String>,
    os_version: Option<String>,
    cpu_arch: Option<String>,
    cpu_cores: Option<i32>,
    total_memory_mb: Option<i64>,
    disk_total_gb: Option<i64>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    gateway_name: Option<String>,
    gateway_zone: Option<String>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteAgentRow {
    id: DbUuid,
    hostname: String,
    organization_id: DbUuid,
    gateway_id: Option<DbUuid>,
    labels: Value,
    ip_addresses: Value,
    version: Option<String>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl AgentRepository for SqliteAgentRepository {
    async fn list_agents(&self, org_id: Uuid) -> Result<Vec<AgentListItem>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteAgentListRow>(
            r#"SELECT a.id, a.hostname, a.organization_id, a.gateway_id, a.labels, a.ip_addresses,
                   a.version, a.os_name, a.os_version, a.cpu_arch, a.cpu_cores,
                   a.total_memory_mb, a.disk_total_gb,
                   a.last_heartbeat_at, a.is_active, a.created_at,
                   g.name as gateway_name, g.zone as gateway_zone
            FROM agents a
            LEFT JOIN gateways g ON a.gateway_id = g.id
            WHERE a.organization_id = $1
            ORDER BY a.hostname"#,
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AgentListItem {
                id: r.id.into_inner(),
                hostname: r.hostname,
                organization_id: r.organization_id.into_inner(),
                gateway_id: r.gateway_id.map(|g| g.into_inner()),
                labels: r.labels,
                ip_addresses: r.ip_addresses,
                version: r.version,
                os_name: r.os_name,
                os_version: r.os_version,
                cpu_arch: r.cpu_arch,
                cpu_cores: r.cpu_cores,
                total_memory_mb: r.total_memory_mb,
                disk_total_gb: r.disk_total_gb,
                last_heartbeat_at: r.last_heartbeat_at,
                is_active: r.is_active,
                created_at: r.created_at,
                gateway_name: r.gateway_name,
                gateway_zone: r.gateway_zone,
            })
            .collect())
    }

    async fn get_agent(&self, id: Uuid, org_id: Uuid) -> Result<Option<Agent>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteAgentRow>(
            "SELECT id, hostname, organization_id, gateway_id, labels, ip_addresses, \
             version, last_heartbeat_at, is_active, created_at \
             FROM agents WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Agent {
            id: r.id.into_inner(),
            hostname: r.hostname,
            organization_id: r.organization_id.into_inner(),
            gateway_id: r.gateway_id.map(|g| g.into_inner()),
            labels: r.labels,
            ip_addresses: r.ip_addresses,
            version: r.version,
            last_heartbeat_at: r.last_heartbeat_at,
            is_active: r.is_active,
            created_at: r.created_at,
        }))
    }

    async fn get_agent_info(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<(String, Option<Uuid>)>, sqlx::Error> {
        let row: Option<(String, Option<DbUuid>)> = sqlx::query_as(
            "SELECT hostname, gateway_id FROM agents WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(h, g)| (h, g.map(|g| g.into_inner()))))
    }

    async fn block_agent(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE agents SET is_active = 0, gateway_id = NULL WHERE id = $1")
            .bind(DbUuid::from(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn unblock_agent(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE agents SET is_active = 1 WHERE id = $1")
            .bind(DbUuid::from(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_agent_repository(pool: DbPool) -> Box<dyn AgentRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgAgentRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteAgentRepository::new(pool))
    }
}

// ============================================================================
// Free functions (api/agents.rs queries)
// ============================================================================

/// Insert a state transition to UNREACHABLE when an agent is blocked.
pub async fn insert_unreachable_transition(
    pool: &DbPool,
    component_id: Uuid,
    from_state: &str,
    details_json: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
           VALUES ($1, $2, 'UNREACHABLE', 'agent_blocked', $3)"#,
    )
    .bind(component_id)
    .bind(from_state)
    .bind(details_json)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger, details)
           VALUES ($1, $2, $3, 'UNREACHABLE', 'agent_blocked', $4)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(component_id))
    .bind(from_state)
    .bind(serde_json::to_string(details_json).unwrap_or_default())
    .execute(pool)
    .await?;

    Ok(())
}

/// Log a block event in certificate_events.
pub async fn log_agent_block_event(pool: &DbPool, agent_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           SELECT $1, 'blocked', certificate_fingerprint, certificate_cn
           FROM agents WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(agent_id))
    .execute(pool)
    .await
    .ok();
}

/// Delete an agent and its related records.
pub async fn delete_agent_cascade(pool: &DbPool, agent_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE components SET agent_id = NULL WHERE agent_id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM discovery_reports WHERE agent_id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM certificate_events WHERE agent_id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM binding_profile_mappings WHERE agent_id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Check if an agent exists in a given organization.
pub async fn agent_exists_in_org(
    pool: &DbPool,
    agent_id: Uuid,
    org_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row: Option<(DbUuid,)> =
        sqlx::query_as("SELECT id FROM agents WHERE id = $1 AND organization_id = $2")
            .bind(crate::db::bind_id(agent_id))
            .bind(crate::db::bind_id(org_id))
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// Get agent with hostname, checking org membership.
pub async fn get_agent_in_org(
    pool: &DbPool,
    agent_id: Uuid,
    org_id: Uuid,
) -> Result<Option<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as("SELECT id, hostname FROM agents WHERE id = $1 AND organization_id = $2")
        .bind(crate::db::bind_id(agent_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await
}

/// Get components for an agent (for transitioning to UNREACHABLE).
pub async fn get_agent_components<
    T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
>(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        r#"
        SELECT c.id, c.name, c.application_id, a.name AS app_name
        FROM components c
        JOIN applications a ON c.application_id = a.id
        WHERE c.agent_id = $1
        "#,
    )
    .bind(crate::db::bind_id(agent_id))
    .fetch_all(pool)
    .await
}

/// Fetch agent metrics (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_agent_metrics<
    T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
>(
    pool: &DbPool,
    agent_id: Uuid,
    minutes: i32,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(&format!(
        "SELECT cpu_pct, memory_pct, disk_used_pct, created_at
             FROM agent_metrics
             WHERE agent_id = $1 AND created_at > {} - ($2 || ' minutes')::interval
             ORDER BY created_at ASC",
        crate::db::sql::now()
    ))
    .bind(crate::db::bind_id(agent_id))
    .bind(minutes)
    .fetch_all(pool)
    .await
}

/// Fetch agent metrics (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_agent_metrics<
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
>(
    pool: &DbPool,
    agent_id: Uuid,
    minutes: i32,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        "SELECT cpu_pct, memory_pct, disk_used_pct, created_at
             FROM agent_metrics
             WHERE agent_id = $1 AND created_at > datetime('now', '-' || $2 || ' minutes')
             ORDER BY created_at ASC",
    )
    .bind(crate::db::bind_id(agent_id))
    .bind(minutes)
    .fetch_all(pool)
    .await
}

/// Verify agents belong to organization (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn verify_agents_in_org(
    pool: &DbPool,
    agent_ids: &[Uuid],
    org_id: Uuid,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as("SELECT id, hostname FROM agents WHERE id = ANY($1) AND organization_id = $2")
        .bind(agent_ids)
        .bind(org_id)
        .fetch_all(pool)
        .await
}

/// Verify agents belong to organization (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn verify_agents_in_org(
    pool: &DbPool,
    agent_ids: &[Uuid],
    org_id: Uuid,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=agent_ids.len()).map(|i| format!("${}", i)).collect();
    let org_placeholder = format!("${}", agent_ids.len() + 1);
    let query = format!(
        "SELECT id, hostname FROM agents WHERE id IN ({}) AND organization_id = {}",
        placeholders.join(", "),
        org_placeholder
    );
    let mut q = sqlx::query_as::<_, (String, String)>(&query);
    for id in agent_ids {
        q = q.bind(id.to_string());
    }
    q = q.bind(org_id.to_string());
    let rows: Vec<(String, String)> = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id_str, hostname)| Uuid::parse_str(&id_str).ok().map(|id| (id, hostname)))
        .collect())
}

/// Delete all agent-related records in a transaction (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn bulk_delete_agent_records<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    agent_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE components SET agent_id = NULL WHERE agent_id = ANY($1)")
        .bind(agent_ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM discovery_reports WHERE agent_id = ANY($1)")
        .bind(agent_ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM certificate_events WHERE agent_id = ANY($1)")
        .bind(agent_ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM binding_profile_mappings WHERE agent_id = ANY($1)")
        .bind(agent_ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM agents WHERE id = ANY($1)")
        .bind(agent_ids)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Delete all agent-related records in a transaction (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn bulk_delete_agent_records<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    agent_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    for agent_id in agent_ids {
        let id_str = agent_id.to_string();
        sqlx::query("UPDATE components SET agent_id = NULL WHERE agent_id = $1")
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM discovery_reports WHERE agent_id = $1")
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM certificate_events WHERE agent_id = $1")
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM binding_profile_mappings WHERE agent_id = $1")
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
