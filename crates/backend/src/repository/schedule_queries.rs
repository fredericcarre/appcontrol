//! Query functions for schedule domain. All sqlx queries live here.

#![allow(unused_imports, dead_code, clippy::too_many_arguments)]
use crate::db::{DbJson, DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

/// Get the application name.
pub async fn get_app_name(pool: &DbPool, app_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT name FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
}

/// Get a component's display name (display_name or name fallback).
pub async fn get_component_display_name(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(display_name, name) FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
}

/// Get the application_id for a component.
pub async fn get_component_app_id(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row =
            sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
                .bind(DbUuid::from(component_id))
                .fetch_optional(pool)
                .await?;
        Ok(row.map(|v| v.into_inner()))
    }
}

/// Get the organization_id for an application.
pub async fn get_app_org_id(pool: &DbPool, app_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>("SELECT organization_id FROM applications WHERE id = $1")
            .bind(app_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT organization_id FROM applications WHERE id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|v| v.into_inner()))
    }
}

// ============================================================================
// Operation schedule queries (core/operation_scheduler.rs)
// ============================================================================

/// Row returned when querying for due operation schedules.
#[derive(Debug, sqlx::FromRow)]
pub struct DueOperationSchedule {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub application_id: Option<DbUuid>,
    pub component_id: Option<DbUuid>,
    pub name: String,
    pub operation: String,
    pub cron_expression: String,
    pub timezone: String,
}

/// Fetch due operation schedules (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_due_operation_schedules(
    pool: &DbPool,
) -> Result<Vec<DueOperationSchedule>, sqlx::Error> {
    sqlx::query_as::<_, DueOperationSchedule>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, operation, cron_expression, timezone
        FROM operation_schedules
        WHERE is_enabled = true
          AND next_run_at IS NOT NULL
          AND next_run_at <= now()
        ORDER BY next_run_at ASC
        LIMIT 10
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await
}

// ============================================================================
// Snapshot schedule queries (core/snapshot_scheduler.rs)
// ============================================================================

/// Fetch due snapshot schedules (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_due_snapshot_schedules<
    T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
>(
    pool: &DbPool,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        r#"
        SELECT id, organization_id, name, agent_ids, frequency, retention_days
        FROM snapshot_schedules
        WHERE enabled = true
          AND next_run_at IS NOT NULL
          AND next_run_at <= now()
        ORDER BY next_run_at ASC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Fetch most recent discovery report IDs per agent (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_recent_report_ids(
    pool: &DbPool,
    agent_ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT DISTINCT ON (agent_id) id
        FROM discovery_reports
        WHERE agent_id = ANY($1)
          AND scanned_at > now() - interval '1 minute'
        ORDER BY agent_id, scanned_at DESC
        "#,
    )
    .bind(agent_ids)
    .fetch_all(pool)
    .await
}

/// Fetch most recent discovery report IDs per agent (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_recent_report_ids(
    pool: &DbPool,
    agent_ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=agent_ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        r#"
        SELECT id FROM discovery_reports
        WHERE agent_id IN ({})
          AND scanned_at > datetime('now', '-1 minute')
        GROUP BY agent_id
        HAVING scanned_at = MAX(scanned_at)
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_scalar::<_, String>(&query);
    for id in agent_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<String> = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|s| uuid::Uuid::parse_str(&s).ok())
        .collect())
}

/// Fetch services from discovery reports for correlation (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_services_for_correlation(
    pool: &DbPool,
    report_ids: &[uuid::Uuid],
) -> Result<serde_json::Value, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT COALESCE(jsonb_agg(svc), '[]'::jsonb)
        FROM (
            SELECT
                r.hostname,
                p->>'name' as process_name,
                p->'listening_ports' as ports,
                p->'technology_hint' as technology_hint
            FROM discovery_reports r,
                 jsonb_array_elements(r.report->'processes') p
            WHERE r.id = ANY($1)
              AND p->'listening_ports' IS NOT NULL
              AND jsonb_array_length(p->'listening_ports') > 0
        ) svc
        "#,
    )
    .bind(report_ids)
    .fetch_one(pool)
    .await
}

/// Fetch services from discovery reports for correlation (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_services_for_correlation(
    pool: &DbPool,
    report_ids: &[uuid::Uuid],
) -> Result<serde_json::Value, sqlx::Error> {
    if report_ids.is_empty() {
        return Ok(serde_json::json!([]));
    }
    let placeholders: Vec<String> = (1..=report_ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        "SELECT hostname, report FROM discovery_reports WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String)>(&query);
    for id in report_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String, String)> = q.fetch_all(pool).await?;

    let mut services = Vec::new();
    for (hostname, report_str) in rows {
        if let Ok(report) = serde_json::from_str::<serde_json::Value>(&report_str) {
            if let Some(processes) = report.get("processes").and_then(|p| p.as_array()) {
                for p in processes {
                    if let Some(ports) = p.get("listening_ports").and_then(|lp| lp.as_array()) {
                        if !ports.is_empty() {
                            services.push(serde_json::json!({
                                "hostname": hostname,
                                "process_name": p.get("name"),
                                "ports": ports,
                                "technology_hint": p.get("technology_hint"),
                            }));
                        }
                    }
                }
            }
        }
    }
    Ok(serde_json::Value::Array(services))
}

/// Insert a scheduled snapshot record (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn insert_scheduled_snapshot(
    pool: &DbPool,
    snapshot_id: uuid::Uuid,
    schedule_id: DbUuid,
    organization_id: DbUuid,
    agent_ids: &crate::db::UuidArray,
    report_ids: &crate::db::UuidArray,
    correlation_result: &serde_json::Value,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO scheduled_snapshots
            (id, schedule_id, organization_id, agent_ids, report_ids, correlation_result, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(snapshot_id)
    .bind(schedule_id)
    .bind(organization_id)
    .bind(agent_ids)
    .bind(report_ids)
    .bind(correlation_result)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update snapshot schedule after run (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn update_snapshot_schedule_after_run(
    pool: &DbPool,
    schedule_id: DbUuid,
    next_run: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE snapshot_schedules
        SET last_run_at = now(),
            next_run_at = $2
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .bind(next_run)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update snapshot schedule after run (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_snapshot_schedule_after_run(
    pool: &DbPool,
    schedule_id: DbUuid,
    next_run: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE snapshot_schedules
        SET last_run_at = datetime('now'),
            next_run_at = $2
        WHERE id = $1
        "#,
    )
    .bind(schedule_id.to_string())
    .bind(next_run.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete expired snapshots (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn cleanup_expired_snapshots(pool: &DbPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM scheduled_snapshots
        WHERE expires_at IS NOT NULL
          AND expires_at < now()
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Get application name (common helper).
pub async fn get_app_name_by_id(pool: &DbPool, app_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT name FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Get component display name (common helper).
pub async fn get_comp_display_name(pool: &DbPool, comp_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(display_name, name) FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(comp_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Get application_id from component.
pub async fn get_component_app_id_sched(
    pool: &DbPool,
    comp_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(crate::db::bind_id(comp_id))
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(|u| u.into_inner()))
}

/// List schedules for an application (enabled only).
pub async fn list_app_schedules(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<crate::api::schedules::ScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, crate::api::schedules::ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE application_id = $1 AND is_enabled = true
        ORDER BY next_run_at ASC NULLS LAST
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// List all schedules for an application (including disabled).
pub async fn list_app_schedules_all(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<crate::api::schedules::ScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, crate::api::schedules::ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules
        WHERE application_id = $1
        ORDER BY next_run_at ASC NULLS LAST
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Get organization_id for an application (for schedule).
pub async fn get_org_id_for_app_sched(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, DbUuid>("SELECT organization_id FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(|u| u.into_inner()))
}

/// Create an operation schedule.
pub async fn create_operation_schedule(
    pool: &DbPool,
    id: Uuid,
    org_id: Uuid,
    app_id: Uuid,
    comp_id: Option<Uuid>,
    name: &str,
    description: Option<&str>,
    operation: &str,
    cron_expression: &str,
    timezone: &str,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO operation_schedules
        (id, organization_id, application_id, component_id, name, description, operation, cron_expression, timezone, next_run_at, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(crate::db::bind_id(id))
    .bind(crate::db::bind_id(org_id))
    .bind(crate::db::bind_id(app_id))
    .bind(comp_id.map(crate::db::bind_id))
    .bind(name)
    .bind(description)
    .bind(operation)
    .bind(cron_expression)
    .bind(timezone)
    .bind(next_run_at)
    .bind(crate::db::bind_id(created_by))
    .execute(pool)
    .await?;
    Ok(())
}

/// Get schedule by ID.
pub async fn get_schedule_by_id(
    pool: &DbPool,
    id: Uuid,
) -> Result<Option<crate::api::schedules::ScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, crate::api::schedules::ScheduleRow>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules WHERE id = $1
        "#,
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(pool)
    .await
}

/// Toggle schedule enabled/disabled.
pub async fn toggle_schedule(
    pool: &DbPool,
    id: Uuid,
    enabled: bool,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    let sql = format!(
        "UPDATE operation_schedules SET is_enabled = $2, next_run_at = $3, updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    );
    sqlx::query(&sql)
        .bind(crate::db::bind_id(id))
        .bind(enabled)
        .bind(next_run_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update schedule cron/timezone/name/description.
pub async fn update_schedule(
    pool: &DbPool,
    id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    cron_expression: Option<&str>,
    timezone: Option<&str>,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    let sql = format!(
        r#"UPDATE operation_schedules SET
           name = COALESCE($2, name),
           description = COALESCE($3, description),
           cron_expression = COALESCE($4, cron_expression),
           timezone = COALESCE($5, timezone),
           next_run_at = COALESCE($6, next_run_at),
           updated_at = {}
           WHERE id = $1"#,
        crate::db::sql::now()
    );
    sqlx::query(&sql)
        .bind(crate::db::bind_id(id))
        .bind(name)
        .bind(description)
        .bind(cron_expression)
        .bind(timezone)
        .bind(next_run_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a schedule.
pub async fn delete_schedule(pool: &DbPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM operation_schedules WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Get execution history for a schedule.
pub async fn get_schedule_executions(
    pool: &DbPool,
    schedule_id: Uuid,
    limit: i64,
) -> Result<Vec<crate::api::schedules::ExecutionRow>, sqlx::Error> {
    sqlx::query_as::<_, crate::api::schedules::ExecutionRow>(
        r#"
        SELECT id, schedule_id, action_log_id, executed_at, status, message, duration_ms
        FROM operation_schedule_executions
        WHERE schedule_id = $1
        ORDER BY executed_at DESC
        LIMIT $2
        "#,
    )
    .bind(crate::db::bind_id(schedule_id))
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Fetch due operation schedules (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_due_operation_schedules(
    pool: &DbPool,
) -> Result<Vec<DueOperationSchedule>, sqlx::Error> {
    sqlx::query_as::<_, DueOperationSchedule>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, operation, cron_expression, timezone
        FROM operation_schedules
        WHERE is_enabled = 1
          AND next_run_at IS NOT NULL
          AND next_run_at <= datetime('now')
        ORDER BY next_run_at ASC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await
}

// ============================================================================
// Additional schedule queries (migrated from api/schedules.rs)
// ============================================================================

use crate::api::schedules::{ExecutionRow, ScheduleRow};

/// Fetch component schedules (all).
pub async fn list_component_schedules_all(
    pool: &DbPool,
    comp_id: Uuid,
) -> Result<Vec<ScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, ScheduleRow>(
        r#"SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules WHERE component_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(crate::db::bind_id(comp_id))
    .fetch_all(pool)
    .await
}

/// Fetch component schedules (enabled only).
pub async fn list_component_schedules_enabled(
    pool: &DbPool,
    comp_id: Uuid,
) -> Result<Vec<ScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, ScheduleRow>(
        r#"SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules WHERE component_id = $1 AND is_enabled = true ORDER BY created_at DESC"#,
    ).bind(crate::db::bind_id(comp_id)).fetch_all(pool).await
}

/// Get component display name.
pub async fn get_comp_display_name_for_sched(pool: &DbPool, comp_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT COALESCE(display_name, name) FROM components WHERE id = $1")
        .bind(crate::db::bind_id(comp_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Get org_id from component's application.
pub async fn get_org_id_for_component_app(
    pool: &DbPool,
    app_id: DbUuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>("SELECT organization_id FROM applications WHERE id = $1")
            .bind(*app_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT organization_id FROM applications WHERE id = $1",
        )
        .bind(app_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|v| v.into_inner()))
    }
}

/// Create a component schedule.
pub async fn create_component_schedule(
    pool: &DbPool,
    schedule_id: Uuid,
    org_id: Uuid,
    comp_id: Uuid,
    name: &str,
    description: Option<&str>,
    operation: &str,
    cron_expression: &str,
    timezone: &str,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO operation_schedules
            (id, organization_id, component_id, name, description, operation,
             cron_expression, timezone, is_enabled, next_run_at, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, $9, $10)"#,
    )
    .bind(schedule_id)
    .bind(org_id)
    .bind(crate::db::bind_id(comp_id))
    .bind(name)
    .bind(description)
    .bind(operation)
    .bind(cron_expression)
    .bind(timezone)
    .bind(next_run_at)
    .bind(crate::db::bind_id(created_by))
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch a schedule by ID (generic).
pub async fn fetch_schedule_row(
    pool: &DbPool,
    schedule_id: Uuid,
) -> Result<Option<ScheduleRow>, sqlx::Error> {
    sqlx::query_as::<_, ScheduleRow>(
        r#"SELECT id, organization_id, application_id, component_id, name, description,
               operation, cron_expression, timezone, is_enabled,
               last_run_at, next_run_at, last_run_status, last_run_message,
               created_by, created_at, updated_at
        FROM operation_schedules WHERE id = $1"#,
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
}

/// Update schedule fields.
pub async fn update_schedule_fields(
    pool: &DbPool,
    schedule_id: Uuid,
    name: &str,
    description: &Option<String>,
    operation: &str,
    cron_expression: &str,
    timezone: &str,
    is_enabled: bool,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE operation_schedules SET name = $2, description = $3, operation = $4, cron_expression = $5,
             timezone = $6, is_enabled = $7, next_run_at = $8, updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    ))
    .bind(schedule_id).bind(name).bind(description).bind(operation)
    .bind(cron_expression).bind(timezone).bind(is_enabled).bind(next_run_at)
    .execute(pool).await?;
    Ok(())
}

/// Delete a schedule (operation_schedules).
pub async fn delete_operation_schedule(
    pool: &DbPool,
    schedule_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM operation_schedules WHERE id = $1")
        .bind(schedule_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Toggle schedule enabled + next_run_at.
pub async fn toggle_schedule_enabled(
    pool: &DbPool,
    schedule_id: Uuid,
    enabled: bool,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE operation_schedules SET is_enabled = $2, next_run_at = $3, updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    ))
    .bind(schedule_id).bind(enabled).bind(next_run_at).execute(pool).await?;
    Ok(())
}

/// Set next_run_at to now (for run-now).
pub async fn set_schedule_run_now(pool: &DbPool, schedule_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE operation_schedules SET next_run_at = {now}, updated_at = {now} WHERE id = $1",
        now = crate::db::sql::now()
    ))
    .bind(schedule_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// List schedule executions.
pub async fn list_executions(
    pool: &DbPool,
    schedule_id: Uuid,
) -> Result<Vec<ExecutionRow>, sqlx::Error> {
    sqlx::query_as::<_, ExecutionRow>(
        r#"SELECT id, schedule_id, action_log_id, executed_at, status, message, duration_ms
        FROM operation_schedule_executions WHERE schedule_id = $1
        ORDER BY executed_at DESC LIMIT 100"#,
    )
    .bind(schedule_id)
    .fetch_all(pool)
    .await
}
