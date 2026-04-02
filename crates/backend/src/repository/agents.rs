//! Agent repository — all agent-related database queries.

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

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
