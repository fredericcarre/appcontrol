//! Query functions for gateway domain. All sqlx queries live here.
//!
//! These handle gateway API queries that aren't part of the GatewayRepository trait
//! (which handles list/get/create). Many of these involve transactions or
//! security-related operations.

#![allow(unused_imports, dead_code, clippy::too_many_arguments)]
use crate::db::{DbJson, DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Gateway update / management queries
// ============================================================================

/// Check if a site exists in an organization.
pub async fn site_exists_in_org(
    pool: &DbPool,
    site_id: Uuid,
    org_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE id = $1 AND organization_id = $2)",
        )
        .bind(crate::db::bind_id(site_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_one(pool)
        .await?;
        Ok(val)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sites WHERE id = $1 AND organization_id = $2")
                .bind(DbUuid::from(site_id))
                .bind(DbUuid::from(org_id))
                .fetch_one(pool)
                .await?;
        Ok(count > 0)
    }
}

/// Get gateway site_id and zone.
pub async fn get_gateway_site_and_zone(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Option<(Option<Uuid>, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (Option<Uuid>, String)>(
            "SELECT site_id, zone FROM gateways WHERE id = $1 AND organization_id = $2",
        )
        .bind(crate::db::bind_id(gateway_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_as::<_, (Option<DbUuid>, String)>(
            "SELECT site_id, zone FROM gateways WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(gateway_id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|(sid, zone)| (sid.map(|s| s.into_inner()), zone)))
    }
}

/// Get gateway name and zone.
pub async fn get_gateway_name_and_zone(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT name, zone FROM gateways WHERE id = $1 AND organization_id = $2",
    )
    .bind(DbUuid::from(gateway_id))
    .bind(DbUuid::from(org_id))
    .fetch_optional(pool)
    .await
}

/// Check if a gateway exists in an organization.
pub async fn gateway_exists_in_org(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM gateways WHERE id = $1 AND organization_id = $2)",
        )
        .bind(crate::db::bind_id(gateway_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_one(pool)
        .await?;
        Ok(val)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let count: i32 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM gateways WHERE id = $1 AND organization_id = $2",
        )
        .bind(DbUuid::from(gateway_id))
        .bind(DbUuid::from(org_id))
        .fetch_one(pool)
        .await?;
        Ok(count > 0)
    }
}

/// List agents for a gateway.
pub struct GatewayAgentInfo {
    pub id: Uuid,
    pub hostname: String,
    pub is_active: bool,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_gateway_agents(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Vec<GatewayAgentInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let rows =
            sqlx::query_as::<_, (Uuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT id, hostname, is_active, last_heartbeat_at
               FROM agents
               WHERE gateway_id = $1 AND organization_id = $2
               ORDER BY hostname"#,
            )
            .bind(crate::db::bind_id(gateway_id))
            .bind(crate::db::bind_id(org_id))
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, hostname, is_active, last_heartbeat_at)| GatewayAgentInfo {
                    id,
                    hostname,
                    is_active,
                    last_heartbeat_at,
                },
            )
            .collect())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let rows =
            sqlx::query_as::<_, (DbUuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT id, hostname, is_active, last_heartbeat_at
               FROM agents
               WHERE gateway_id = $1 AND organization_id = $2
               ORDER BY hostname"#,
            )
            .bind(DbUuid::from(gateway_id))
            .bind(DbUuid::from(org_id))
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, hostname, is_active, last_heartbeat_at)| GatewayAgentInfo {
                    id: id.into_inner(),
                    hostname,
                    is_active,
                    last_heartbeat_at,
                },
            )
            .collect())
    }
}

// ============================================================================
// Suspend / activate (PG/SQLite different boolean syntax)
// ============================================================================

/// Gateway row returned by suspend/activate operations.
pub struct GatewayUpdateRow {
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

/// Suspend a gateway (set is_active = false).
pub async fn suspend_gateway(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Option<GatewayUpdateRow>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
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
        let row = sqlx::query_as::<_, Row>(
            r#"UPDATE gateways SET is_active = false
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, zone, hostname, port, site_id,
                         certificate_fingerprint, is_active,
                         COALESCE(is_primary, false) as is_primary,
                         COALESCE(priority, 0) as priority,
                         version, last_heartbeat_at, created_at"#,
        )
        .bind(crate::db::bind_id(gateway_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| GatewayUpdateRow {
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
        }))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
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
        let row = sqlx::query_as::<_, Row>(
            r#"UPDATE gateways SET is_active = 0
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, zone, hostname, port, site_id,
                         certificate_fingerprint, is_active,
                         COALESCE(is_primary, 0) as is_primary,
                         COALESCE(priority, 0) as priority,
                         version, last_heartbeat_at, created_at"#,
        )
        .bind(DbUuid::from(gateway_id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| GatewayUpdateRow {
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
        }))
    }
}

/// Activate a gateway (set is_active = true).
pub async fn activate_gateway(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Option<GatewayUpdateRow>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
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
        let row = sqlx::query_as::<_, Row>(
            r#"UPDATE gateways SET is_active = true
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, zone, hostname, port, site_id,
                         certificate_fingerprint, is_active,
                         COALESCE(is_primary, false) as is_primary,
                         COALESCE(priority, 0) as priority,
                         version, last_heartbeat_at, created_at"#,
        )
        .bind(crate::db::bind_id(gateway_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| GatewayUpdateRow {
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
        }))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
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
        let row = sqlx::query_as::<_, Row>(
            r#"UPDATE gateways SET is_active = 1
               WHERE id = $1 AND organization_id = $2
               RETURNING id, organization_id, name, zone, hostname, port, site_id,
                         certificate_fingerprint, is_active,
                         COALESCE(is_primary, 0) as is_primary,
                         COALESCE(priority, 0) as priority,
                         version, last_heartbeat_at, created_at"#,
        )
        .bind(DbUuid::from(gateway_id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| GatewayUpdateRow {
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
        }))
    }
}

// ============================================================================
// Certificate / security queries
// ============================================================================

/// Get agent certificate fingerprint and CN.
pub async fn get_agent_cert_info(
    pool: &DbPool,
    agent_id: Uuid,
    org_id: Uuid,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT certificate_fingerprint, certificate_cn FROM agents WHERE id = $1 AND organization_id = $2",
    )
    .bind(DbUuid::from(agent_id))
    .bind(DbUuid::from(org_id))
    .fetch_optional(pool)
    .await
}

/// Get gateway certificate fingerprint and CN.
pub async fn get_gateway_cert_info(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT certificate_fingerprint, certificate_cn FROM gateways WHERE id = $1 AND organization_id = $2",
    )
    .bind(DbUuid::from(gateway_id))
    .bind(DbUuid::from(org_id))
    .fetch_optional(pool)
    .await
}

/// List revoked certificates for an organization.
pub struct RevokedCertInfo {
    pub id: Uuid,
    pub fingerprint: String,
    pub cn: Option<String>,
    pub agent_id: Option<Uuid>,
    pub gateway_id: Option<Uuid>,
    pub reason: String,
    pub revoked_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_revoked_certificates(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Vec<RevokedCertInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<String>,
                Option<Uuid>,
                Option<Uuid>,
                String,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            r#"SELECT id, fingerprint, cn, agent_id, gateway_id, reason, revoked_at
               FROM revoked_certificates
               WHERE organization_id = $1
               ORDER BY revoked_at DESC
               LIMIT 100"#,
        )
        .bind(crate::db::bind_id(org_id))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, fp, cn, aid, gid, reason, at)| RevokedCertInfo {
                id,
                fingerprint: fp,
                cn,
                agent_id: aid,
                gateway_id: gid,
                reason,
                revoked_at: at,
            })
            .collect())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                Option<String>,
                Option<DbUuid>,
                Option<DbUuid>,
                String,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            r#"SELECT id, fingerprint, cn, agent_id, gateway_id, reason, revoked_at
               FROM revoked_certificates
               WHERE organization_id = $1
               ORDER BY revoked_at DESC
               LIMIT 100"#,
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, fp, cn, aid, gid, reason, at)| RevokedCertInfo {
                id: id.into_inner(),
                fingerprint: fp,
                cn,
                agent_id: aid.map(|a| a.into_inner()),
                gateway_id: gid.map(|g| g.into_inner()),
                reason,
                revoked_at: at,
            })
            .collect())
    }
}

/// Verify agent certificate pinning.
pub async fn verify_agent_cert_pinning(
    pool: &DbPool,
    agent_id: Uuid,
    presented_fingerprint: &str,
) -> bool {
    #[cfg(feature = "postgres")]
    let stored: Option<Option<String>> = sqlx::query_scalar(
        "SELECT certificate_fingerprint FROM agents WHERE id = $1 AND is_active = true AND identity_verified = true",
    )
    .bind(crate::db::bind_id(agent_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let stored: Option<Option<String>> = sqlx::query_scalar(
        "SELECT certificate_fingerprint FROM agents WHERE id = $1 AND is_active = 1 AND identity_verified = 1",
    )
    .bind(DbUuid::from(agent_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match stored {
        Some(Some(fp)) => fp == presented_fingerprint,
        _ => false,
    }
}

/// Check if a certificate fingerprint is revoked for an organization.
pub async fn is_cert_revoked_in_org(pool: &DbPool, org_id: Uuid, fingerprint: &str) -> bool {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM revoked_certificates WHERE organization_id = $1 AND fingerprint = $2)",
        )
        .bind(crate::db::bind_id(org_id))
        .bind(fingerprint)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let count: i32 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM revoked_certificates WHERE organization_id = $1 AND fingerprint = $2",
        )
        .bind(DbUuid::from(org_id))
        .bind(fingerprint)
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        count > 0
    }
}

/// Get components for an agent (for UNREACHABLE transition in gateway block).
pub struct GatewayAgentComponent {
    pub id: Uuid,
    pub name: String,
    pub application_id: Uuid,
    pub app_name: String,
}

pub async fn get_agent_components_for_unreachable(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Vec<GatewayAgentComponent>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            name: String,
            application_id: Uuid,
            app_name: String,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT c.id, c.name, c.application_id, a.name AS app_name
               FROM components c
               JOIN applications a ON c.application_id = a.id
               WHERE c.agent_id = $1"#,
        )
        .bind(crate::db::bind_id(agent_id))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| GatewayAgentComponent {
                id: r.id,
                name: r.name,
                application_id: r.application_id,
                app_name: r.app_name,
            })
            .collect())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: DbUuid,
            name: String,
            application_id: DbUuid,
            app_name: String,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT c.id, c.name, c.application_id, a.name AS app_name
               FROM components c
               JOIN applications a ON c.application_id = a.id
               WHERE c.agent_id = $1"#,
        )
        .bind(DbUuid::from(agent_id))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| GatewayAgentComponent {
                id: r.id.into_inner(),
                name: r.name,
                application_id: r.application_id.into_inner(),
                app_name: r.app_name,
            })
            .collect())
    }
}

/// Insert UNREACHABLE state transition with JSON details (gateway_blocked trigger).
pub async fn insert_gateway_blocked_transition(
    pool: &DbPool,
    component_id: Uuid,
    from_state: &str,
    details: &Value,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
           VALUES ($1, $2, 'UNREACHABLE', 'gateway_blocked', $3)"#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(from_state)
    .bind(details)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger, details)
           VALUES ($1, $2, $3, 'UNREACHABLE', 'gateway_blocked', $4)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(component_id))
    .bind(from_state)
    .bind(serde_json::to_string(details).unwrap_or_default())
    .execute(pool)
    .await?;

    Ok(())
}

/// Deactivate agent and clear identity verification (for cert revocation).
pub async fn deactivate_agent_identity(pool: &DbPool, agent_id: Uuid) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE agents SET is_active = false, identity_verified = false WHERE id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(pool)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE agents SET is_active = 0, identity_verified = 0 WHERE id = $1")
        .bind(DbUuid::from(agent_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Deactivate a gateway (set is_active = false).
pub async fn deactivate_gateway(pool: &DbPool, gateway_id: Uuid) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .execute(pool)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE gateways SET is_active = 0 WHERE id = $1")
        .bind(DbUuid::from(gateway_id))
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Gateway update queries (api/gateways.rs)
// ============================================================================

/// Unset primary for gateways in a site (except the given gateway).
pub async fn unset_primary_in_site(
    pool: &DbPool,
    org_id: Uuid,
    site_id: Uuid,
    except_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND site_id = $2 AND id != $3")
        .bind(crate::db::bind_id(org_id))
        .bind(crate::db::bind_id(site_id))
        .bind(crate::db::bind_id(except_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Unset primary for gateways in a zone without site (except the given gateway).
pub async fn unset_primary_in_zone(
    pool: &DbPool,
    org_id: Uuid,
    zone: &str,
    except_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND zone = $2 AND site_id IS NULL AND id != $3")
        .bind(crate::db::bind_id(org_id))
        .bind(zone)
        .bind(crate::db::bind_id(except_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Update a gateway and return its updated state. PostgreSQL only (uses RETURNING).
#[cfg(feature = "postgres")]
pub async fn update_gateway_returning(
    pool: &DbPool,
    id: Uuid,
    org_id: Uuid,
    name: &Option<String>,
    site_id: Option<Uuid>,
    is_active: Option<bool>,
    is_primary: Option<bool>,
    priority: Option<i32>,
) -> Result<Option<crate::api::gateways::GatewayRow>, sqlx::Error> {
    sqlx::query_as::<_, crate::api::gateways::GatewayRow>(
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
    .fetch_optional(pool)
    .await
}

/// Disconnect all agents from a gateway (set gateway_id = NULL).
pub async fn disconnect_agents_from_gateway(
    pool: &DbPool,
    gateway_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agents SET gateway_id = NULL WHERE gateway_id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a gateway. Returns rows affected.
pub async fn delete_gateway(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM gateways WHERE id = $1 AND organization_id = $2")
        .bind(crate::db::bind_id(gateway_id))
        .bind(crate::db::bind_id(org_id))
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Insert a revoked certificate record.
pub async fn insert_revoked_agent_cert(
    pool: &DbPool,
    org_id: Uuid,
    fingerprint: &str,
    cn: &str,
    agent_id: Uuid,
    reason: &str,
    revoked_by: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, agent_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(agent_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (id, organization_id, fingerprint, cn, agent_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(agent_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert a certificate revocation event (agent).
pub async fn insert_agent_cert_revoked_event(
    pool: &DbPool,
    agent_id: Uuid,
    fingerprint: &str,
    cn: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           VALUES ($1, 'revoked', $2, $3)"#,
    )
    .bind(crate::db::bind_id(agent_id))
    .bind(fingerprint)
    .bind(cn)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO certificate_events (id, agent_id, event_type, fingerprint, cn)
           VALUES ($1, $2, 'revoked', $3, $4)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(agent_id))
    .bind(fingerprint)
    .bind(cn)
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert a revoked gateway certificate record.
pub async fn insert_revoked_gateway_cert(
    pool: &DbPool,
    org_id: Uuid,
    fingerprint: &str,
    cn: &str,
    gateway_id: Uuid,
    reason: &str,
    revoked_by: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, gateway_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(gateway_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (id, organization_id, fingerprint, cn, gateway_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(gateway_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert a certificate revocation event (gateway).
pub async fn insert_gateway_cert_revoked_event(
    pool: &DbPool,
    gateway_id: Uuid,
    fingerprint: &str,
    cn: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO certificate_events (gateway_id, event_type, fingerprint, cn)
           VALUES ($1, 'revoked', $2, $3)"#,
    )
    .bind(crate::db::bind_id(gateway_id))
    .bind(fingerprint)
    .bind(cn)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Promote to primary (transaction queries)
// ============================================================================

/// Unset primary flag for all gateways in a site except the specified one.
pub async fn unset_primary_in_site_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    site_id: Uuid,
    except_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND site_id = $2 AND id != $3")
        .bind(crate::db::bind_id(org_id))
        .bind(crate::db::bind_id(site_id))
        .bind(except_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Unset primary flag for all gateways in a zone (no site) except the specified one.
pub async fn unset_primary_in_zone_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    zone: &str,
    except_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND zone = $2 AND site_id IS NULL AND id != $3")
        .bind(crate::db::bind_id(org_id))
        .bind(zone)
        .bind(except_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Set a gateway as primary.
pub async fn set_primary_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    gateway_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_primary = true WHERE id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Insert a gateway status event.
pub async fn insert_gateway_status_event_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    gateway_id: Uuid,
    event_type: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO gateway_status_events (organization_id, gateway_id, event_type, triggered_by)
           VALUES ($1, $2, $3, 'manual')"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(gateway_id))
    .bind(event_type)
    .execute(&mut **tx)
    .await
    .ok();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO gateway_status_events (id, organization_id, gateway_id, event_type, triggered_by)
           VALUES ($1, $2, $3, $4, 'manual')"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(gateway_id))
    .bind(event_type)
    .execute(&mut **tx)
    .await
    .ok();

    Ok(())
}

// ============================================================================
// Block gateway (transaction queries)
// ============================================================================

/// Deactivate gateway in transaction (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn deactivate_gateway_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    gateway_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Get agent IDs for a gateway in a transaction.
pub async fn get_gateway_agent_ids_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    gateway_id: Uuid,
    org_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM agents WHERE gateway_id = $1 AND organization_id = $2")
        .bind(crate::db::bind_id(gateway_id))
        .bind(crate::db::bind_id(org_id))
        .fetch_all(&mut **tx)
        .await
}

/// Disconnect all agents from a gateway in a transaction.
pub async fn disconnect_agents_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    gateway_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agents SET gateway_id = NULL WHERE gateway_id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// ============================================================================
// Block agent via gateway (transaction queries)
// ============================================================================

/// Deactivate agent and clear identity (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn deactivate_agent_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    agent_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agents SET is_active = false, identity_verified = false WHERE id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Deactivate agent and clear identity (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn deactivate_agent_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    agent_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agents SET is_active = 0, identity_verified = 0 WHERE id = $1")
        .bind(DbUuid::from(agent_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Insert an agent blocked event in transaction.
pub async fn insert_agent_blocked_event_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    agent_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           SELECT $1, 'blocked', certificate_fingerprint, certificate_cn
           FROM agents WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(agent_id))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Insert an agent status event in transaction.
pub async fn insert_agent_status_event_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    gateway_id: Uuid,
    agent_id: Uuid,
    event_type: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO gateway_status_events (organization_id, gateway_id, agent_id, event_type, triggered_by)
           VALUES ($1, $2, $3, $4, 'manual')"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(gateway_id))
    .bind(crate::db::bind_id(agent_id))
    .bind(event_type)
    .execute(&mut **tx)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO gateway_status_events (id, organization_id, gateway_id, agent_id, event_type, triggered_by)
           VALUES ($1, $2, $3, $4, $5, 'manual')"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(gateway_id))
    .bind(crate::db::bind_id(agent_id))
    .bind(event_type)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

// ============================================================================
// Revoke gateway cert (transaction queries)
// ============================================================================

/// Insert revoked certificate record in transaction.
pub async fn insert_revoked_cert_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    fingerprint: &str,
    cn: &str,
    reason: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, reason)
           VALUES ($1, $2, $3, $4) ON CONFLICT (fingerprint) DO NOTHING"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(reason)
    .execute(&mut **tx)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (id, organization_id, fingerprint, cn, reason)
           VALUES ($1, $2, $3, $4, $5) ON CONFLICT (fingerprint) DO NOTHING"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(reason)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Deactivate a gateway in transaction (for revocation).
#[cfg(feature = "postgres")]
pub async fn deactivate_gateway_for_revocation_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    gateway_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Insert revoked certificate for an agent (with agent_id) in transaction.
pub async fn insert_revoked_agent_cert_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    fingerprint: &str,
    cn: &str,
    agent_id: Uuid,
    reason: &str,
    revoked_by: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, agent_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(agent_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(&mut **tx)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (id, organization_id, fingerprint, cn, agent_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(agent_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Insert certificate event for an agent in transaction.
pub async fn insert_agent_cert_event_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    agent_id: Uuid,
    event_type: &str,
    fingerprint: &str,
    cn: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(crate::db::bind_id(agent_id))
    .bind(event_type)
    .bind(fingerprint)
    .bind(cn)
    .execute(&mut **tx)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO certificate_events (id, agent_id, event_type, fingerprint, cn)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(agent_id))
    .bind(event_type)
    .bind(fingerprint)
    .bind(cn)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Deactivate agent and clear identity in transaction (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn deactivate_agent_clear_identity_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    agent_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agents SET is_active = false, identity_verified = false WHERE id = $1")
        .bind(crate::db::bind_id(agent_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Insert revoked certificate for a gateway (with gateway_id) in transaction.
pub async fn insert_revoked_gateway_cert_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    org_id: Uuid,
    fingerprint: &str,
    cn: &str,
    gateway_id: Uuid,
    reason: &str,
    revoked_by: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, gateway_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(gateway_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(&mut **tx)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO revoked_certificates (id, organization_id, fingerprint, cn, gateway_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(org_id))
    .bind(fingerprint)
    .bind(cn)
    .bind(crate::db::bind_id(gateway_id))
    .bind(reason)
    .bind(crate::db::bind_id(revoked_by))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Insert certificate event for a gateway in transaction.
pub async fn insert_gateway_cert_event_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    gateway_id: Uuid,
    event_type: &str,
    fingerprint: &str,
    cn: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO certificate_events (gateway_id, event_type, fingerprint, cn)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(crate::db::bind_id(gateway_id))
    .bind(event_type)
    .bind(fingerprint)
    .bind(cn)
    .execute(&mut **tx)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO certificate_events (id, gateway_id, event_type, fingerprint, cn)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(gateway_id))
    .bind(event_type)
    .bind(fingerprint)
    .bind(cn)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// SQLite versions of transaction functions
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_gateway_returning(
    pool: &DbPool,
    id: Uuid,
    org_id: Uuid,
    name: &Option<String>,
    site_id: Option<Uuid>,
    is_active: Option<bool>,
    is_primary: Option<bool>,
    priority: Option<i32>,
) -> Result<Option<crate::api::gateways::GatewayRow>, sqlx::Error> {
    use crate::db::{bind_id, DbUuid};
    sqlx::query(
        "UPDATE gateways SET name = COALESCE($3, name), site_id = COALESCE($4, site_id), \
         is_active = COALESCE($5, is_active), is_primary = COALESCE($6, is_primary), \
         priority = COALESCE($7, priority) WHERE id = $1 AND organization_id = $2",
    )
    .bind(bind_id(id))
    .bind(bind_id(org_id))
    .bind(name)
    .bind(site_id.map(DbUuid::from))
    .bind(is_active.map(|b| if b { 1i32 } else { 0 }))
    .bind(is_primary.map(|b| if b { 1i32 } else { 0 }))
    .bind(priority)
    .execute(pool)
    .await?;
    sqlx::query_as::<_, crate::api::gateways::GatewayRow>(
        "SELECT id, organization_id, name, zone, hostname, port, site_id, \
         certificate_fingerprint, is_active, COALESCE(is_primary, 0) as is_primary, \
         COALESCE(priority, 0) as priority, version, last_heartbeat_at, created_at \
         FROM gateways WHERE id = $1",
    )
    .bind(bind_id(id))
    .fetch_optional(pool)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn deactivate_gateway_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_active = 0 WHERE id = $1")
        .bind(crate::db::DbUuid::from(id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn deactivate_gateway_for_revocation_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gateways SET is_active = 0, certificate_fingerprint = NULL WHERE id = $1")
        .bind(crate::db::DbUuid::from(id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn deactivate_agent_clear_identity_tx<'a>(
    tx: &mut crate::db::DbTransaction<'a>,
    agent_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agents SET is_active = 0, identity_verified = 0 WHERE id = $1")
        .bind(crate::db::DbUuid::from(agent_id))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
