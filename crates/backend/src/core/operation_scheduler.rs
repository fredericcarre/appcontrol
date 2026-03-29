//! Background job that executes scheduled operations (start/stop/restart).
//!
//! Checks every 60 seconds for schedules where next_run_at <= now() and is_enabled = true.
//! When a schedule is due, executes the operation via the sequencer, logs to action_log
//! with triggered_by='scheduler', and updates the schedule with last_run_at/next_run_at.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule as CronSchedule;
use uuid::Uuid;

use crate::db::{DbPool, DbUuid};
use crate::middleware::audit;
use crate::AppState;

/// Row returned when querying for due schedules.
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct DueSchedule {
    id: DbUuid,
    organization_id: DbUuid,
    application_id: Option<DbUuid>,
    component_id: Option<DbUuid>,
    name: String,
    operation: String,
    cron_expression: String,
    timezone: String,
}

/// Start the operation scheduler background task.
/// Runs every `check_interval`, queries for schedules whose next_run_at has passed,
/// and executes them.
pub async fn run_operation_scheduler(state: Arc<AppState>, check_interval: Duration) {
    let mut interval = tokio::time::interval(check_interval);

    loop {
        interval.tick().await;

        if let Err(e) = execute_due_schedules(&state).await {
            tracing::error!("Operation scheduler error: {}", e);
        }
    }
}

/// Find and execute all schedules that are due.
async fn execute_due_schedules(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    // Find schedules where next_run_at <= now() and enabled = true
    // Use FOR UPDATE SKIP LOCKED to prevent multiple backend instances from picking up the same schedule
    #[cfg(feature = "postgres")]
    let due_schedules = sqlx::query_as::<_, DueSchedule>(
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
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let due_schedules = sqlx::query_as::<_, DueSchedule>(
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
    .fetch_all(&state.db)
    .await?;

    for schedule in due_schedules {
        tracing::info!(
            schedule_id = %schedule.id,
            schedule_name = %schedule.name,
            operation = %schedule.operation,
            "Executing scheduled operation"
        );

        let result = execute_single_schedule(state, &schedule).await;
        if let Err(e) = update_schedule_after_run(&state.db, &schedule, result).await {
            tracing::error!(
                schedule_id = %schedule.id,
                error = %e,
                "Failed to update schedule status after execution"
            );
        }
    }

    Ok(())
}

/// Execute a single schedule: log action, execute operation, update status.
async fn execute_single_schedule(state: &Arc<AppState>, schedule: &DueSchedule) -> ScheduleResult {
    let start_time = std::time::Instant::now();

    // Determine the target (application or component)
    let (resource_type, resource_id, target_name) = if let Some(app_id) = schedule.application_id {
        let app_name: Option<String> =
            sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
                .bind(app_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
        (
            "application",
            app_id,
            app_name.unwrap_or_else(|| app_id.to_string()),
        )
    } else if let Some(comp_id) = schedule.component_id {
        let comp_name: Option<String> =
            sqlx::query_scalar("SELECT COALESCE(display_name, name) FROM components WHERE id = $1")
                .bind(comp_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
        (
            "component",
            comp_id,
            comp_name.unwrap_or_else(|| comp_id.to_string()),
        )
    } else {
        return Err("Schedule has no target (neither application_id nor component_id)".to_string());
    };

    // For scheduler-triggered actions, we need a system user ID.
    // We use a well-known UUID for the "scheduler" system user.
    // In a proper setup, this should be a real system user created by migration.
    let scheduler_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap();

    // Log action BEFORE executing (critical rule: log before execute)
    let action_name = format!("scheduled_{}", schedule.operation);
    let details = serde_json::json!({
        "schedule_id": schedule.id,
        "schedule_name": schedule.name,
        "triggered_by": "scheduler",
        "target_name": target_name,
    });

    let action_id = match audit::log_action(
        &state.db,
        scheduler_user_id,
        &action_name,
        resource_type,
        resource_id,
        details,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(
                schedule_id = %schedule.id,
                error = %e,
                "Failed to log action for scheduled operation"
            );
            return Err(format!("Failed to log action: {}", e));
        }
    };

    // Execute the operation
    let operation_result = match schedule.operation.as_str() {
        "start" => execute_start(state, schedule).await,
        "stop" => execute_stop(state, schedule).await,
        "restart" => execute_restart(state, schedule).await,
        _ => Err(format!("Unknown operation: {}", schedule.operation)),
    };

    let duration_ms = start_time.elapsed().as_millis() as i32;

    // Update action log with result
    match &operation_result {
        Ok(()) => {
            if let Err(e) = audit::complete_action_success(&state.db, action_id).await {
                tracing::warn!(action_id = %action_id, "Failed to mark action as success: {}", e);
            }
        }
        Err(msg) => {
            if let Err(e) = audit::complete_action_failed(&state.db, action_id, msg).await {
                tracing::warn!(action_id = %action_id, "Failed to mark action as failed: {}", e);
            }
        }
    }

    // Record execution in history table (append-only)
    let execution_id = Uuid::new_v4();
    let (status, message) = match &operation_result {
        Ok(()) => ("success", None),
        Err(msg) => ("failed", Some(msg.as_str())),
    };

    #[cfg(feature = "postgres")]
    let _ = sqlx::query(
        r#"
        INSERT INTO operation_schedule_executions (id, schedule_id, action_log_id, status, message, duration_ms)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(execution_id)
    .bind(schedule.id)
    .bind(action_id)
    .bind(status)
    .bind(message)
    .bind(duration_ms)
    .execute(&state.db)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let _ = sqlx::query(
        r#"
        INSERT INTO operation_schedule_executions (id, schedule_id, action_log_id, status, message, duration_ms)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(execution_id.to_string())
    .bind(schedule.id.to_string())
    .bind(action_id.to_string())
    .bind(status)
    .bind(message)
    .bind(duration_ms)
    .execute(&state.db)
    .await;

    operation_result
}

/// Execute a start operation for the schedule target.
async fn execute_start(state: &Arc<AppState>, schedule: &DueSchedule) -> ScheduleResult {
    if let Some(app_id) = schedule.application_id {
        super::sequencer::execute_start(state, app_id)
            .await
            .map_err(|e| e.to_string())
    } else if let Some(comp_id) = schedule.component_id {
        super::sequencer::start_single_component(state, *comp_id)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("No target specified".to_string())
    }
}

/// Execute a stop operation for the schedule target.
async fn execute_stop(state: &Arc<AppState>, schedule: &DueSchedule) -> ScheduleResult {
    if let Some(app_id) = schedule.application_id {
        super::sequencer::execute_stop(state, app_id)
            .await
            .map_err(|e| e.to_string())
    } else if let Some(comp_id) = schedule.component_id {
        super::sequencer::stop_single_component(state, *comp_id)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("No target specified".to_string())
    }
}

/// Execute a restart operation: stop, then start.
async fn execute_restart(state: &Arc<AppState>, schedule: &DueSchedule) -> ScheduleResult {
    // Stop first
    execute_stop(state, schedule).await?;

    // Brief pause between stop and start
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Then start
    execute_start(state, schedule).await
}

type ScheduleResult = Result<(), String>;

/// Update the schedule after execution: set last_run_at, calculate next_run_at, update status.
async fn update_schedule_after_run(
    db: &DbPool,
    schedule: &DueSchedule,
    result: ScheduleResult,
) -> Result<(), sqlx::Error> {
    let (status, message) = match result {
        Ok(()) => ("success", None),
        Err(msg) => ("failed", Some(msg)),
    };

    // Calculate next run time from cron expression
    let next_run = calculate_next_run(&schedule.cron_expression, &schedule.timezone);

    tracing::debug!(
        schedule_id = %schedule.id,
        cron = %schedule.cron_expression,
        timezone = %schedule.timezone,
        next_run = ?next_run,
        "Calculated next run time"
    );

    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        UPDATE operation_schedules
        SET last_run_at = now(),
            last_run_status = $2,
            last_run_message = $3,
            next_run_at = $4,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(schedule.id)
    .bind(status)
    .bind(&message)
    .bind(next_run)
    .execute(db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"
        UPDATE operation_schedules
        SET last_run_at = datetime('now'),
            last_run_status = $2,
            last_run_message = $3,
            next_run_at = $4,
            updated_at = datetime('now')
        WHERE id = $1
        "#,
    )
    .bind(schedule.id.to_string())
    .bind(status)
    .bind(&message)
    .bind(next_run.map(|dt| dt.to_rfc3339()))
    .execute(db)
    .await?;

    Ok(())
}

/// Convert a 5-field cron expression to 6-field format (adding seconds).
/// The cron crate requires 6 fields: sec min hour day month weekday
/// Standard Unix cron uses 5 fields: min hour day month weekday
pub fn to_6_field_cron(cron_expr: &str) -> String {
    let fields: Vec<&str> = cron_expr.split_whitespace().collect();
    if fields.len() == 5 {
        // Prepend "0" for seconds
        format!("0 {}", cron_expr)
    } else {
        // Already 6 or 7 fields, use as-is
        cron_expr.to_string()
    }
}

/// Check if a cron expression is valid (supports both 5 and 6 field formats).
pub fn is_valid_cron(cron_expr: &str) -> bool {
    let cron_6field = to_6_field_cron(cron_expr);
    cron_6field.parse::<CronSchedule>().is_ok()
}

/// Calculate the next run time from a cron expression and timezone.
pub fn calculate_next_run(cron_expr: &str, timezone: &str) -> Option<DateTime<Utc>> {
    // Parse the timezone (default to UTC if invalid)
    let tz: chrono_tz::Tz = timezone.parse().unwrap_or(chrono_tz::UTC);

    // Convert to 6-field format if needed
    let cron_6field = to_6_field_cron(cron_expr);

    // Parse the cron expression
    let schedule: CronSchedule = match cron_6field.parse() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(cron = cron_expr, cron_6field = %cron_6field, error = %e, "Invalid cron expression");
            return None;
        }
    };

    // Get the next occurrence in the specified timezone
    schedule
        .upcoming(tz)
        .next()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Convert a preset name to a cron expression.
pub fn preset_to_cron(preset: &str) -> Option<&'static str> {
    match preset {
        "daily_7am" => Some("0 7 * * *"),
        "daily_8am" => Some("0 8 * * *"),
        "daily_19h" => Some("0 19 * * *"),
        "daily_22h" => Some("0 22 * * *"),
        "weekdays_7am" => Some("0 7 * * 1-5"),
        "weekdays_8am" => Some("0 8 * * 1-5"),
        "weekdays_19h" => Some("0 19 * * 1-5"),
        "weekdays_22h" => Some("0 22 * * 1-5"),
        "weekly_sunday_3am" => Some("0 3 * * 0"),
        "weekly_saturday_3am" => Some("0 3 * * 6"),
        "monthly_1st_3am" => Some("0 3 1 * *"),
        "every_hour" => Some("0 * * * *"),
        "every_30min" => Some("*/30 * * * *"),
        _ => None,
    }
}

/// Convert a cron expression to a human-readable description.
pub fn cron_to_human(cron_expr: &str) -> String {
    // Simple pattern matching for common cron expressions
    match cron_expr {
        "0 7 * * *" => "Every day at 7:00 AM".to_string(),
        "0 8 * * *" => "Every day at 8:00 AM".to_string(),
        "0 19 * * *" => "Every day at 7:00 PM".to_string(),
        "0 22 * * *" => "Every day at 10:00 PM".to_string(),
        "0 7 * * 1-5" => "Weekdays at 7:00 AM".to_string(),
        "0 8 * * 1-5" => "Weekdays at 8:00 AM".to_string(),
        "0 19 * * 1-5" => "Weekdays at 7:00 PM".to_string(),
        "0 22 * * 1-5" => "Weekdays at 10:00 PM".to_string(),
        "0 3 * * 0" => "Sundays at 3:00 AM".to_string(),
        "0 3 * * 6" => "Saturdays at 3:00 AM".to_string(),
        "0 3 1 * *" => "1st of month at 3:00 AM".to_string(),
        "0 * * * *" => "Every hour".to_string(),
        "*/30 * * * *" => "Every 30 minutes".to_string(),
        _ => format!("Cron: {}", cron_expr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_to_cron() {
        assert_eq!(preset_to_cron("daily_7am"), Some("0 7 * * *"));
        assert_eq!(preset_to_cron("weekdays_7am"), Some("0 7 * * 1-5"));
        assert_eq!(preset_to_cron("invalid"), None);
    }

    #[test]
    fn test_cron_to_human() {
        assert_eq!(cron_to_human("0 7 * * *"), "Every day at 7:00 AM");
        assert_eq!(cron_to_human("0 7 * * 1-5"), "Weekdays at 7:00 AM");
        assert_eq!(cron_to_human("0 15 * * *"), "Cron: 0 15 * * *");
    }

    #[test]
    fn test_calculate_next_run() {
        // Test with a valid cron expression
        let next = calculate_next_run("0 7 * * *", "Europe/Paris");
        assert!(next.is_some());

        // Test with an invalid cron expression
        let next = calculate_next_run("invalid", "UTC");
        assert!(next.is_none());

        // Test with an invalid timezone (should fallback to UTC)
        let next = calculate_next_run("0 7 * * *", "Invalid/Timezone");
        assert!(next.is_some());
    }
}
