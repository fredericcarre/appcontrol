//! Query functions for websocket domain. All sqlx queries live here.
//!
//! These are free functions (not a trait) because they're called from
//! various async contexts in the WebSocket handler. Each function handles
//! PG/SQLite differences internally via `#[cfg]` attributes.

#![allow(unused_imports, dead_code)]
use crate::db::{DbJson, DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Gateway registration queries
// ============================================================================

/// Look up a site by organization_id and code (for backward-compat zone lookup).
#[allow(clippy::too_many_arguments)]
pub async fn lookup_site_by_code(
    pool: &DbPool,
    org_id: Uuid,
    code: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM sites WHERE organization_id = $1 AND code = $2",
        )
        .bind(org_id)
        .bind(code)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM sites WHERE organization_id = $1 AND code = $2",
        )
        .bind(DbUuid::from(org_id))
        .bind(code)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }
}

/// Upsert a gateway record on registration (with auto-priority assignment).
pub async fn upsert_gateway(
    pool: &DbPool,
    gateway_id: Uuid,
    org_id: Uuid,
    name: &str,
    zone: Option<&str>,
    site_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        WITH site_info AS (
            SELECT
                COALESCE(MAX(priority), -1) + 1 AS next_priority,
                COUNT(*) = 0 AS is_first_in_site
            FROM gateways
            WHERE organization_id = $2
              AND (($5::uuid IS NOT NULL AND site_id = $5) OR ($5::uuid IS NULL AND site_id IS NULL))
              AND id != $1
        )
        INSERT INTO gateways (id, organization_id, name, zone, site_id, is_active, is_primary, priority, last_heartbeat_at)
        SELECT $1, $2, $3, $4, $5, true,
               CASE WHEN $5::uuid IS NOT NULL THEN si.is_first_in_site ELSE false END,
               si.next_priority,
               now()
        FROM site_info si
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            zone = COALESCE(EXCLUDED.zone, gateways.zone),
            site_id = COALESCE(EXCLUDED.site_id, gateways.site_id),
            last_heartbeat_at = now()
        "#,
    )
    .bind(gateway_id)
    .bind(org_id)
    .bind(name)
    .bind(zone)
    .bind(site_id)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO gateways (id, organization_id, name, zone, site_id, is_active, last_heartbeat_at) \
         VALUES ($1, $2, $3, $4, $5, 1, datetime('now')) \
         ON CONFLICT (id) DO UPDATE SET \
             name = EXCLUDED.name, \
             zone = COALESCE(EXCLUDED.zone, gateways.zone), \
             site_id = COALESCE(EXCLUDED.site_id, gateways.site_id), \
             last_heartbeat_at = datetime('now')",
    )
    .bind(DbUuid::from(gateway_id))
    .bind(DbUuid::from(org_id))
    .bind(name)
    .bind(zone.unwrap_or("default"))
    .bind(site_id.map(DbUuid::from))
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================================
// Agent connection / security queries
// ============================================================================

/// Check if a certificate fingerprint has been revoked.
pub async fn is_certificate_revoked(pool: &DbPool, fingerprint: &str) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM revoked_certificates WHERE fingerprint = $1)",
        )
        .bind(fingerprint)
        .fetch_one(pool)
        .await?;
        Ok(val)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let count: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM revoked_certificates WHERE fingerprint = $1")
                .bind(fingerprint)
                .fetch_one(pool)
                .await?;
        Ok(count > 0)
    }
}

/// Get the stored certificate fingerprint for an agent.
pub async fn get_agent_cert_fingerprint(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Option<Option<String>>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar("SELECT certificate_fingerprint FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar("SELECT certificate_fingerprint FROM agents WHERE id = $1")
            .bind(DbUuid::from(agent_id))
            .fetch_optional(pool)
            .await
    }
}

/// Update agent record with gateway_id, certificate fingerprint, CN, and version.
pub async fn update_agent_connection_info(
    pool: &DbPool,
    agent_id: Uuid,
    gateway_id: Uuid,
    hostname: &str,
    cert_fingerprint: Option<&str>,
    cert_cn: Option<&str>,
    version: Option<&str>,
) -> Result<(), sqlx::Error> {
    // In standalone/dev mode (no enrollment), agents may not exist yet.
    // Use an upsert: INSERT if missing, UPDATE if already present.
    // Derive org_id from the gateway's organization_id.
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, hostname, is_active, gateway_id,
               certificate_fingerprint, certificate_cn, identity_verified, version)
           SELECT $1, g.organization_id, $6,
               true, $2, $3, $4, ($3 IS NOT NULL), $5
           FROM gateways g WHERE g.id = $2
           ON CONFLICT (id) DO UPDATE SET
               gateway_id = $2,
               hostname = $6,
               certificate_fingerprint = COALESCE($3, agents.certificate_fingerprint),
               certificate_cn = COALESCE($4, agents.certificate_cn),
               version = COALESCE($5, agents.version),
               identity_verified = ($3 IS NOT NULL)"#,
    )
    .bind(agent_id)
    .bind(gateway_id)
    .bind(cert_fingerprint)
    .bind(cert_cn)
    .bind(version)
    .bind(hostname)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        // SQLite: two-step approach — first try UPDATE; if 0 rows affected, INSERT.
        let rows = sqlx::query(
            "UPDATE agents SET gateway_id = $2, hostname = $6, \
             certificate_fingerprint = COALESCE($3, certificate_fingerprint), \
             certificate_cn = COALESCE($4, certificate_cn), \
             version = COALESCE($5, version), \
             identity_verified = ($3 IS NOT NULL) \
             WHERE id = $1",
        )
        .bind(DbUuid::from(agent_id))
        .bind(DbUuid::from(gateway_id))
        .bind(cert_fingerprint)
        .bind(cert_cn)
        .bind(version)
        .bind(hostname)
        .execute(pool)
        .await?;

        if rows.rows_affected() == 0 {
            // Agent doesn't exist yet — create from gateway's org
            sqlx::query(
                "INSERT OR IGNORE INTO agents (id, organization_id, hostname, is_active, gateway_id, \
                 certificate_fingerprint, certificate_cn, identity_verified, version) \
                 SELECT $1, g.organization_id, $6, \
                 1, $2, $3, $4, ($3 IS NOT NULL), $5 \
                 FROM gateways g WHERE g.id = $2",
            )
            .bind(DbUuid::from(agent_id))
            .bind(DbUuid::from(gateway_id))
            .bind(cert_fingerprint)
            .bind(cert_cn)
            .bind(version)
            .bind(hostname)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

// ============================================================================
// Agent heartbeat / metrics queries
// ============================================================================

/// Restore agent gateway_id if it was NULL (e.g. after block/unblock).
/// Returns true if a row was actually updated.
pub async fn restore_agent_gateway_if_null(
    pool: &DbPool,
    agent_id: Uuid,
    gateway_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let result =
        sqlx::query("UPDATE agents SET gateway_id = $2 WHERE id = $1 AND gateway_id IS NULL")
            .bind(agent_id)
            .bind(gateway_id)
            .execute(pool)
            .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result =
        sqlx::query("UPDATE agents SET gateway_id = $2 WHERE id = $1 AND gateway_id IS NULL")
            .bind(DbUuid::from(agent_id))
            .bind(DbUuid::from(gateway_id))
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

/// Insert agent metrics (cpu, memory, disk).
pub async fn insert_agent_metrics(
    pool: &DbPool,
    agent_id: Uuid,
    cpu: f32,
    memory: f32,
    disk: Option<f32>,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO agent_metrics (agent_id, cpu_pct, memory_pct, disk_used_pct) VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(cpu)
    .bind(memory)
    .bind(disk)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO agent_metrics (agent_id, cpu_pct, memory_pct, disk_used_pct) VALUES ($1, $2, $3, $4)",
    )
    .bind(DbUuid::from(agent_id))
    .bind(cpu)
    .bind(memory)
    .bind(disk)
    .execute(pool)
    .await?;

    Ok(())
}

/// Update gateway heartbeat timestamp only (no is_active change).
pub async fn update_gateway_heartbeat_ts(
    pool: &DbPool,
    gateway_id: Uuid,
) -> Result<(), sqlx::Error> {
    let sql = format!(
        "UPDATE gateways SET last_heartbeat_at = {} WHERE id = $1",
        crate::db::sql::now()
    );
    #[cfg(feature = "postgres")]
    sqlx::query(&sql).bind(gateway_id).execute(pool).await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(&sql)
        .bind(DbUuid::from(gateway_id))
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Agent registration queries
// ============================================================================

/// Check if an agent is blocked (is_active = false).
pub async fn is_agent_blocked(pool: &DbPool, agent_id: Uuid) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: Option<bool> =
            sqlx::query_scalar("SELECT NOT COALESCE(is_active, true) FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_optional(pool)
                .await?;
        Ok(val.unwrap_or(false))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let val: Option<i32> = sqlx::query_scalar(
            "SELECT CASE WHEN COALESCE(is_active, 1) = 0 THEN 1 ELSE 0 END FROM agents WHERE id = $1",
        )
        .bind(DbUuid::from(agent_id))
        .fetch_optional(pool)
        .await?;
        Ok(val.unwrap_or(0) != 0)
    }
}

/// Update agent record on Register message (hostname, IPs, version, system info).
pub async fn update_agent_registration(
    pool: &DbPool,
    agent_id: Uuid,
    hostname: &str,
    ip_addresses: &[String],
    version: Option<&str>,
    os_name: Option<&str>,
    os_version: Option<&str>,
    cpu_arch: Option<&str>,
    cpu_cores: Option<i32>,
    total_memory_mb: Option<i64>,
    disk_total_gb: Option<i64>,
    cert_fingerprint: Option<&str>,
) -> Result<(), sqlx::Error> {
    let sql = format!(
        "UPDATE agents SET hostname = $2, ip_addresses = $3, last_heartbeat_at = {}, \
         version = COALESCE($4, version), \
         os_name = COALESCE($5, os_name), \
         os_version = COALESCE($6, os_version), \
         cpu_arch = COALESCE($7, cpu_arch), \
         cpu_cores = COALESCE($8, cpu_cores), \
         total_memory_mb = COALESCE($9, total_memory_mb), \
         disk_total_gb = COALESCE($10, disk_total_gb), \
         certificate_fingerprint = COALESCE($11, certificate_fingerprint), \
         identity_verified = ($11 IS NOT NULL) \
         WHERE id = $1 AND is_active = {}",
        crate::db::sql::now(),
        if cfg!(feature = "postgres") {
            "true"
        } else {
            "1"
        }
    );

    #[cfg(feature = "postgres")]
    sqlx::query(&sql)
        .bind(agent_id)
        .bind(hostname)
        .bind(serde_json::json!(ip_addresses))
        .bind(version)
        .bind(os_name)
        .bind(os_version)
        .bind(cpu_arch)
        .bind(cpu_cores)
        .bind(total_memory_mb)
        .bind(disk_total_gb)
        .bind(cert_fingerprint)
        .execute(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(&sql)
        .bind(DbUuid::from(agent_id))
        .bind(hostname)
        .bind(serde_json::to_string(ip_addresses).unwrap_or_default())
        .bind(version)
        .bind(os_name)
        .bind(os_version)
        .bind(cpu_arch)
        .bind(cpu_cores)
        .bind(total_memory_mb)
        .bind(disk_total_gb)
        .bind(cert_fingerprint)
        .execute(pool)
        .await?;

    Ok(())
}

// ============================================================================
// Discovery / update queries
// ============================================================================

/// Insert a discovery report.
pub async fn insert_discovery_report(
    pool: &DbPool,
    agent_id: Uuid,
    hostname: &str,
    report: &Value,
    scanned_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO discovery_reports (agent_id, hostname, report, scanned_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(agent_id)
    .bind(hostname)
    .bind(report)
    .bind(scanned_at)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO discovery_reports (agent_id, hostname, report, scanned_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(DbUuid::from(agent_id))
    .bind(hostname)
    .bind(serde_json::to_string(report).unwrap_or_default())
    .bind(scanned_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(())
}

/// Update agent update task status.
pub async fn update_agent_update_task(
    pool: &DbPool,
    update_id: Uuid,
    status: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    let sql = format!(
        "UPDATE agent_update_tasks \
         SET status = $2, error = $3, \
             completed_at = CASE WHEN $2 IN ('complete', 'failed') THEN {} ELSE completed_at END \
         WHERE id = $1",
        crate::db::sql::now()
    );

    #[cfg(feature = "postgres")]
    sqlx::query(&sql)
        .bind(update_id)
        .bind(status)
        .bind(error)
        .execute(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(&sql)
        .bind(DbUuid::from(update_id))
        .bind(status)
        .bind(error)
        .execute(pool)
        .await?;

    Ok(())
}

// ============================================================================
// Component config queries (send_config_to_agent)
// ============================================================================

/// Component config info for agent.
pub struct AgentComponentConfig {
    pub component_id: Uuid,
    pub name: String,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub post_start_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    pub rebuild_infra_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub env_vars: Value,
}

/// Get all component configs for an agent (non-suspended apps only).
pub async fn get_agent_component_configs(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Vec<AgentComponentConfig>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                i32,
                i32,
                i32,
                Value,
            ),
        >(
            "SELECT c.id, c.name, c.check_cmd, c.start_cmd, c.stop_cmd,
                    c.integrity_check_cmd, c.post_start_check_cmd, c.infra_check_cmd,
                    c.rebuild_cmd, c.rebuild_infra_cmd,
                    c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                    COALESCE(c.env_vars, '{}'::jsonb)
             FROM components c
             JOIN applications a ON c.application_id = a.id
             WHERE c.agent_id = $1
               AND a.is_suspended = false",
        )
        .bind(agent_id)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    check,
                    start,
                    stop,
                    integrity,
                    post_start,
                    infra,
                    rebuild,
                    rebuild_infra,
                    interval,
                    start_to,
                    stop_to,
                    env,
                )| {
                    AgentComponentConfig {
                        component_id: id,
                        name,
                        check_cmd: check,
                        start_cmd: start,
                        stop_cmd: stop,
                        integrity_check_cmd: integrity,
                        post_start_check_cmd: post_start,
                        infra_check_cmd: infra,
                        rebuild_cmd: rebuild,
                        rebuild_infra_cmd: rebuild_infra,
                        check_interval_seconds: interval,
                        start_timeout_seconds: start_to,
                        stop_timeout_seconds: stop_to,
                        env_vars: env,
                    }
                },
            )
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
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                i32,
                i32,
                i32,
                String,
            ),
        >(
            "SELECT c.id, c.name, c.check_cmd, c.start_cmd, c.stop_cmd,
                    c.integrity_check_cmd, c.post_start_check_cmd, c.infra_check_cmd,
                    c.rebuild_cmd, c.rebuild_infra_cmd,
                    c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                    COALESCE(c.env_vars, '{}')
             FROM components c
             JOIN applications a ON c.application_id = a.id
             WHERE c.agent_id = $1
               AND a.is_suspended = 0",
        )
        .bind(DbUuid::from(agent_id))
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    name,
                    check,
                    start,
                    stop,
                    integrity,
                    post_start,
                    infra,
                    rebuild,
                    rebuild_infra,
                    interval,
                    start_to,
                    stop_to,
                    env,
                )| {
                    AgentComponentConfig {
                        component_id: id.into_inner(),
                        name,
                        check_cmd: check,
                        start_cmd: start,
                        stop_cmd: stop,
                        integrity_check_cmd: integrity,
                        post_start_check_cmd: post_start,
                        infra_check_cmd: infra,
                        rebuild_cmd: rebuild,
                        rebuild_infra_cmd: rebuild_infra,
                        check_interval_seconds: interval,
                        start_timeout_seconds: start_to,
                        stop_timeout_seconds: stop_to,
                        env_vars: serde_json::from_str::<Value>(&env)
                            .unwrap_or(serde_json::json!({})),
                    }
                },
            )
            .collect())
    }
}

// ============================================================================
// mark_agent_components_unreachable queries
// ============================================================================

/// Component info for UNREACHABLE transition.
pub struct UnreachableComponentInfo {
    pub id: Uuid,
    pub name: String,
    pub current_state: String,
    pub application_id: Uuid,
    pub app_name: String,
}

/// Get non-stopped components for an agent (for UNREACHABLE transition).
pub async fn get_agent_active_components(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Vec<UnreachableComponentInfo>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        #[cfg(feature = "postgres")]
        id: Uuid,
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        id: DbUuid,
        name: String,
        current_state: String,
        #[cfg(feature = "postgres")]
        application_id: Uuid,
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        application_id: DbUuid,
        app_name: String,
    }

    let sql = r#"
        SELECT c.id, c.name, c.current_state, c.application_id, a.name AS app_name
        FROM components c
        JOIN applications a ON a.id = c.application_id
        WHERE c.agent_id = $1
          AND c.current_state NOT IN ('UNREACHABLE', 'STOPPED', 'STOPPING', 'UNKNOWN')
    "#;

    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<_, Row>(sql)
            .bind(agent_id)
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| UnreachableComponentInfo {
                id: r.id,
                name: r.name,
                current_state: r.current_state,
                application_id: r.application_id,
                app_name: r.app_name,
            })
            .collect())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let rows = sqlx::query_as::<_, Row>(sql)
            .bind(DbUuid::from(agent_id))
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| UnreachableComponentInfo {
                id: r.id.into_inner(),
                name: r.name,
                current_state: r.current_state,
                application_id: r.application_id.into_inner(),
                app_name: r.app_name,
            })
            .collect())
    }
}

/// Insert state transition to UNREACHABLE with JSON details.
pub async fn insert_unreachable_transition(
    pool: &DbPool,
    component_id: Uuid,
    from_state: &str,
    trigger: &str,
    agent_id: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
        VALUES ($1, $2, 'UNREACHABLE', $3,
                jsonb_build_object('previous_state', $2, 'agent_id', $4::text))
        "#,
    )
    .bind(component_id)
    .bind(from_state)
    .bind(trigger)
    .bind(agent_id.to_string())
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let details = serde_json::json!({
            "previous_state": from_state,
            "agent_id": agent_id.to_string(),
        });
        sqlx::query(
            r#"
            INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger, details)
            VALUES ($1, $2, $3, 'UNREACHABLE', $4, $5)
            "#,
        )
        .bind(DbUuid::new_v4())
        .bind(DbUuid::from(component_id))
        .bind(from_state)
        .bind(trigger)
        .bind(serde_json::to_string(&details).unwrap_or_default())
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Update component state to UNREACHABLE.
pub async fn set_component_unreachable(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE components SET current_state = 'UNREACHABLE' WHERE id = $1")
        .bind(component_id)
        .execute(pool)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE components SET current_state = 'UNREACHABLE' WHERE id = $1")
        .bind(DbUuid::from(component_id))
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// push_config_to_affected_agents queries
// ============================================================================

/// Get distinct agent IDs for components of an application.
pub async fn get_agents_for_application(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM components \
             WHERE application_id = $1 AND agent_id IS NOT NULL",
        )
        .bind(app_id)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let rows: Vec<DbUuid> = sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM components \
             WHERE application_id = $1 AND agent_id IS NOT NULL",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|u| u.into_inner()).collect())
    }
}

/// Get distinct agent IDs for a set of component IDs.
pub async fn get_agents_for_components(
    pool: &DbPool,
    comp_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if comp_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM components WHERE id = ANY($1) AND agent_id IS NOT NULL",
        )
        .bind(comp_ids)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let placeholders: Vec<String> = (1..=comp_ids.len()).map(|i| format!("${}", i)).collect();
        let query = format!(
            "SELECT DISTINCT agent_id FROM components WHERE id IN ({}) AND agent_id IS NOT NULL",
            placeholders.join(", ")
        );
        let mut q = sqlx::query_scalar::<_, String>(&query);
        for id in comp_ids {
            q = q.bind(id.to_string());
        }
        let rows: Vec<String> = q.fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect())
    }
}

// ============================================================================
// Enrollment token queries
// ============================================================================

/// Enrollment token info.
pub struct EnrollmentTokenInfo {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub scope: String,
    pub max_uses: Option<i32>,
    pub current_uses: i32,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Validate an enrollment token by hash and return its info.
pub async fn get_enrollment_token_by_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<EnrollmentTokenInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            r#"SELECT id, organization_id, scope, max_uses, current_uses, expires_at
               FROM enrollment_tokens
               WHERE token_hash = $1
               AND revoked_at IS NULL"#,
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;
        Ok(
            row.map(|(id, org, scope, max, cur, exp)| EnrollmentTokenInfo {
                id,
                organization_id: org,
                scope,
                max_uses: max,
                current_uses: cur,
                expires_at: exp,
            }),
        )
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_as::<_, (DbUuid, DbUuid, String, Option<i32>, i32, String)>(
            r#"SELECT id, organization_id, scope, max_uses, current_uses, expires_at
               FROM enrollment_tokens
               WHERE token_hash = $1
               AND revoked_at IS NULL"#,
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|(id, org, scope, max, cur, exp_str)| {
            let exp = chrono::DateTime::parse_from_rfc3339(&exp_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(&exp_str, "%Y-%m-%d %H:%M:%S")
                        .map(|ndt| ndt.and_utc())
                })
                .unwrap_or_else(|_| chrono::Utc::now());
            EnrollmentTokenInfo {
                id: id.into_inner(),
                organization_id: org.into_inner(),
                scope,
                max_uses: max,
                current_uses: cur,
                expires_at: exp,
            }
        }))
    }
}

/// Increment enrollment token usage count.
pub async fn increment_enrollment_token_uses(
    pool: &DbPool,
    token_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE enrollment_tokens SET current_uses = current_uses + 1 WHERE id = $1")
        .bind(DbUuid::from(token_id))
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Command result queries
// ============================================================================

/// Get component info for a command result (for broadcasting).
pub struct CommandComponentInfo {
    pub component_id: Uuid,
    pub component_name: String,
    pub application_id: Uuid,
}

/// Look up component info by command execution request_id.
pub async fn get_command_component_info(
    pool: &DbPool,
    request_id: Uuid,
) -> Result<Option<CommandComponentInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let row = sqlx::query_as::<_, (DbUuid, String, DbUuid)>(
            r#"SELECT c.id, c.name, c.application_id
               FROM command_executions ce
               JOIN components c ON ce.component_id = c.id
               WHERE ce.request_id = $1"#,
        )
        .bind(DbUuid::from(request_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|(id, name, app_id)| CommandComponentInfo {
            component_id: *id,
            component_name: name,
            application_id: *app_id,
        }))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_as::<_, (DbUuid, String, DbUuid)>(
            r#"SELECT c.id, c.name, c.application_id
               FROM command_executions ce
               JOIN components c ON ce.component_id = c.id
               WHERE ce.request_id = $1"#,
        )
        .bind(DbUuid::from(request_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|(id, name, app_id)| CommandComponentInfo {
            component_id: id.into_inner(),
            component_name: name,
            application_id: app_id.into_inner(),
        }))
    }
}

/// Get component_id from command_executions by request_id.
pub async fn get_command_execution_component_id(
    pool: &DbPool,
    request_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT component_id FROM command_executions WHERE request_id = $1",
        )
        .bind(DbUuid::from(request_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|u| *u))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT component_id FROM command_executions WHERE request_id = $1",
        )
        .bind(DbUuid::from(request_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }
}
