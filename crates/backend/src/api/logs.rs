//! API endpoints for component log sources and log access.
//!
//! Provides CRUD for log sources (file paths, Windows Event Log, diagnostic commands)
//! and endpoints to retrieve logs from agents.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::middleware::audit;
use crate::repository::misc_queries;
use crate::repository::misc_queries::{LogComponentRow, LogSourceRow};
use crate::AppState;
use appcontrol_common::PermissionLevel;

use std::sync::Arc;

// ============================================================================
// Request/Response DTOs
// ============================================================================

#[derive(Debug, Serialize)]
pub struct LogSourceResponse {
    pub id: Uuid,
    pub component_id: Uuid,
    pub name: String,
    pub source_type: String,
    pub description: Option<String>,

    // File source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,

    // Event log source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_log_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_log_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_log_level: Option<String>,

    // Command source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_timeout_seconds: Option<i32>,

    // Settings
    pub max_lines: i32,
    pub max_age_hours: i32,
    pub is_sensitive: bool,
    pub display_order: i32,

    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLogSourceRequest {
    pub name: String,
    pub source_type: String,
    pub description: Option<String>,

    // File source
    pub file_path: Option<String>,

    // Event log source
    pub event_log_name: Option<String>,
    pub event_log_source: Option<String>,
    pub event_log_level: Option<String>,

    // Command source
    pub command: Option<String>,
    pub command_timeout_seconds: Option<i32>,

    // Settings
    pub max_lines: Option<i32>,
    pub max_age_hours: Option<i32>,
    pub is_sensitive: Option<bool>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLogSourceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub file_path: Option<String>,
    pub event_log_name: Option<String>,
    pub event_log_source: Option<String>,
    pub event_log_level: Option<String>,
    pub command: Option<String>,
    pub command_timeout_seconds: Option<i32>,
    pub max_lines: Option<i32>,
    pub max_age_hours: Option<i32>,
    pub is_sensitive: Option<bool>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct GetLogsQuery {
    pub source: Option<String>, // Source ID or "process" for stdout/stderr
    pub lines: Option<i32>,     // Number of lines (default 100)
    pub filter: Option<String>, // Text filter or log level (ERROR, WARN, INFO)
    pub since: Option<String>,  // Time range: "1h", "24h", "7d"
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: Option<DateTime<Utc>>,
    pub level: Option<String>,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub component_id: Uuid,
    pub component_name: String,
    pub source_type: String,
    pub source_name: String,
    pub entries: Vec<LogEntry>,
    pub total_lines: i32,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticCommandResponse {
    pub command_name: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: i64,
    pub executed_at: DateTime<Utc>,
}

// ============================================================================
// Log Sources CRUD
// ============================================================================

/// GET /api/v1/components/:component_id/log-sources
pub async fn list_log_sources(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Vec<LogSourceResponse>>, ApiError> {
    // Get component and check permission
    let component = get_component_with_permission(
        &state,
        &user,
        DbUuid::from(component_id),
        PermissionLevel::View,
    )
    .await?;

    let rows = misc_queries::list_log_sources(&state.db, component_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter sensitive sources if user doesn't have edit permission
    let can_see_sensitive = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await
        >= PermissionLevel::Edit;

    let responses: Vec<LogSourceResponse> = rows
        .into_iter()
        .filter(|r| !r.is_sensitive || can_see_sensitive)
        .map(row_to_response)
        .collect();

    Ok(Json(responses))
}

/// POST /api/v1/components/:component_id/log-sources
pub async fn create_log_source(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Json(req): Json<CreateLogSourceRequest>,
) -> Result<(StatusCode, Json<LogSourceResponse>), ApiError> {
    // Check edit permission
    let component = get_component_with_permission(
        &state,
        &user,
        DbUuid::from(component_id),
        PermissionLevel::Edit,
    )
    .await?;

    // Validate source type
    if !["file", "event_log", "command"].contains(&req.source_type.as_str()) {
        return Err(ApiError::Validation(format!(
            "Invalid source_type '{}'. Must be 'file', 'event_log', or 'command'",
            req.source_type
        )));
    }

    // Validate required fields based on type
    match req.source_type.as_str() {
        "file" if req.file_path.as_deref().unwrap_or("").is_empty() => {
            return Err(ApiError::Validation(
                "file_path is required for file source".into(),
            ));
        }
        "command" if req.command.as_deref().unwrap_or("").is_empty() => {
            return Err(ApiError::Validation(
                "command is required for command source".into(),
            ));
        }
        _ => {}
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    misc_queries::create_log_source(
        &state.db,
        id,
        component_id,
        component.organization_id,
        &req.name,
        &req.source_type,
        &req.description,
        &req.file_path,
        &req.event_log_name,
        &req.event_log_source,
        &req.event_log_level,
        &req.command,
        req.command_timeout_seconds.unwrap_or(30),
        req.max_lines.unwrap_or(1000),
        req.max_age_hours.unwrap_or(24),
        req.is_sensitive.unwrap_or(false),
        req.display_order.unwrap_or(0),
        user.user_id,
        now,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Audit log
    audit::log_action(
        &state.db,
        user.user_id,
        "create_log_source",
        "component",
        component_id,
        serde_json::json!({
            "log_source_id": id,
            "name": req.name,
            "source_type": req.source_type,
        }),
    )
    .await
    .ok();

    let response = LogSourceResponse {
        id,
        component_id,
        name: req.name,
        source_type: req.source_type,
        description: req.description,
        file_path: req.file_path,
        event_log_name: req.event_log_name,
        event_log_source: req.event_log_source,
        event_log_level: req.event_log_level,
        command: req.command,
        command_timeout_seconds: Some(req.command_timeout_seconds.unwrap_or(30)),
        max_lines: req.max_lines.unwrap_or(1000),
        max_age_hours: req.max_age_hours.unwrap_or(24),
        is_sensitive: req.is_sensitive.unwrap_or(false),
        display_order: req.display_order.unwrap_or(0),
        created_at: now,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// PUT /api/v1/log-sources/:id
pub async fn update_log_source(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(source_id): Path<Uuid>,
    Json(req): Json<UpdateLogSourceRequest>,
) -> Result<Json<LogSourceResponse>, ApiError> {
    // Get source and component
    let source = misc_queries::get_log_source_by_id(&state.db, source_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound)?;

    // Check edit permission on component
    get_component_with_permission(&state, &user, source.component_id, PermissionLevel::Edit)
        .await?;

    // Update fields
    let name = req.name.unwrap_or(source.name);
    let description = req.description.or(source.description);
    let file_path = req.file_path.or(source.file_path);
    let event_log_name = req.event_log_name.or(source.event_log_name);
    let event_log_source = req.event_log_source.or(source.event_log_source);
    let event_log_level = req.event_log_level.or(source.event_log_level);
    let command = req.command.or(source.command);
    let command_timeout_seconds = req
        .command_timeout_seconds
        .unwrap_or(source.command_timeout_seconds);
    let max_lines = req.max_lines.unwrap_or(source.max_lines);
    let max_age_hours = req.max_age_hours.unwrap_or(source.max_age_hours);
    let is_sensitive = req.is_sensitive.unwrap_or(source.is_sensitive);
    let display_order = req.display_order.unwrap_or(source.display_order);

    misc_queries::update_log_source(
        &state.db,
        source_id,
        &name,
        &description,
        &file_path,
        &event_log_name,
        &event_log_source,
        &event_log_level,
        &command,
        command_timeout_seconds,
        max_lines,
        max_age_hours,
        is_sensitive,
        display_order,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let response = LogSourceResponse {
        id: source_id,
        component_id: *source.component_id,
        name,
        source_type: source.source_type,
        description,
        file_path,
        event_log_name,
        event_log_source,
        event_log_level,
        command,
        command_timeout_seconds: Some(command_timeout_seconds),
        max_lines,
        max_age_hours,
        is_sensitive,
        display_order,
        created_at: source.created_at,
    };

    Ok(Json(response))
}

/// DELETE /api/v1/log-sources/:id
pub async fn delete_log_source(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(source_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Get source
    let source = misc_queries::get_log_source_by_id(&state.db, source_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound)?;

    // Check edit permission on component
    get_component_with_permission(&state, &user, source.component_id, PermissionLevel::Edit)
        .await?;

    misc_queries::delete_log_source(&state.db, source_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Audit log
    audit::log_action(
        &state.db,
        user.user_id,
        "delete_log_source",
        "component",
        source.component_id,
        serde_json::json!({
            "log_source_id": source_id,
            "name": source.name,
        }),
    )
    .await
    .ok();

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Log Retrieval
// ============================================================================

/// GET /api/v1/components/:component_id/logs
pub async fn get_component_logs(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Query(query): Query<GetLogsQuery>,
) -> Result<Json<LogsResponse>, ApiError> {
    // Check operate permission (minimum for log access)
    let component = get_component_with_permission(
        &state,
        &user,
        DbUuid::from(component_id),
        PermissionLevel::Operate,
    )
    .await?;

    let source_type: String;
    let source_name: String;

    // Determine which source to use
    let entries = match query.source.as_deref() {
        None | Some("process") => {
            source_type = "process".to_string();
            source_name = "Console output".to_string();
            get_process_logs_from_agent(&state, &component, &query).await?
        }
        Some(source_id_str) => {
            let source_id = Uuid::parse_str(source_id_str)
                .map_err(|_| ApiError::Validation("Invalid source ID".into()))?;

            let source = misc_queries::get_log_source_by_id_and_component(
                &state.db,
                source_id,
                component_id,
            )
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or(ApiError::NotFound)?;

            // Check sensitive access
            if source.is_sensitive {
                let perm = effective_permission(
                    &state.db,
                    user.user_id,
                    component.application_id,
                    user.is_admin(),
                )
                .await;
                if perm < PermissionLevel::Edit {
                    return Err(ApiError::Forbidden);
                }
            }

            source_type = source.source_type.clone();
            source_name = source.name.clone();

            match source.source_type.as_str() {
                "file" => get_file_logs_from_agent(&state, &component, &source, &query).await?,
                "event_log" => {
                    get_event_logs_from_agent(&state, &component, &source, &query).await?
                }
                "command" => {
                    return Err(ApiError::Validation(
                        "Use POST /components/:id/logs/command/:name for commands".into(),
                    ));
                }
                _ => return Err(ApiError::Internal("Unknown source type".into())),
            }
        }
    };

    // Log access for audit
    log_access_audit(
        &state,
        &user,
        DbUuid::from(component_id),
        query
            .source
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok().map(DbUuid::from)),
        &source_type,
        &source_name,
        &query,
    )
    .await
    .ok();

    let total_lines = entries.len() as i32;
    let max_lines = query.lines.unwrap_or(100);
    let truncated = total_lines > max_lines;

    Ok(Json(LogsResponse {
        component_id,
        component_name: component.name,
        source_type,
        source_name,
        entries: entries.into_iter().take(max_lines as usize).collect(),
        total_lines,
        truncated,
    }))
}

/// POST /api/v1/components/:component_id/logs/command/:command_name
pub async fn run_diagnostic_command(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, command_name)): Path<(Uuid, String)>,
) -> Result<Json<DiagnosticCommandResponse>, ApiError> {
    // Check operate permission
    let component = get_component_with_permission(
        &state,
        &user,
        DbUuid::from(component_id),
        PermissionLevel::Operate,
    )
    .await?;

    // Find the command source
    let source =
        misc_queries::get_log_source_by_component_type_name(&state.db, component_id, &command_name)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound)?;

    // Check sensitive access
    if source.is_sensitive {
        let perm = effective_permission(
            &state.db,
            user.user_id,
            component.application_id,
            user.is_admin(),
        )
        .await;
        if perm < PermissionLevel::Edit {
            return Err(ApiError::Forbidden);
        }
    }

    // Audit log before execution
    audit::log_action(
        &state.db,
        user.user_id,
        "run_diagnostic_command",
        "component",
        component_id,
        serde_json::json!({
            "command_name": command_name,
            "command": source.command,
        }),
    )
    .await
    .ok();

    // Execute via agent
    let result = execute_command_on_agent(
        &state,
        &component,
        source.command.as_deref().unwrap_or(""),
        source.command_timeout_seconds,
    )
    .await?;

    Ok(Json(result))
}

// ============================================================================
// Helper Functions
// ============================================================================

fn row_to_response(row: LogSourceRow) -> LogSourceResponse {
    LogSourceResponse {
        id: *row.id,
        component_id: *row.component_id,
        name: row.name,
        source_type: row.source_type,
        description: row.description,
        file_path: row.file_path,
        event_log_name: row.event_log_name,
        event_log_source: row.event_log_source,
        event_log_level: row.event_log_level,
        command: row.command,
        command_timeout_seconds: Some(row.command_timeout_seconds),
        max_lines: row.max_lines,
        max_age_hours: row.max_age_hours,
        is_sensitive: row.is_sensitive,
        display_order: row.display_order,
        created_at: row.created_at,
    }
}

async fn get_component_with_permission(
    state: &AppState,
    user: &AuthUser,
    component_id: DbUuid,
    required_level: PermissionLevel,
) -> Result<LogComponentRow, ApiError> {
    let component = misc_queries::get_component_for_logs(&state.db, component_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound)?;

    let permission = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;

    if permission < required_level {
        return Err(ApiError::Forbidden);
    }

    Ok(component)
}

async fn log_access_audit(
    state: &AppState,
    user: &AuthUser,
    component_id: DbUuid,
    log_source_id: Option<DbUuid>,
    source_type: &str,
    source_name: &str,
    query: &GetLogsQuery,
) -> Result<(), ApiError> {
    let org_id = misc_queries::get_component_org_id(&state.db, component_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    misc_queries::insert_log_access_audit(
        &state.db,
        Uuid::new_v4(),
        org_id,
        user.user_id,
        component_id,
        log_source_id,
        source_type,
        source_name,
        query.lines,
        &query.filter,
        parse_time_range_hours(&query.since),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(())
}

fn parse_time_range_hours(since: &Option<String>) -> Option<i32> {
    since.as_ref().and_then(|s| {
        if s.ends_with('h') {
            s.trim_end_matches('h').parse().ok()
        } else if s.ends_with('d') {
            s.trim_end_matches('d').parse::<i32>().ok().map(|d| d * 24)
        } else {
            None
        }
    })
}

// ============================================================================
// Agent Communication: dispatch GetProcessLogs / GetFileLogs / GetEventLogs to
// the component's agent, register a pending request keyed by request_id, await
// the matching ComponentLogs / FileLogs / EventLogs response. The agent side
// (`crates/agent/src/connection.rs`) already implements the responder.
// ============================================================================

/// Convert the protocol's [`ComponentLogEntry`] into the API's [`LogEntry`]
/// (same shape, kept separate so the two can evolve independently).
fn to_api_entry(e: appcontrol_common::protocol::ComponentLogEntry) -> LogEntry {
    LogEntry {
        timestamp: e.timestamp,
        level: e.level,
        content: e.content,
    }
}

/// Pull `entries` out of the JSON the WS handler put on `pending_log_requests`.
fn extract_entries_from_response(value: serde_json::Value) -> Result<Vec<LogEntry>, ApiError> {
    let entries: Vec<appcontrol_common::protocol::ComponentLogEntry> = serde_json::from_value(
        value
            .get("entries")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|e| ApiError::Internal(format!("Failed to parse log entries: {}", e)))?;
    Ok(entries.into_iter().map(to_api_entry).collect())
}

/// Round-trip a request to the agent and wait for the typed response.
///
/// Returns `[<info entry explaining why it's empty>]` when:
/// * the component has no agent yet (e.g. unenrolled site),
/// * the gateway/agent is offline,
/// * the agent took longer than `pending_log_requests.timeout()` (30s) to
///   answer.
///
/// We intentionally surface these as a single-line LogEntry instead of a 500
/// so the UI keeps rendering the Logs tab gracefully.
async fn dispatch_log_request(
    state: &AppState,
    agent_id: Option<DbUuid>,
    request_id: Uuid,
    msg: appcontrol_common::BackendMessage,
    no_agent_msg: &str,
) -> Result<Vec<LogEntry>, ApiError> {
    let Some(agent_id) = agent_id.map(|a| a.into_inner()) else {
        return Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("WARN".into()),
            content: no_agent_msg.to_string(),
        }]);
    };

    let rx = state.pending_log_requests.register(request_id);

    if !state.ws_hub.send_to_agent(agent_id, msg) {
        // Drop the pending request so it doesn't leak; an eventual late
        // arrival would just hit the cleanup path.
        state
            .pending_log_requests
            .complete(request_id, Err("agent offline".into()));
        return Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("WARN".into()),
            content: format!("Agent {agent_id} is not currently reachable via its gateway."),
        }]);
    }

    let timeout = state.pending_log_requests.timeout();
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(Ok(value))) => extract_entries_from_response(value),
        Ok(Ok(Err(err))) => Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("ERROR".into()),
            content: format!("Agent reported error: {err}"),
        }]),
        Ok(Err(_)) => Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("WARN".into()),
            content: "Agent response channel was dropped before completion.".into(),
        }]),
        Err(_) => Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("WARN".into()),
            content: format!(
                "Timed out after {}s waiting for the agent to return logs.",
                timeout.as_secs()
            ),
        }]),
    }
}

async fn get_process_logs_from_agent(
    state: &AppState,
    component: &LogComponentRow,
    query: &GetLogsQuery,
) -> Result<Vec<LogEntry>, ApiError> {
    let request_id = Uuid::new_v4();
    let msg = appcontrol_common::BackendMessage::GetProcessLogs {
        request_id,
        component_id: component.id.into_inner(),
        lines: query.lines.or(Some(100)),
        filter: query.filter.clone(),
        since: query.since.clone(),
    };
    dispatch_log_request(
        state,
        component.agent_id,
        request_id,
        msg,
        "No agent assigned to this component — process log capture needs an enrolled agent.",
    )
    .await
}

async fn get_file_logs_from_agent(
    state: &AppState,
    component: &LogComponentRow,
    source: &LogSourceRow,
    query: &GetLogsQuery,
) -> Result<Vec<LogEntry>, ApiError> {
    let Some(file_path) = source.file_path.clone() else {
        return Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("ERROR".into()),
            content: "Log source is of type 'file' but has no file_path configured.".into(),
        }]);
    };
    let request_id = Uuid::new_v4();
    let msg = appcontrol_common::BackendMessage::GetFileLogs {
        request_id,
        component_id: component.id.into_inner(),
        file_path,
        lines: query.lines.or(Some(100)),
        filter: query.filter.clone(),
        since: query.since.clone(),
    };
    dispatch_log_request(
        state,
        component.agent_id,
        request_id,
        msg,
        "No agent assigned to this component — file log retrieval needs an enrolled agent.",
    )
    .await
}

async fn get_event_logs_from_agent(
    state: &AppState,
    component: &LogComponentRow,
    source: &LogSourceRow,
    query: &GetLogsQuery,
) -> Result<Vec<LogEntry>, ApiError> {
    let Some(log_name) = source.event_log_name.clone() else {
        return Ok(vec![LogEntry {
            timestamp: Some(Utc::now()),
            level: Some("ERROR".into()),
            content: "Log source is of type 'event_log' but has no event_log_name configured."
                .into(),
        }]);
    };
    let request_id = Uuid::new_v4();
    let msg = appcontrol_common::BackendMessage::GetEventLogs {
        request_id,
        component_id: component.id.into_inner(),
        log_name,
        source: source.event_log_source.clone(),
        level: source.event_log_level.clone(),
        lines: query.lines.or(Some(100)),
        since: query.since.clone(),
    };
    dispatch_log_request(
        state,
        component.agent_id,
        request_id,
        msg,
        "No agent assigned to this component — Windows Event Log retrieval needs an enrolled agent.",
    )
    .await
}

async fn execute_command_on_agent(
    _state: &AppState,
    _component: &LogComponentRow,
    _command: &str,
    _timeout: i32,
) -> Result<DiagnosticCommandResponse, ApiError> {
    Ok(DiagnosticCommandResponse {
        command_name: "placeholder".into(),
        exit_code: 0,
        stdout: "[Diagnostic command execution not yet implemented]".into(),
        stderr: "".into(),
        duration_ms: 0,
        executed_at: Utc::now(),
    })
}
