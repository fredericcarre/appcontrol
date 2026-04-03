//! Shared types for schedule endpoints.

use crate::db::DbUuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Request/Response DTOs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub operation: String,
    #[serde(default)]
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String { "Europe/Paris".to_string() }

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleResponse {
    pub id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub operation: String,
    pub cron_expression: String,
    pub cron_human: String,
    pub timezone: String,
    pub is_enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub next_run_relative: Option<String>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_status: Option<String>,
    pub last_run_message: Option<String>,
    pub target_type: String,
    pub target_id: DbUuid,
    pub target_name: String,
    pub created_by: Option<DbUuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleExecutionResponse {
    pub id: DbUuid,
    pub schedule_id: DbUuid,
    pub action_log_id: Option<DbUuid>,
    pub executed_at: DateTime<Utc>,
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct PresetInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    pub cron: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSchedulesQuery {
    #[serde(default)]
    pub include_disabled: Option<bool>,
}

// ============================================================================
// Database row types
// ============================================================================

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
pub struct ScheduleRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub application_id: Option<DbUuid>,
    pub component_id: Option<DbUuid>,
    pub name: String,
    pub description: Option<String>,
    pub operation: String,
    pub cron_expression: String,
    pub timezone: String,
    pub is_enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_status: Option<String>,
    pub last_run_message: Option<String>,
    pub created_by: Option<DbUuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct ExecutionRow {
    pub id: DbUuid,
    pub schedule_id: DbUuid,
    pub action_log_id: Option<DbUuid>,
    pub executed_at: DateTime<Utc>,
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: Option<i32>,
}

// ============================================================================
// Helper functions
// ============================================================================

use crate::core::operation_scheduler::cron_to_human;

pub fn relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = dt - now;
    let secs = diff.num_seconds();

    if secs < 0 { return "Overdue".to_string(); }
    if secs < 60 { return format!("In {} seconds", secs); }
    if secs < 3600 {
        let mins = secs / 60;
        return format!("In {} minute{}", mins, if mins == 1 { "" } else { "s" });
    }
    if secs < 86400 {
        let hours = secs / 3600;
        return format!("In {} hour{}", hours, if hours == 1 { "" } else { "s" });
    }
    let days = secs / 86400;
    format!("In {} day{}", days, if days == 1 { "" } else { "s" })
}

pub async fn get_target_info(
    db: &crate::db::DbPool,
    application_id: Option<DbUuid>,
    component_id: Option<DbUuid>,
) -> (String, DbUuid, String) {
    use crate::repository::schedule_queries as sched_repo;
    if let Some(app_id) = application_id {
        let name = sched_repo::get_app_name_by_id(db, *app_id).await;
        ("application".to_string(), app_id, name.unwrap_or_else(|| app_id.to_string()))
    } else if let Some(comp_id) = component_id {
        let name = sched_repo::get_comp_display_name(db, *comp_id).await;
        ("component".to_string(), comp_id, name.unwrap_or_else(|| comp_id.to_string()))
    } else {
        ("unknown".to_string(), DbUuid::nil(), "Unknown".to_string())
    }
}

pub fn row_to_response(row: ScheduleRow, target_type: String, target_id: DbUuid, target_name: String) -> ScheduleResponse {
    let next_run_relative = row.next_run_at.map(relative_time);
    let cron_human = cron_to_human(&row.cron_expression);

    ScheduleResponse {
        id: row.id, name: row.name, description: row.description, operation: row.operation,
        cron_expression: row.cron_expression, cron_human, timezone: row.timezone,
        is_enabled: row.is_enabled, next_run_at: row.next_run_at, next_run_relative,
        last_run_at: row.last_run_at, last_run_status: row.last_run_status,
        last_run_message: row.last_run_message, target_type, target_id, target_name,
        created_by: row.created_by, created_at: row.created_at, updated_at: row.updated_at,
    }
}

pub async fn get_app_id_for_component(db: &crate::db::DbPool, component_id: DbUuid) -> Option<DbUuid> {
    crate::repository::schedule_queries::get_component_app_id_sched(db, *component_id)
        .await.ok().flatten().map(DbUuid::from)
}
