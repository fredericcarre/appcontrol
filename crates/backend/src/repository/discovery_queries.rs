//! Query functions for discovery domain. All sqlx queries live here.

#![allow(unused_imports, dead_code, clippy::too_many_arguments)]
use crate::db::{DbJson, DbPool, DbUuid, IntArray, UuidArray};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Agent queries for discovery
// ============================================================================

/// List active agent IDs for an organization.
pub async fn list_active_agent_ids(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Vec<DbUuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM agents WHERE organization_id = $1 AND is_active = true",
        )
        .bind(org_id)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM agents WHERE organization_id = $1 AND is_active = 1",
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(pool)
        .await
    }
}

/// Get agent IP addresses (JSONB for postgres, TEXT for sqlite).
pub async fn get_agent_ip_addresses(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT COALESCE(ip_addresses, '[]'::jsonb) FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT COALESCE(ip_addresses, '[]') FROM agents WHERE id = $1",
        )
        .bind(DbUuid::from(agent_id))
        .fetch_optional(pool)
        .await
    }
}

// ============================================================================
// Report queries
// ============================================================================

/// List recent discovery reports.
pub async fn list_discovery_reports(
    pool: &DbPool,
) -> Result<Vec<(DbUuid, DbUuid, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, agent_id, hostname, scanned_at
         FROM discovery_reports
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .fetch_all(pool)
    .await
}

/// Get a full discovery report by ID.
pub async fn get_discovery_report(
    pool: &DbPool,
    report_id: Uuid,
) -> Result<
    Option<(
        DbUuid,
        DbUuid,
        String,
        serde_json::Value,
        chrono::DateTime<chrono::Utc>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            serde_json::Value,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id, agent_id, hostname, report, scanned_at
         FROM discovery_reports WHERE id = $1",
    )
    .bind(crate::db::bind_id(report_id))
    .fetch_optional(pool)
    .await
}

/// Get the latest report for an agent.
pub async fn get_latest_report_for_agent(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Option<(DbUuid, String, serde_json::Value)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, serde_json::Value)>(
        "SELECT agent_id, hostname, report FROM discovery_reports
         WHERE agent_id = $1
         ORDER BY scanned_at DESC LIMIT 1",
    )
    .bind(crate::db::bind_id(agent_id))
    .fetch_optional(pool)
    .await
}

// ============================================================================
// Draft queries
// ============================================================================

/// List drafts for an organization.
pub async fn list_drafts(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, status, inferred_at
         FROM discovery_drafts
         WHERE organization_id = $1
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_all(pool)
    .await
}

/// Get draft header.
pub async fn get_draft_header(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<Option<(DbUuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, status, inferred_at FROM discovery_drafts WHERE id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .fetch_optional(pool)
    .await
}

/// Get draft components (postgres).
#[cfg(feature = "postgres")]
#[allow(clippy::type_complexity)]
pub async fn get_draft_components(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        Option<String>,
        Option<String>,
        String,
        serde_json::Value,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
        Option<String>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as(
        "SELECT id, suggested_name, process_name, host, component_type, metadata,
                check_cmd, start_cmd, stop_cmd, restart_cmd,
                command_confidence, command_source,
                COALESCE(config_files, '[]'::jsonb),
                COALESCE(log_files, '[]'::jsonb),
                matched_service
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(pool)
    .await
}

/// Get draft components (sqlite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn get_draft_components(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        Option<String>,
        Option<String>,
        String,
        serde_json::Value,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
        Option<String>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as(
        "SELECT id, suggested_name, process_name, host, component_type, metadata,
                check_cmd, start_cmd, stop_cmd, restart_cmd,
                command_confidence, command_source,
                COALESCE(config_files, '[]'),
                COALESCE(log_files, '[]'),
                matched_service
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .fetch_all(pool)
    .await
}

/// Get draft dependencies.
pub async fn get_draft_dependencies(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<Vec<(DbUuid, DbUuid, DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, DbUuid, String)>(
        "SELECT id, from_component, to_component, inferred_via
         FROM discovery_draft_dependencies WHERE draft_id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .fetch_all(pool)
    .await
}

/// Get user's organization ID.
pub async fn get_user_org_id(pool: &DbPool, user_id: Uuid) -> Result<DbUuid, sqlx::Error> {
    sqlx::query_scalar::<_, DbUuid>("SELECT organization_id FROM users WHERE id = $1")
        .bind(crate::db::bind_id(user_id))
        .fetch_one(pool)
        .await
}

/// Insert a draft.
pub async fn insert_draft(
    pool: &DbPool,
    draft_id: Uuid,
    org_id: DbUuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO discovery_drafts (id, organization_id, name) VALUES ($1, $2, $3)")
        .bind(crate::db::bind_id(draft_id))
        .bind(org_id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Insert a draft component.
#[allow(clippy::too_many_arguments)]
pub async fn insert_draft_component(
    pool: &DbPool,
    comp_id: Uuid,
    draft_id: Uuid,
    agent_id: Option<Uuid>,
    name: &str,
    process_name: &Option<String>,
    host: &Option<String>,
    listening_ports: IntArray,
    component_type: &str,
    check_cmd: &Option<String>,
    start_cmd: &Option<String>,
    stop_cmd: &Option<String>,
    restart_cmd: &Option<String>,
    command_confidence: &str,
    command_source: &Option<String>,
    config_files: &serde_json::Value,
    log_files: &serde_json::Value,
    matched_service: &Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO discovery_draft_components
         (id, draft_id, agent_id, suggested_name, process_name, host,
          listening_ports, component_type,
          check_cmd, start_cmd, stop_cmd, restart_cmd,
          command_confidence, command_source,
          config_files, log_files, matched_service)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(crate::db::bind_id(comp_id))
    .bind(crate::db::bind_id(draft_id))
    .bind(agent_id.map(crate::db::bind_id))
    .bind(name)
    .bind(process_name)
    .bind(host)
    .bind(listening_ports)
    .bind(component_type)
    .bind(check_cmd)
    .bind(start_cmd)
    .bind(stop_cmd)
    .bind(restart_cmd)
    .bind(command_confidence)
    .bind(command_source)
    .bind(config_files)
    .bind(log_files)
    .bind(matched_service)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a draft dependency.
pub async fn insert_draft_dependency(
    pool: &DbPool,
    draft_id: Uuid,
    from_component: Uuid,
    to_component: Uuid,
    inferred_via: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO discovery_draft_dependencies
         (id, draft_id, from_component, to_component, inferred_via)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(draft_id))
    .bind(crate::db::bind_id(from_component))
    .bind(crate::db::bind_id(to_component))
    .bind(inferred_via)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get draft status.
pub async fn get_draft_status(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT status FROM discovery_drafts WHERE id = $1")
        .bind(crate::db::bind_id(draft_id))
        .fetch_optional(pool)
        .await
}

/// Update a draft component.
pub async fn update_draft_component(
    pool: &DbPool,
    comp_id: Uuid,
    draft_id: Uuid,
    name: &str,
    component_type: &str,
    check_cmd: &Option<String>,
    start_cmd: &Option<String>,
    stop_cmd: &Option<String>,
    restart_cmd: &Option<String>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE discovery_draft_components
         SET suggested_name = $2, component_type = $3,
             check_cmd = $5, start_cmd = $6, stop_cmd = $7, restart_cmd = $8
         WHERE id = $1 AND draft_id = $4",
    )
    .bind(crate::db::bind_id(comp_id))
    .bind(name)
    .bind(component_type)
    .bind(crate::db::bind_id(draft_id))
    .bind(check_cmd)
    .bind(start_cmd)
    .bind(stop_cmd)
    .bind(restart_cmd)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Delete a draft dependency.
pub async fn delete_draft_dependency(
    pool: &DbPool,
    dep_id: Uuid,
    draft_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM discovery_draft_dependencies WHERE id = $1 AND draft_id = $2")
            .bind(crate::db::bind_id(dep_id))
            .bind(crate::db::bind_id(draft_id))
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Apply draft queries
// ============================================================================

/// Get draft header for apply.
pub async fn get_draft_for_apply(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<Option<(DbUuid, DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, String, String)>(
        "SELECT id, organization_id, name, status FROM discovery_drafts WHERE id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .fetch_optional(pool)
    .await
}

/// Get first site ID for an organization.
pub async fn get_first_site_id(
    pool: &DbPool,
    org_id: DbUuid,
) -> Result<Option<DbUuid>, sqlx::Error> {
    sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM sites WHERE organization_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await
}

/// Create an application from a draft.
pub async fn create_app_from_draft(
    pool: &DbPool,
    app_id: Uuid,
    org_id: DbUuid,
    site_id: DbUuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO applications (id, organization_id, site_id, name, mode)
         VALUES ($1, $2, $3, $4, 'advisory')",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(org_id)
    .bind(crate::db::bind_id(site_id))
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get draft components for apply (postgres).
#[cfg(feature = "postgres")]
pub async fn get_draft_comps_for_apply(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<DbUuid>,
        Option<String>,
        Option<String>,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
    )>,
    sqlx::Error,
> {
    sqlx::query_as(
        "SELECT id, suggested_name, process_name, host, component_type, agent_id,
                check_cmd, start_cmd, stop_cmd,
                COALESCE(config_files, '[]'::jsonb),
                COALESCE(log_files, '[]'::jsonb)
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(pool)
    .await
}

/// Get draft components for apply (sqlite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn get_draft_comps_for_apply(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<DbUuid>,
        Option<String>,
        Option<String>,
        Option<String>,
        serde_json::Value,
        serde_json::Value,
    )>,
    sqlx::Error,
> {
    sqlx::query_as(
        "SELECT id, suggested_name, process_name, host, component_type, agent_id,
                check_cmd, start_cmd, stop_cmd,
                COALESCE(config_files, '[]'),
                COALESCE(log_files, '[]')
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .fetch_all(pool)
    .await
}

/// Insert a real component from draft.
pub async fn insert_component_from_draft(
    pool: &DbPool,
    comp_id: Uuid,
    app_id: Uuid,
    name: &str,
    comp_type: &str,
    host: &Option<String>,
    agent_id: &Option<Uuid>,
    check_cmd: &Option<String>,
    start_cmd: &Option<String>,
    stop_cmd: &Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO components (id, application_id, name, component_type, host, agent_id,
                                 check_cmd, start_cmd, stop_cmd, current_state)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'UNKNOWN')",
    )
    .bind(crate::db::bind_id(comp_id))
    .bind(crate::db::bind_id(app_id))
    .bind(name)
    .bind(comp_type)
    .bind(host)
    .bind(agent_id.map(crate::db::bind_id))
    .bind(check_cmd)
    .bind(start_cmd)
    .bind(stop_cmd)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a custom command for a component.
pub async fn insert_component_command(
    pool: &DbPool,
    comp_id: Uuid,
    label: &str,
    command: &str,
) -> Result<(), sqlx::Error> {
    let _ = sqlx::query(
        "INSERT INTO component_commands (id, component_id, name, command) VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(comp_id))
    .bind(label)
    .bind(command)
    .execute(pool)
    .await;
    Ok(())
}

/// Get draft dependencies for apply.
pub async fn get_draft_deps_for_apply(
    pool: &DbPool,
    draft_id: Uuid,
) -> Result<Vec<(DbUuid, DbUuid)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT from_component, to_component
         FROM discovery_draft_dependencies WHERE draft_id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .fetch_all(pool)
    .await
}

/// Insert a real dependency.
pub async fn insert_dependency(
    pool: &DbPool,
    app_id: Uuid,
    from_id: Uuid,
    to_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dependencies (id, application_id, from_component_id, to_component_id)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(from_id))
    .bind(crate::db::bind_id(to_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark draft as applied.
pub async fn mark_draft_applied(
    pool: &DbPool,
    draft_id: Uuid,
    app_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE discovery_drafts SET status = 'applied', applied_app_id = $2 WHERE id = $1",
    )
    .bind(crate::db::bind_id(draft_id))
    .bind(crate::db::bind_id(app_id))
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Snapshot schedule queries
// ============================================================================

/// Row type for schedule queries.
#[derive(Debug, sqlx::FromRow)]
pub struct DiscoveryScheduleRow {
    pub id: DbUuid,
    pub name: String,
    pub agent_ids: UuidArray,
    pub frequency: String,
    pub cron_expression: Option<String>,
    pub enabled: bool,
    pub retention_days: i32,
    pub last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List snapshot schedules for an organization.
pub async fn list_snapshot_schedules(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Vec<DiscoveryScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, DiscoveryScheduleRow>(
        "SELECT id, name, agent_ids, frequency, cron_expression, enabled,
                retention_days, last_run_at, next_run_at, created_at
         FROM snapshot_schedules
         WHERE organization_id = $1
         ORDER BY created_at DESC",
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_all(pool)
    .await
}

/// Insert a snapshot schedule.
pub async fn insert_snapshot_schedule(
    pool: &DbPool,
    schedule_id: Uuid,
    org_id: Uuid,
    name: &str,
    agent_ids: UuidArray,
    frequency: &str,
    retention_days: i32,
    next_run: chrono::DateTime<chrono::Utc>,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO snapshot_schedules (id, organization_id, name, agent_ids, frequency, retention_days, next_run_at, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(crate::db::bind_id(schedule_id))
    .bind(crate::db::bind_id(org_id))
    .bind(name)
    .bind(agent_ids)
    .bind(frequency)
    .bind(retention_days)
    .bind(next_run)
    .bind(crate::db::bind_id(created_by))
    .execute(pool)
    .await?;
    Ok(())
}

/// Check if a snapshot schedule exists for an organization.
pub async fn schedule_exists(
    pool: &DbPool,
    schedule_id: Uuid,
    org_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM snapshot_schedules WHERE id = $1 AND organization_id = $2)",
    )
    .bind(crate::db::bind_id(schedule_id))
    .bind(crate::db::bind_id(org_id))
    .fetch_one(pool)
    .await
}

/// Update schedule name.
pub async fn update_schedule_name(pool: &DbPool, id: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE snapshot_schedules SET name = $2 WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update schedule agent_ids.
pub async fn update_schedule_agent_ids(
    pool: &DbPool,
    id: Uuid,
    agent_ids: UuidArray,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE snapshot_schedules SET agent_ids = $2 WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .bind(agent_ids)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update schedule frequency.
pub async fn update_schedule_frequency(
    pool: &DbPool,
    id: Uuid,
    frequency: &str,
    next_run: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE snapshot_schedules SET frequency = $2, next_run_at = $3 WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .bind(frequency)
        .bind(next_run)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get schedule frequency.
pub async fn get_schedule_frequency(pool: &DbPool, id: Uuid) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT frequency FROM snapshot_schedules WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .fetch_one(pool)
        .await
}

/// Enable/disable schedule.
pub async fn update_schedule_enabled(
    pool: &DbPool,
    id: Uuid,
    enabled: bool,
    next_run: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    if enabled {
        sqlx::query("UPDATE snapshot_schedules SET enabled = $2, next_run_at = $3 WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .bind(enabled)
            .bind(next_run)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("UPDATE snapshot_schedules SET enabled = $2 WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .bind(enabled)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Update schedule retention days.
pub async fn update_schedule_retention(
    pool: &DbPool,
    id: Uuid,
    retention_days: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE snapshot_schedules SET retention_days = $2 WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .bind(retention_days)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a snapshot schedule.
pub async fn delete_snapshot_schedule(
    pool: &DbPool,
    id: Uuid,
    org_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM snapshot_schedules WHERE id = $1 AND organization_id = $2")
            .bind(crate::db::bind_id(id))
            .bind(crate::db::bind_id(org_id))
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Snapshot queries
// ============================================================================

/// Row type for snapshot queries.
#[derive(Debug, sqlx::FromRow)]
pub struct SnapshotRow {
    pub id: DbUuid,
    pub schedule_id: DbUuid,
    pub schedule_name: String,
    pub agent_ids: UuidArray,
    pub report_ids: UuidArray,
    pub captured_at: chrono::DateTime<chrono::Utc>,
}

/// List snapshots, optionally filtered by schedule_id.
pub async fn list_snapshots(
    pool: &DbPool,
    org_id: Uuid,
    schedule_id: Option<DbUuid>,
) -> Result<Vec<SnapshotRow>, sqlx::Error> {
    if let Some(schedule_id) = schedule_id {
        sqlx::query_as::<_, SnapshotRow>(
            "SELECT ss.id, ss.schedule_id, sch.name as schedule_name, ss.agent_ids, ss.report_ids, ss.captured_at
             FROM scheduled_snapshots ss
             JOIN snapshot_schedules sch ON sch.id = ss.schedule_id
             WHERE ss.organization_id = $1 AND ss.schedule_id = $2
             ORDER BY ss.captured_at DESC
             LIMIT 100",
        )
        .bind(crate::db::bind_id(org_id))
        .bind(schedule_id)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, SnapshotRow>(
            "SELECT ss.id, ss.schedule_id, sch.name as schedule_name, ss.agent_ids, ss.report_ids, ss.captured_at
             FROM scheduled_snapshots ss
             JOIN snapshot_schedules sch ON sch.id = ss.schedule_id
             WHERE ss.organization_id = $1
             ORDER BY ss.captured_at DESC
             LIMIT 100",
        )
        .bind(crate::db::bind_id(org_id))
        .fetch_all(pool)
        .await
    }
}

/// Get snapshot correlation result.
#[cfg(feature = "postgres")]
pub async fn get_snapshot_correlation(
    pool: &DbPool,
    snapshot_id: DbUuid,
    org_id: Uuid,
) -> Result<Option<(serde_json::Value,)>, sqlx::Error> {
    sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT COALESCE(correlation_result, '{}'::jsonb)
         FROM scheduled_snapshots
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(snapshot_id)
    .bind(crate::db::bind_id(org_id))
    .fetch_optional(pool)
    .await
}

/// Get snapshot correlation result (sqlite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn get_snapshot_correlation(
    pool: &DbPool,
    snapshot_id: DbUuid,
    org_id: Uuid,
) -> Result<Option<(serde_json::Value,)>, sqlx::Error> {
    sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT COALESCE(correlation_result, '{}')
         FROM scheduled_snapshots
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(snapshot_id)
    .bind(crate::db::bind_id(org_id))
    .fetch_optional(pool)
    .await
}

// ============================================================================
// File content queries
// ============================================================================

/// Check if an agent exists and belongs to an organization.
pub async fn agent_exists_in_org(
    pool: &DbPool,
    agent_id: DbUuid,
    org_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND organization_id = $2)",
    )
    .bind(agent_id)
    .bind(crate::db::bind_id(org_id))
    .fetch_one(pool)
    .await
}
