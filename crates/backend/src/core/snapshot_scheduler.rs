//! Background task that executes scheduled discovery snapshots.
//!
//! Runs periodically, checks for schedules whose next_run_at has passed,
//! triggers discovery scans on the specified agents, and stores the results.

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[cfg(feature = "postgres")]
use crate::db::UuidArray;
use crate::AppState;

/// Row returned when querying for due schedules.
#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct DueSchedule {
    id: Uuid,
    organization_id: Uuid,
    name: String,
    agent_ids: UuidArray,
    frequency: String,
    retention_days: i32,
}

/// Calculate next run time based on frequency.
#[cfg(feature = "postgres")]
fn calculate_next_run(frequency: &str) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, Duration, Timelike, Utc};

    let now = Utc::now();

    match frequency {
        "hourly" => now
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .map(|t| t + Duration::hours(1))
            .unwrap_or(now + Duration::hours(1)),
        "daily" => now
            .with_hour(0)
            .and_then(|t| t.with_minute(0))
            .and_then(|t| t.with_second(0))
            .map(|t| t + Duration::days(1))
            .unwrap_or(now + Duration::days(1)),
        "weekly" => {
            let days_until_sunday = (7 - now.weekday().num_days_from_sunday()) % 7;
            let days_until_sunday = if days_until_sunday == 0 {
                7
            } else {
                days_until_sunday
            };
            now.with_hour(0)
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_second(0))
                .map(|t| t + Duration::days(days_until_sunday as i64))
                .unwrap_or(now + Duration::days(7))
        }
        "monthly" => {
            let next_month = if now.month() == 12 {
                now.with_year(now.year() + 1).and_then(|t| t.with_month(1))
            } else {
                now.with_month(now.month() + 1)
            };
            next_month
                .and_then(|t| t.with_day(1))
                .and_then(|t| t.with_hour(0))
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_second(0))
                .unwrap_or(now + Duration::days(30))
        }
        _ => now + Duration::days(1),
    }
}

/// Start the snapshot scheduler background task.
/// Runs every `check_interval`, queries for schedules whose next_run_at has passed,
/// and executes them.
///
/// NOTE: Currently only fully implemented for PostgreSQL. SQLite support is partial.
#[cfg(feature = "postgres")]
pub async fn run_snapshot_scheduler(state: Arc<AppState>, check_interval: Duration) {
    let mut interval = tokio::time::interval(check_interval);

    loop {
        interval.tick().await;

        if let Err(e) = execute_due_schedules(&state).await {
            tracing::error!("Snapshot scheduler error: {}", e);
        }

        if let Err(e) = cleanup_expired_snapshots(&state).await {
            tracing::error!("Snapshot cleanup error: {}", e);
        }
    }
}

/// SQLite stub: snapshot scheduling not yet fully implemented for SQLite.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn run_snapshot_scheduler(_state: Arc<AppState>, check_interval: Duration) {
    tracing::warn!(
        "Snapshot scheduler disabled: not yet implemented for SQLite backend. \
         Scheduled discovery snapshots will not run automatically."
    );

    // Sleep forever to keep the task alive but do nothing
    let mut interval = tokio::time::interval(check_interval);
    loop {
        interval.tick().await;
        // No-op for SQLite
    }
}

/// Find and execute all schedules that are due.
#[cfg(feature = "postgres")]
async fn execute_due_schedules(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    // Find schedules where next_run_at <= now() and enabled = true
    let due_schedules = sqlx::query_as::<_, DueSchedule>(
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
    .fetch_all(&state.db)
    .await?;

    for schedule in due_schedules {
        tracing::info!(
            schedule_id = %schedule.id,
            schedule_name = %schedule.name,
            agents = schedule.agent_ids.0.len(),
            "Executing scheduled snapshot"
        );

        if let Err(e) = execute_single_schedule(state, &schedule).await {
            tracing::error!(
                schedule_id = %schedule.id,
                error = %e,
                "Failed to execute scheduled snapshot"
            );
        }
    }

    Ok(())
}

/// Execute a single schedule: trigger discovery, store snapshot.
#[cfg(feature = "postgres")]
async fn execute_single_schedule(
    state: &Arc<AppState>,
    schedule: &DueSchedule,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let request_id = Uuid::new_v4();

    // Trigger discovery on all agents in the schedule
    let mut successful_agents = Vec::new();
    for agent_id in &schedule.agent_ids.0 {
        let msg = appcontrol_common::BackendMessage::RequestDiscovery { request_id };
        if state.ws_hub.send_to_agent(*agent_id, msg) {
            successful_agents.push(*agent_id);
        }
    }

    // Wait a bit for reports to come in (agents should respond quickly)
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Collect the report IDs for agents that were scanned
    let report_ids = fetch_recent_report_ids(&state.db, &successful_agents).await?;

    // Create correlation result for comparison
    let correlation_result = if !report_ids.is_empty() {
        // Simplified correlation - just get the services as a single JSON array
        let services = fetch_services_for_correlation(&state.db, &report_ids).await?;

        serde_json::json!({
            "services": services,
            "agents_analyzed": successful_agents.len(),
        })
    } else {
        serde_json::json!({
            "services": [],
            "agents_analyzed": 0,
        })
    };

    // Calculate expiration date
    let expires_at = chrono::Utc::now() + chrono::Duration::days(schedule.retention_days as i64);

    // Create the scheduled snapshot record
    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO scheduled_snapshots
            (id, schedule_id, organization_id, agent_ids, report_ids, correlation_result, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(snapshot_id)
    .bind(schedule.id)
    .bind(schedule.organization_id)
    .bind(UuidArray::from(successful_agents.clone()))
    .bind(UuidArray::from(report_ids.clone()))
    .bind(&correlation_result)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    // Update the schedule: set last_run_at and calculate next_run_at
    let next_run = calculate_next_run(&schedule.frequency);
    update_schedule_after_run(&state.db, schedule.id, next_run).await?;

    tracing::info!(
        schedule_id = %schedule.id,
        snapshot_id = %snapshot_id,
        reports = report_ids.len(),
        "Scheduled snapshot completed"
    );

    Ok(())
}

// ============================================================================
// Database-specific helper functions
// ============================================================================

#[cfg(feature = "postgres")]
async fn fetch_recent_report_ids(
    db: &crate::db::DbPool,
    agent_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
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
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[allow(dead_code)] // Will be used when snapshot scheduler is fully implemented for SQLite
async fn fetch_recent_report_ids(
    db: &crate::db::DbPool,
    agent_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
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
    let rows: Vec<String> = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|s| Uuid::parse_str(&s).ok())
        .collect())
}

#[cfg(feature = "postgres")]
async fn fetch_services_for_correlation(
    db: &crate::db::DbPool,
    report_ids: &[Uuid],
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
    .fetch_one(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[allow(dead_code)] // Will be used when snapshot scheduler is fully implemented for SQLite
async fn fetch_services_for_correlation(
    db: &crate::db::DbPool,
    report_ids: &[Uuid],
) -> Result<serde_json::Value, sqlx::Error> {
    // SQLite: simplified approach - fetch reports and process in Rust
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
    let rows: Vec<(String, String)> = q.fetch_all(db).await?;

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

#[cfg(feature = "postgres")]
async fn update_schedule_after_run(
    db: &crate::db::DbPool,
    schedule_id: Uuid,
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
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[allow(dead_code)] // Will be used when snapshot scheduler is fully implemented for SQLite
async fn update_schedule_after_run(
    db: &crate::db::DbPool,
    schedule_id: Uuid,
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
    .execute(db)
    .await?;
    Ok(())
}

/// Clean up expired snapshots based on retention_days.
#[cfg(feature = "postgres")]
async fn cleanup_expired_snapshots(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM scheduled_snapshots
        WHERE expires_at IS NOT NULL
          AND expires_at < now()
        "#,
    )
    .execute(&state.db)
    .await?;

    let deleted = result.rows_affected();
    if deleted > 0 {
        tracing::info!(count = deleted, "Cleaned up expired snapshots");
    }

    Ok(())
}
