//! Background task that executes scheduled discovery snapshots.
//!
//! Runs periodically, checks for schedules whose next_run_at has passed,
//! triggers discovery scans on the specified agents, and stores the results.

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::db::DbUuid;
#[cfg(feature = "postgres")]
use crate::db::UuidArray;
use crate::AppState;

/// Row returned when querying for due schedules.
#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct DueSchedule {
    id: DbUuid,
    organization_id: DbUuid,
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
    let due_schedules = crate::repository::schedule_queries::fetch_due_snapshot_schedules::<DueSchedule>(&state.db).await?;

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
    let report_ids = crate::repository::schedule_queries::fetch_recent_report_ids(&state.db, &successful_agents).await?;

    // Create correlation result for comparison
    let correlation_result = if !report_ids.is_empty() {
        // Simplified correlation - just get the services as a single JSON array
        let services = crate::repository::schedule_queries::fetch_services_for_correlation(&state.db, &report_ids).await?;

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
    crate::repository::schedule_queries::insert_scheduled_snapshot(
        &state.db, snapshot_id, schedule.id, schedule.organization_id,
        &UuidArray::from(successful_agents.clone()),
        &UuidArray::from(report_ids.clone()),
        &correlation_result, expires_at,
    ).await?;

    // Update the schedule: set last_run_at and calculate next_run_at
    let next_run = calculate_next_run(&schedule.frequency);
    crate::repository::schedule_queries::update_snapshot_schedule_after_run(&state.db, schedule.id, next_run).await?;

    tracing::info!(
        schedule_id = %schedule.id,
        snapshot_id = %snapshot_id,
        reports = report_ids.len(),
        "Scheduled snapshot completed"
    );

    Ok(())
}

// Database helper functions moved to repository::schedule_queries

/// Clean up expired snapshots based on retention_days.
#[cfg(feature = "postgres")]
async fn cleanup_expired_snapshots(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    let deleted = crate::repository::schedule_queries::cleanup_expired_snapshots(&state.db).await?;
    if deleted > 0 {
        tracing::info!(count = deleted, "Cleaned up expired snapshots");
    }
    Ok(())
}
