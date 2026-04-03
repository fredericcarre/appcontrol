//! Snapshot schedules, snapshots, snapshot comparison, and file content reading.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::{DbUuid, UuidArray};
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::repository::discovery_queries as repo;
use crate::AppState;

// ============================================================================
// Snapshot Schedules
// ============================================================================

/// List snapshot schedules for the organization.
pub async fn list_schedules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = repo::list_snapshot_schedules(&state.db, *user.organization_id).await?;

    let schedules: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id, "name": row.name, "agent_ids": row.agent_ids.0,
                "frequency": row.frequency, "cron_expression": row.cron_expression,
                "enabled": row.enabled, "retention_days": row.retention_days,
                "last_run_at": row.last_run_at, "next_run_at": row.next_run_at,
                "created_at": row.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "schedules": schedules })))
}

/// Create a new snapshot schedule.
#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub agent_ids: Vec<Uuid>,
    pub frequency: String,
    #[serde(default = "default_retention")]
    pub retention_days: i32,
}

fn default_retention() -> i32 {
    30
}

pub async fn create_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateScheduleRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if body.agent_ids.is_empty() {
        return Err(ApiError::Validation("At least one agent_id is required".to_string()));
    }

    let valid_frequencies = ["hourly", "daily", "weekly", "monthly"];
    if !valid_frequencies.contains(&body.frequency.as_str()) {
        return Err(ApiError::Validation(format!(
            "Invalid frequency. Must be one of: {:?}",
            valid_frequencies
        )));
    }

    log_action(
        &state.db, user.user_id, "discovery_create_schedule", "snapshot_schedule", Uuid::nil(),
        json!({ "name": &body.name, "frequency": &body.frequency, "agents": body.agent_ids.len() }),
    ).await?;

    let schedule_id = Uuid::new_v4();
    let next_run = calculate_next_run(&body.frequency);

    repo::insert_snapshot_schedule(
        &state.db, schedule_id, *user.organization_id, &body.name,
        UuidArray::from(body.agent_ids.clone()), &body.frequency,
        body.retention_days, next_run, *user.user_id,
    ).await?;

    Ok(Json(json!({
        "id": schedule_id, "name": body.name, "agent_ids": body.agent_ids,
        "frequency": body.frequency, "retention_days": body.retention_days,
        "enabled": true, "next_run_at": next_run,
    })))
}

/// Update a snapshot schedule.
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct UpdateScheduleRequest {
    pub name: Option<String>,
    pub agent_ids: Option<Vec<Uuid>>,
    pub frequency: Option<String>,
    pub enabled: Option<bool>,
    pub retention_days: Option<i32>,
}

pub async fn update_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
    Json(body): Json<UpdateScheduleRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let exists = repo::schedule_exists(&state.db, schedule_id, *user.organization_id).await?;
    if !exists {
        return Err(ApiError::NotFound);
    }

    if body.name.is_none()
        && body.agent_ids.is_none()
        && body.frequency.is_none()
        && body.enabled.is_none()
        && body.retention_days.is_none()
    {
        return Err(ApiError::Validation("No fields to update".to_string()));
    }

    log_action(
        &state.db, user.user_id, "discovery_update_schedule", "snapshot_schedule",
        schedule_id, json!({ "updates": &body }),
    ).await?;

    if let Some(ref name) = body.name {
        repo::update_schedule_name(&state.db, schedule_id, name).await?;
    }
    if let Some(ref agent_ids) = body.agent_ids {
        repo::update_schedule_agent_ids(&state.db, schedule_id, UuidArray::from(agent_ids.clone())).await?;
    }
    if let Some(ref frequency) = body.frequency {
        let next_run = calculate_next_run(frequency);
        repo::update_schedule_frequency(&state.db, schedule_id, frequency, next_run).await?;
    }
    if let Some(enabled) = body.enabled {
        if enabled {
            let freq = repo::get_schedule_frequency(&state.db, schedule_id).await?;
            let next_run = calculate_next_run(&freq);
            repo::update_schedule_enabled(&state.db, schedule_id, true, Some(next_run)).await?;
        } else {
            repo::update_schedule_enabled(&state.db, schedule_id, false, None).await?;
        }
    }
    if let Some(retention_days) = body.retention_days {
        repo::update_schedule_retention(&state.db, schedule_id, retention_days).await?;
    }

    Ok(Json(json!({ "updated": true, "schedule_id": schedule_id })))
}

/// Delete a snapshot schedule.
pub async fn delete_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db, user.user_id, "discovery_delete_schedule", "snapshot_schedule",
        schedule_id, json!({}),
    ).await?;

    let rows = repo::delete_snapshot_schedule(&state.db, schedule_id, *user.organization_id).await?;
    if rows == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(Json(json!({ "deleted": true })))
}

// ============================================================================
// Snapshots
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListSnapshotsQuery {
    pub schedule_id: Option<DbUuid>,
}

/// List scheduled snapshots.
pub async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<ListSnapshotsQuery>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = repo::list_snapshots(&state.db, *user.organization_id, query.schedule_id).await?;

    let snapshots: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id, "schedule_id": row.schedule_id,
                "schedule_name": row.schedule_name, "agent_ids": row.agent_ids.0,
                "report_ids": row.report_ids.0, "captured_at": row.captured_at,
            })
        })
        .collect();

    Ok(Json(json!({ "snapshots": snapshots })))
}

/// Compare two snapshots and return differences.
#[derive(Debug, Deserialize)]
pub struct CompareSnapshotsRequest {
    pub snapshot_id_1: DbUuid,
    pub snapshot_id_2: DbUuid,
}

pub async fn compare_snapshots(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CompareSnapshotsRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let snap1 = repo::get_snapshot_correlation(&state.db, body.snapshot_id_1, *user.organization_id).await?;
    let snap2 = repo::get_snapshot_correlation(&state.db, body.snapshot_id_2, *user.organization_id).await?;

    let (corr1,) = snap1.ok_or(ApiError::NotFound)?;
    let (corr2,) = snap2.ok_or(ApiError::NotFound)?;

    let services1 = corr1.get("services").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    let services2 = corr2.get("services").and_then(|s| s.as_array()).cloned().unwrap_or_default();

    fn service_key(svc: &Value) -> String {
        let host = svc.get("hostname").and_then(|h| h.as_str()).unwrap_or("");
        let proc = svc.get("process_name").and_then(|p| p.as_str()).unwrap_or("");
        let ports: Vec<String> = svc
            .get("ports").and_then(|p| p.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n.to_string())).collect())
            .unwrap_or_default();
        format!("{}:{}:{}", host, proc, ports.join(","))
    }

    let keys1: std::collections::HashSet<String> = services1.iter().map(service_key).collect();
    let keys2: std::collections::HashSet<String> = services2.iter().map(service_key).collect();

    let added: Vec<Value> = services2.iter().filter(|s| !keys1.contains(&service_key(s))).cloned().collect();
    let removed: Vec<Value> = services1.iter().filter(|s| !keys2.contains(&service_key(s))).cloned().collect();

    let mut modified: Vec<Value> = Vec::new();
    for svc1 in &services1 {
        let key = service_key(svc1);
        if keys2.contains(&key) {
            if let Some(svc2) = services2.iter().find(|s| service_key(s) == key) {
                let ports1: Vec<u64> = svc1.get("ports").and_then(|p| p.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default();
                let ports2: Vec<u64> = svc2.get("ports").and_then(|p| p.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default();

                if ports1 != ports2 {
                    modified.push(json!({
                        "before": svc1, "after": svc2,
                        "changes": [format!("ports: {:?} -> {:?}", ports1, ports2)],
                    }));
                }
            }
        }
    }

    Ok(Json(json!({
        "added": added, "removed": removed, "modified": modified,
        "summary": { "added_count": added.len(), "removed_count": removed.len(), "modified_count": modified.len() }
    })))
}

// ============================================================================
// File Content Reading
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ReadFileContentRequest {
    pub agent_id: DbUuid,
    pub path: String,
    #[serde(default = "default_tail_lines")]
    pub tail_lines: Option<u32>,
}

fn default_tail_lines() -> Option<u32> {
    Some(100)
}

/// Read file content from an agent.
pub async fn read_file_content(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ReadFileContentRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let agent_exists = repo::agent_exists_in_org(&state.db, body.agent_id, *user.organization_id).await?;
    if !agent_exists {
        return Err(ApiError::NotFound);
    }

    let request_id = Uuid::new_v4();

    log_action(
        &state.db, user.user_id, "discovery_read_file", "agent", body.agent_id,
        json!({ "path": &body.path, "tail_lines": body.tail_lines }),
    ).await?;

    let command = if let Some(tail_lines) = body.tail_lines {
        format!(
            r#"powershell -Command "Get-Content -Path '{}' -Tail {} -ErrorAction Stop""#,
            body.path.replace('\'', "''"), tail_lines
        )
    } else {
        format!(
            r#"powershell -Command "Get-Content -Path '{}' -Raw -ErrorAction Stop""#,
            body.path.replace('\'', "''")
        )
    };

    let msg = appcontrol_common::BackendMessage::ExecuteCommand {
        request_id,
        component_id: *DbUuid::nil(),
        command: command.clone(),
        timeout_seconds: 30,
        exec_mode: "sync".to_string(),
    };

    let sent = state.ws_hub.send_to_agent(body.agent_id, msg);

    if !sent {
        return Err(ApiError::Conflict("Agent is not connected".to_string()));
    }

    Ok(Json(json!({
        "request_id": request_id, "agent_id": body.agent_id,
        "path": body.path, "command": command, "sent": true,
    })))
}

// ============================================================================
// Helper functions
// ============================================================================

fn calculate_next_run(frequency: &str) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, Duration, Timelike, Utc};

    let now = Utc::now();

    match frequency {
        "hourly" => now.with_minute(0).and_then(|t| t.with_second(0))
            .map(|t| t + Duration::hours(1)).unwrap_or(now + Duration::hours(1)),
        "daily" => now.with_hour(0).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0))
            .map(|t| t + Duration::days(1)).unwrap_or(now + Duration::days(1)),
        "weekly" => {
            let days_until_sunday = (7 - now.weekday().num_days_from_sunday()) % 7;
            let days_until_sunday = if days_until_sunday == 0 { 7 } else { days_until_sunday };
            now.with_hour(0).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0))
                .map(|t| t + Duration::days(days_until_sunday as i64))
                .unwrap_or(now + Duration::days(7))
        }
        "monthly" => {
            let next_month = if now.month() == 12 {
                now.with_year(now.year() + 1).and_then(|t| t.with_month(1))
            } else {
                now.with_month(now.month() + 1)
            };
            next_month.and_then(|t| t.with_day(1)).and_then(|t| t.with_hour(0))
                .and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0))
                .unwrap_or(now + Duration::days(30))
        }
        _ => now + Duration::days(1),
    }
}
