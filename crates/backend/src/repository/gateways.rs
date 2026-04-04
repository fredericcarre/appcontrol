//! Gateway repository — all gateway-related database queries.

use async_trait::async_trait;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct Gateway {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub zone: Option<String>,
    pub hostname: Option<String>,
    pub port: Option<i32>,
    pub site_id: Option<Uuid>,
    pub certificate_fingerprint: Option<String>,
    pub is_active: bool,
    pub is_primary: bool,
    pub priority: i32,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Gateway list item with additional computed fields.
#[derive(Debug)]
pub struct GatewayListRow {
    pub id: Uuid,
    pub name: String,
    pub zone: Option<String>,
    pub is_active: bool,
    pub is_primary: bool,
    pub priority: i32,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub agent_count: i64,
    pub site_id: Option<Uuid>,
    pub site_name: Option<String>,
    pub site_code: Option<String>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait GatewayRepository: Send + Sync {
    /// List gateways with site info for an organization.
    async fn list_gateways(&self, org_id: Uuid) -> Result<Vec<GatewayListRow>, sqlx::Error>;

    /// Get a single gateway by ID.
    async fn get_gateway(&self, id: Uuid, org_id: Uuid) -> Result<Option<Gateway>, sqlx::Error>;

    /// Update gateway fields (name, site_id, is_active, is_primary, priority).
    async fn update_gateway(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        site_id: Option<Uuid>,
        is_active: Option<bool>,
        is_primary: Option<bool>,
        priority: Option<i32>,
    ) -> Result<Option<Gateway>, sqlx::Error>;

    /// Check if a site exists for the org.
    async fn site_exists(&self, site_id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error>;

    /// Unset primary for all gateways in a site (except given gateway).
    async fn unset_primary_in_site(
        &self,
        org_id: Uuid,
        site_id: Uuid,
        except_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    /// Get gateway site_id and zone for primary election.
    async fn get_gateway_site_info(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<(Option<Uuid>, String)>, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgGatewayRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgGatewayRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgGatewayRow {
    id: Uuid,
    organization_id: Uuid,
    name: String,
    zone: Option<String>,
    hostname: Option<String>,
    port: Option<i32>,
    site_id: Option<Uuid>,
    certificate_fingerprint: Option<String>,
    is_active: bool,
    is_primary: bool,
    priority: i32,
    version: Option<String>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
impl From<PgGatewayRow> for Gateway {
    fn from(r: PgGatewayRow) -> Self {
        Gateway {
            id: r.id,
            organization_id: r.organization_id,
            name: r.name,
            zone: r.zone,
            hostname: r.hostname,
            port: r.port,
            site_id: r.site_id,
            certificate_fingerprint: r.certificate_fingerprint,
            is_active: r.is_active,
            is_primary: r.is_primary,
            priority: r.priority,
            version: r.version,
            last_heartbeat_at: r.last_heartbeat_at,
            created_at: r.created_at,
        }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl GatewayRepository for PgGatewayRepository {
    async fn list_gateways(&self, org_id: Uuid) -> Result<Vec<GatewayListRow>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<String>,
                bool,
                bool,
                i32,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                i64,
                Option<Uuid>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"SELECT
                   g.id, g.name, g.zone, g.is_active,
                   COALESCE(g.is_primary, false) as is_primary,
                   COALESCE(g.priority, 0) as priority,
                   g.version, g.last_heartbeat_at,
                   COALESCE((SELECT COUNT(*) FROM agents a WHERE a.gateway_id = g.id), 0) as agent_count,
                   g.site_id, s.name as site_name, s.code as site_code
               FROM gateways g
               LEFT JOIN sites s ON s.id = g.site_id
               WHERE g.organization_id = $1
               ORDER BY COALESCE(s.name, 'zzz'), g.priority, g.name"#,
        )
        .bind(crate::db::bind_id(org_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    zone,
                    is_active,
                    is_primary,
                    priority,
                    version,
                    last_heartbeat,
                    agent_count,
                    site_id,
                    site_name,
                    site_code,
                )| {
                    GatewayListRow {
                        id,
                        name,
                        zone,
                        is_active,
                        is_primary,
                        priority,
                        version,
                        last_heartbeat_at: last_heartbeat,
                        agent_count,
                        site_id,
                        site_name,
                        site_code,
                    }
                },
            )
            .collect())
    }

    async fn get_gateway(&self, id: Uuid, org_id: Uuid) -> Result<Option<Gateway>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgGatewayRow>(
            r#"SELECT id, organization_id, name, zone, hostname, port, site_id,
                      certificate_fingerprint, is_active,
                      COALESCE(is_primary, false) as is_primary,
                      COALESCE(priority, 0) as priority,
                      version, last_heartbeat_at, created_at
               FROM gateways WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(id)
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn update_gateway(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        site_id: Option<Uuid>,
        is_active: Option<bool>,
        is_primary: Option<bool>,
        priority: Option<i32>,
    ) -> Result<Option<Gateway>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgGatewayRow>(
            r#"UPDATE gateways SET
                   name = COALESCE($3, name),
                   site_id = COALESCE($4, site_id),
                   is_active = COALESCE($5, is_active),
                   is_primary = COALESCE($6, is_primary),
                   priority = COALESCE($7, priority)
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, zone, hostname, port, site_id,
                         certificate_fingerprint, is_active,
                         COALESCE(is_primary, false) as is_primary,
                         COALESCE(priority, 0) as priority,
                         version, last_heartbeat_at, created_at"#,
        )
        .bind(id)
        .bind(crate::db::bind_id(org_id))
        .bind(name)
        .bind(crate::db::bind_opt_id(site_id))
        .bind(is_active)
        .bind(is_primary)
        .bind(priority)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn site_exists(&self, site_id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE id = $1 AND organization_id = $2)",
        )
        .bind(crate::db::bind_id(site_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_one(&self.pool)
        .await
    }

    async fn unset_primary_in_site(
        &self,
        org_id: Uuid,
        site_id: Uuid,
        except_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE gateways SET is_primary = false \
             WHERE organization_id = $1 AND site_id = $2 AND id != $3",
        )
        .bind(crate::db::bind_id(org_id))
        .bind(crate::db::bind_id(site_id))
        .bind(except_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_gateway_site_info(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<(Option<Uuid>, String)>, sqlx::Error> {
        sqlx::query_as("SELECT site_id, zone FROM gateways WHERE id = $1 AND organization_id = $2")
            .bind(id)
            .bind(crate::db::bind_id(org_id))
            .fetch_optional(&self.pool)
            .await
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteGatewayRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteGatewayRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteGatewayRow {
    id: DbUuid,
    organization_id: DbUuid,
    name: String,
    zone: Option<String>,
    hostname: Option<String>,
    port: Option<i32>,
    site_id: Option<DbUuid>,
    certificate_fingerprint: Option<String>,
    is_active: bool,
    is_primary: bool,
    priority: i32,
    version: Option<String>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl From<SqliteGatewayRow> for Gateway {
    fn from(r: SqliteGatewayRow) -> Self {
        Gateway {
            id: r.id.into_inner(),
            organization_id: r.organization_id.into_inner(),
            name: r.name,
            zone: r.zone,
            hostname: r.hostname,
            port: r.port,
            site_id: r.site_id.map(|s| s.into_inner()),
            certificate_fingerprint: r.certificate_fingerprint,
            is_active: r.is_active,
            is_primary: r.is_primary,
            priority: r.priority,
            version: r.version,
            last_heartbeat_at: r.last_heartbeat_at,
            created_at: r.created_at,
        }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl GatewayRepository for SqliteGatewayRepository {
    async fn list_gateways(&self, org_id: Uuid) -> Result<Vec<GatewayListRow>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                Option<String>,
                bool,
                bool,
                i32,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                i64,
                Option<DbUuid>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"SELECT
                   g.id, g.name, g.zone, g.is_active,
                   COALESCE(g.is_primary, 0) as is_primary,
                   COALESCE(g.priority, 0) as priority,
                   g.version, g.last_heartbeat_at,
                   COALESCE((SELECT COUNT(*) FROM agents a WHERE a.gateway_id = g.id), 0) as agent_count,
                   g.site_id, s.name as site_name, s.code as site_code
               FROM gateways g
               LEFT JOIN sites s ON s.id = g.site_id
               WHERE g.organization_id = $1
               ORDER BY COALESCE(s.name, 'zzz'), g.priority, g.name"#,
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    zone,
                    is_active,
                    is_primary,
                    priority,
                    version,
                    last_heartbeat,
                    agent_count,
                    site_id,
                    site_name,
                    site_code,
                )| {
                    GatewayListRow {
                        id: id.into_inner(),
                        name,
                        zone,
                        is_active,
                        is_primary,
                        priority,
                        version,
                        last_heartbeat_at: last_heartbeat,
                        agent_count,
                        site_id: site_id.map(|s| s.into_inner()),
                        site_name,
                        site_code,
                    }
                },
            )
            .collect())
    }

    async fn get_gateway(&self, id: Uuid, org_id: Uuid) -> Result<Option<Gateway>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteGatewayRow>(
            r#"SELECT id, organization_id, name, zone, hostname, port, site_id,
                      certificate_fingerprint, is_active,
                      COALESCE(is_primary, 0) as is_primary,
                      COALESCE(priority, 0) as priority,
                      version, last_heartbeat_at, created_at
               FROM gateways WHERE id = $1 AND organization_id = $2"#,
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn update_gateway(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: Option<&str>,
        site_id: Option<Uuid>,
        is_active: Option<bool>,
        is_primary: Option<bool>,
        priority: Option<i32>,
    ) -> Result<Option<Gateway>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteGatewayRow>(
            r#"UPDATE gateways SET
                   name = COALESCE($3, name),
                   site_id = COALESCE($4, site_id),
                   is_active = COALESCE($5, is_active),
                   is_primary = COALESCE($6, is_primary),
                   priority = COALESCE($7, priority)
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, zone, hostname, port, site_id,
                         certificate_fingerprint, is_active,
                         COALESCE(is_primary, 0) as is_primary,
                         COALESCE(priority, 0) as priority,
                         version, last_heartbeat_at, created_at"#,
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(site_id.map(DbUuid::from))
        .bind(is_active)
        .bind(is_primary)
        .bind(priority)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn site_exists(&self, site_id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sites WHERE id = $1 AND organization_id = $2")
                .bind(DbUuid::from(site_id))
                .bind(DbUuid::from(org_id))
                .fetch_one(&self.pool)
                .await?;
        Ok(count > 0)
    }

    async fn unset_primary_in_site(
        &self,
        org_id: Uuid,
        site_id: Uuid,
        except_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE gateways SET is_primary = 0 \
             WHERE organization_id = $1 AND site_id = $2 AND id != $3",
        )
        .bind(DbUuid::from(org_id))
        .bind(DbUuid::from(site_id))
        .bind(DbUuid::from(except_id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_gateway_site_info(
        &self,
        id: Uuid,
        org_id: Uuid,
    ) -> Result<Option<(Option<Uuid>, String)>, sqlx::Error> {
        let row: Option<(Option<DbUuid>, String)> = sqlx::query_as(
            "SELECT site_id, zone FROM gateways WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(s, z)| (s.map(|s| s.into_inner()), z)))
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_gateway_repository(pool: DbPool) -> Box<dyn GatewayRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgGatewayRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteGatewayRepository::new(pool))
    }
}
