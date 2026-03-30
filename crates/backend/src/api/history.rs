//! Historical timeline API for application state replay.
//!
//! Provides snapshots of component states at any point in time, plus the events
//! (state transitions, user actions, commands) that occurred during a time range.
//! Used by the frontend "Time Machine" feature to replay application history.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Start of the time range (inclusive)
    pub from: DateTime<Utc>,
    /// End of the time range (inclusive)
    pub to: DateTime<Utc>,
    /// Sampling resolution for snapshots
    #[serde(default = "default_resolution")]
    pub resolution: Resolution,
    /// Maximum number of events to return (default 500, max 1000)
    pub event_limit: Option<i64>,
}

fn default_resolution() -> Resolution {
    Resolution::Minute
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    Minute,
    FiveMinutes,
    Hour,
    Day,
}

impl Resolution {
    fn as_duration(&self) -> Duration {
        match self {
            Resolution::Minute => Duration::minutes(1),
            Resolution::FiveMinutes => Duration::minutes(5),
            Resolution::Hour => Duration::hours(1),
            Resolution::Day => Duration::days(1),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ComponentSnapshot {
    pub id: Uuid,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct TimeSnapshot {
    pub at: DateTime<Utc>,
    pub components: Vec<ComponentSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct HistoryEvent {
    pub at: DateTime<Utc>,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub data: Value,
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub snapshots: Vec<TimeSnapshot>,
    pub events: Vec<HistoryEvent>,
    pub time_range: TimeRange,
}

#[derive(Debug, Serialize)]
pub struct TimeRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub resolution: Resolution,
}

// ---------------------------------------------------------------------------
// Internal row types
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct ComponentRow {
    id: DbUuid,
    name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct StateTransitionRow {
    component_id: DbUuid,
    #[allow(dead_code)]
    from_state: String,
    to_state: String,
    #[allow(dead_code)]
    trigger: String,
    created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /apps/:app_id/history
///
/// Returns historical snapshots and events for an application.
///
/// Query parameters:
/// - `from`: ISO 8601 datetime (required)
/// - `to`: ISO 8601 datetime (required)
/// - `resolution`: "minute", "fiveminutes", "hour", or "day" (default: "minute")
/// - `event_limit`: Maximum events to return (default: 500, max: 1000)
///
/// Response:
/// ```json
/// {
///   "snapshots": [
///     {
///       "at": "2026-03-19T10:00:00Z",
///       "components": [
///         { "id": "uuid", "name": "Database", "state": "RUNNING" }
///       ]
///     }
///   ],
///   "events": [
///     {
///       "at": "2026-03-19T10:15:00Z",
///       "type": "state_change",
///       "component_id": "uuid",
///       "component_name": "Database",
///       "from_state": "STOPPED",
///       "to_state": "STARTING"
///     }
///   ],
///   "time_range": { "from": "...", "to": "...", "resolution": "minute" }
/// }
/// ```
pub async fn app_history(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, ApiError> {
    // 1. Check permissions
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // 2. Validate time range
    let from = params.from;
    let to = params.to;
    if from >= to {
        return Err(ApiError::Validation(
            "Invalid time range: 'from' must be before 'to'".to_string(),
        ));
    }

    // Limit range to 30 days max
    let max_range = Duration::days(30);
    if to - from > max_range {
        return Err(ApiError::Validation(
            "Time range cannot exceed 30 days".to_string(),
        ));
    }

    let event_limit = params.event_limit.unwrap_or(500).min(1000);
    let resolution = params.resolution;

    // 3. Get all components for this app
    #[cfg(feature = "postgres")]
    let components = sqlx::query_as::<_, ComponentRow>(
        "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let components = sqlx::query_as::<_, ComponentRow>(
        "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(&state.db)
    .await?;

    if components.is_empty() {
        return Ok(Json(HistoryResponse {
            snapshots: vec![],
            events: vec![],
            time_range: TimeRange {
                from,
                to,
                resolution,
            },
        }));
    }

    let component_ids: Vec<DbUuid> = components.iter().map(|c| c.id).collect();
    let component_names: HashMap<DbUuid, String> =
        components.iter().map(|c| (c.id, c.name.clone())).collect();

    // 4. Get initial state at 'from' for each component
    // We need to find the most recent state_transition before 'from' for each component
    let component_ids_uuid: Vec<Uuid> = component_ids.iter().map(|id| **id).collect();
    let initial_states = get_initial_states(&state.db, &component_ids_uuid, from).await?;

    // 5. Get all state transitions in the time range
    let transitions = fetch_transition_rows(&state.db, &component_ids_uuid, from, to).await?;

    // 6. Calculate snapshots at each resolution interval
    let snapshots = calculate_snapshots(
        &components,
        &initial_states,
        &transitions,
        from,
        to,
        resolution,
    );

    // 7. Get all events (transitions + actions + commands)
    let events = get_events(
        &state.db,
        DbUuid::from(app_id),
        &component_ids_uuid,
        &component_names,
        from,
        to,
        event_limit,
    )
    .await?;

    Ok(Json(HistoryResponse {
        snapshots,
        events,
        time_range: TimeRange {
            from,
            to,
            resolution,
        },
    }))
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Get the initial state of each component at a given time.
/// This finds the most recent state_transition before the given time.
async fn get_initial_states(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    at: DateTime<Utc>,
) -> Result<HashMap<DbUuid, String>, ApiError> {
    let rows = fetch_initial_states(db, component_ids, at).await?;
    Ok(rows.into_iter().collect())
}

#[cfg(feature = "postgres")]
async fn fetch_initial_states(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    at: DateTime<Utc>,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        r#"
        SELECT c.id, COALESCE(
            (SELECT st.to_state
             FROM state_transitions st
             WHERE st.component_id = c.id AND st.created_at < $2
             ORDER BY st.created_at DESC
             LIMIT 1),
            c.current_state
        ) as state
        FROM components c
        WHERE c.id = ANY($1)
        "#,
    )
    .bind(component_ids)
    .bind(at)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_initial_states(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    at: DateTime<Utc>,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    // SQLite: use IN clause with placeholders
    // $1 is for 'at', component_ids start at $2
    let placeholders: Vec<String> = (2..=1 + component_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"
        SELECT c.id, COALESCE(
            (SELECT st.to_state
             FROM state_transitions st
             WHERE st.component_id = c.id AND st.created_at < $1
             ORDER BY st.created_at DESC
             LIMIT 1),
            c.current_state
        ) as state
        FROM components c
        WHERE c.id IN ({})
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String)>(&query).bind(at.to_rfc3339());
    for id in component_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String, String)> = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id_str, state)| {
            Uuid::parse_str(&id_str)
                .ok()
                .map(|id| (DbUuid::from(id), state))
        })
        .collect())
}

/// Calculate snapshots at each resolution interval.
fn calculate_snapshots(
    components: &[ComponentRow],
    initial_states: &HashMap<DbUuid, String>,
    transitions: &[StateTransitionRow],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    resolution: Resolution,
) -> Vec<TimeSnapshot> {
    let interval = resolution.as_duration();
    let mut snapshots = Vec::new();
    let mut current_states: HashMap<DbUuid, String> = initial_states.clone();

    // Index transitions by time for efficient lookup
    let mut transition_idx = 0;

    let mut current_time = from;
    while current_time <= to {
        // Apply all transitions up to current_time
        while transition_idx < transitions.len()
            && transitions[transition_idx].created_at <= current_time
        {
            let t = &transitions[transition_idx];
            current_states.insert(t.component_id, t.to_state.clone());
            transition_idx += 1;
        }

        // Create snapshot
        let component_snapshots: Vec<ComponentSnapshot> = components
            .iter()
            .map(|c| ComponentSnapshot {
                id: *c.id,
                name: c.name.clone(),
                state: current_states
                    .get(&c.id)
                    .cloned()
                    .unwrap_or_else(|| "UNKNOWN".to_string()),
            })
            .collect();

        snapshots.push(TimeSnapshot {
            at: current_time,
            components: component_snapshots,
        });

        current_time += interval;
    }

    // Limit snapshots to prevent huge responses
    // Keep at most 200 snapshots, evenly distributed
    if snapshots.len() > 200 {
        let step = snapshots.len() / 200;
        snapshots = snapshots.into_iter().step_by(step).collect();
    }

    snapshots
}

/// Get all events (state transitions, user actions, commands) in the time range.
async fn get_events(
    db: &crate::db::DbPool,
    app_id: DbUuid,
    component_ids: &[Uuid],
    component_names: &HashMap<DbUuid, String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<HistoryEvent>, ApiError> {
    let mut events = Vec::new();

    // State transitions
    let transitions = fetch_state_transitions(db, component_ids, from, to, limit).await?;

    for (comp_id, from_state, to_state, trigger, at) in transitions {
        events.push(HistoryEvent {
            at,
            event_type: "state_change".to_string(),
            data: json!({
                "component_id": comp_id,
                "component_name": component_names.get(&comp_id).cloned().unwrap_or_default(),
                "from_state": from_state,
                "to_state": to_state,
                "trigger": trigger,
            }),
        });
    }

    // User actions on the app
    let app_actions = sqlx::query_as::<
        _,
        (
            String,
            String,
            Value,
            DateTime<Utc>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action, al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        WHERE al.resource_id = $1 AND al.created_at >= $2 AND al.created_at <= $3
        ORDER BY al.created_at ASC
        LIMIT $4
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(db)
    .await?;

    for (user, action, details, at, status, error_message) in app_actions {
        events.push(HistoryEvent {
            at,
            event_type: "action".to_string(),
            data: json!({
                "user": user,
                "action": action,
                "details": details,
                "status": status,
                "error_message": error_message,
            }),
        });
    }

    // User actions on components
    let component_actions = fetch_component_actions(db, component_ids, from, to, limit).await?;

    for (user, action, comp_id, comp_name, details, at, status, error_message) in component_actions
    {
        events.push(HistoryEvent {
            at,
            event_type: "action".to_string(),
            data: json!({
                "user": user,
                "action": action,
                "component_id": comp_id,
                "component_name": comp_name,
                "details": details,
                "status": status,
                "error_message": error_message,
            }),
        });
    }

    // Command executions
    let commands = fetch_command_executions(db, component_ids, from, to, limit).await?;

    for (request_id, comp_id, cmd_type, exit_code, duration_ms, dispatched_at, completed_at) in
        commands
    {
        events.push(HistoryEvent {
            at: completed_at.unwrap_or(dispatched_at),
            event_type: "command".to_string(),
            data: json!({
                "request_id": request_id,
                "component_id": comp_id,
                "component_name": component_names.get(&comp_id).cloned().unwrap_or_default(),
                "command_type": cmd_type,
                "exit_code": exit_code,
                "duration_ms": duration_ms,
                "dispatched_at": dispatched_at,
                "completed_at": completed_at,
            }),
        });
    }

    // Sort all events by timestamp
    events.sort_by(|a, b| a.at.cmp(&b.at));

    // Truncate to limit
    events.truncate(limit as usize);

    Ok(events)
}

// ============================================================================
// Database-specific helper functions
// ============================================================================

#[cfg(feature = "postgres")]
async fn fetch_transition_rows(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<StateTransitionRow>, sqlx::Error> {
    sqlx::query_as::<_, StateTransitionRow>(
        r#"
        SELECT component_id, from_state, to_state, trigger, created_at
        FROM state_transitions
        WHERE component_id = ANY($1) AND created_at >= $2 AND created_at <= $3
        ORDER BY created_at ASC
        "#,
    )
    .bind(component_ids)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_transition_rows(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<StateTransitionRow>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (3..=2 + component_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"
        SELECT component_id, from_state, to_state, trigger, created_at
        FROM state_transitions
        WHERE component_id IN ({}) AND created_at >= $1 AND created_at <= $2
        ORDER BY created_at ASC
        "#,
        placeholders.join(", ")
    );
    // SQLite returns TEXT for UUID and timestamp - use a custom row type
    #[derive(sqlx::FromRow)]
    struct SqliteRow {
        component_id: String,
        from_state: String,
        to_state: String,
        trigger: String,
        created_at: String,
    }
    let mut q = sqlx::query_as::<_, SqliteRow>(&query)
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339());
    for id in component_ids {
        q = q.bind(id.to_string());
    }
    let rows = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let id = Uuid::parse_str(&r.component_id).ok()?;
            let at = chrono::DateTime::parse_from_rfc3339(&r.created_at)
                .ok()?
                .with_timezone(&Utc);
            Some(StateTransitionRow {
                component_id: DbUuid::from(id),
                from_state: r.from_state,
                to_state: r.to_state,
                trigger: r.trigger,
                created_at: at,
            })
        })
        .collect())
}

#[cfg(feature = "postgres")]
async fn fetch_state_transitions(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<(DbUuid, String, String, String, DateTime<Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, String, DateTime<Utc>)>(
        r#"
        SELECT component_id, from_state, to_state, trigger, created_at
        FROM state_transitions
        WHERE component_id = ANY($1) AND created_at >= $2 AND created_at <= $3
        ORDER BY created_at ASC
        LIMIT $4
        "#,
    )
    .bind(component_ids)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_state_transitions(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<(DbUuid, String, String, String, DateTime<Utc>)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (4..=3 + component_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"
        SELECT component_id, from_state, to_state, trigger, created_at
        FROM state_transitions
        WHERE component_id IN ({}) AND created_at >= $1 AND created_at <= $2
        ORDER BY created_at ASC
        LIMIT $3
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String, String, String, String)>(&query)
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339())
        .bind(limit);
    for id in component_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String, String, String, String, String)> = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(comp_id, from_state, to_state, trigger, created_at)| {
            let id = Uuid::parse_str(&comp_id).ok()?;
            let at = chrono::DateTime::parse_from_rfc3339(&created_at)
                .ok()?
                .with_timezone(&Utc);
            Some((DbUuid::from(id), from_state, to_state, trigger, at))
        })
        .collect())
}

#[cfg(feature = "postgres")]
async fn fetch_component_actions(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<
    Vec<(
        String,
        String,
        Uuid,
        String,
        Value,
        DateTime<Utc>,
        Option<String>,
        Option<String>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            String,
            String,
            Uuid,
            String,
            Value,
            DateTime<Utc>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT COALESCE(u.email, al.user_id::text), al.action, al.resource_id,
               COALESCE(c.name, al.resource_id::text), al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        LEFT JOIN components c ON c.id = al.resource_id
        WHERE al.resource_type = 'component'
          AND al.resource_id = ANY($1)
          AND al.created_at >= $2 AND al.created_at <= $3
        ORDER BY al.created_at ASC
        LIMIT $4
        "#,
    )
    .bind(component_ids)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_component_actions(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<
    Vec<(
        String,
        String,
        Uuid,
        String,
        Value,
        DateTime<Utc>,
        Option<String>,
        Option<String>,
    )>,
    sqlx::Error,
> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (4..=3 + component_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"
        SELECT COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action, al.resource_id,
               COALESCE(c.name, CAST(al.resource_id AS TEXT)), al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        LEFT JOIN components c ON c.id = al.resource_id
        WHERE al.resource_type = 'component'
          AND al.resource_id IN ({})
          AND al.created_at >= $1 AND al.created_at <= $2
        ORDER BY al.created_at ASC
        LIMIT $3
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        ),
    >(&query)
    .bind(from.to_rfc3339())
    .bind(to.to_rfc3339())
    .bind(limit);
    for id in component_ids {
        q = q.bind(id.to_string());
    }
    let rows = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(
            |(user, action, resource_id, comp_name, details, created_at, status, error)| {
                let id = Uuid::parse_str(&resource_id).ok()?;
                let at = chrono::DateTime::parse_from_rfc3339(&created_at)
                    .ok()?
                    .with_timezone(&Utc);
                let details_val: Value = serde_json::from_str(&details).unwrap_or(Value::Null);
                Some((user, action, id, comp_name, details_val, at, status, error))
            },
        )
        .collect())
}

#[cfg(feature = "postgres")]
async fn fetch_command_executions(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<
    Vec<(
        Uuid,
        Uuid,
        String,
        Option<i16>,
        Option<i32>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            Option<i16>,
            Option<i32>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
        ),
    >(
        r#"
        SELECT ce.request_id, ce.component_id, ce.command_type,
               ce.exit_code, ce.duration_ms, ce.dispatched_at, ce.completed_at
        FROM command_executions ce
        WHERE ce.component_id = ANY($1) AND ce.dispatched_at >= $2 AND ce.dispatched_at <= $3
        ORDER BY ce.dispatched_at ASC
        LIMIT $4
        "#,
    )
    .bind(component_ids)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_command_executions(
    db: &crate::db::DbPool,
    component_ids: &[Uuid],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<
    Vec<(
        Uuid,
        Uuid,
        String,
        Option<i16>,
        Option<i32>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
    )>,
    sqlx::Error,
> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (4..=3 + component_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"
        SELECT ce.request_id, ce.component_id, ce.command_type,
               ce.exit_code, ce.duration_ms, ce.dispatched_at, ce.completed_at
        FROM command_executions ce
        WHERE ce.component_id IN ({}) AND ce.dispatched_at >= $1 AND ce.dispatched_at <= $2
        ORDER BY ce.dispatched_at ASC
        LIMIT $3
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<i16>,
            Option<i32>,
            String,
            Option<String>,
        ),
    >(&query)
    .bind(from.to_rfc3339())
    .bind(to.to_rfc3339())
    .bind(limit);
    for id in component_ids {
        q = q.bind(id.to_string());
    }
    let rows = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(
            |(
                request_id,
                comp_id,
                cmd_type,
                exit_code,
                duration_ms,
                dispatched_at,
                completed_at,
            )| {
                let req_id = Uuid::parse_str(&request_id).ok()?;
                let cid = Uuid::parse_str(&comp_id).ok()?;
                let dispatched = chrono::DateTime::parse_from_rfc3339(&dispatched_at)
                    .ok()?
                    .with_timezone(&Utc);
                let completed = completed_at
                    .and_then(|c| chrono::DateTime::parse_from_rfc3339(&c).ok())
                    .map(|c| c.with_timezone(&Utc));
                Some((
                    req_id,
                    cid,
                    cmd_type,
                    exit_code,
                    duration_ms,
                    dispatched,
                    completed,
                ))
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_duration() {
        assert_eq!(Resolution::Minute.as_duration(), Duration::minutes(1));
        assert_eq!(Resolution::FiveMinutes.as_duration(), Duration::minutes(5));
        assert_eq!(Resolution::Hour.as_duration(), Duration::hours(1));
        assert_eq!(Resolution::Day.as_duration(), Duration::days(1));
    }

    #[test]
    fn test_calculate_snapshots_basic() {
        let components = vec![
            ComponentRow {
                id: DbUuid::new_v4(),
                name: "Database".to_string(),
            },
            ComponentRow {
                id: DbUuid::new_v4(),
                name: "Backend".to_string(),
            },
        ];

        let mut initial_states = HashMap::new();
        initial_states.insert(components[0].id, "STOPPED".to_string());
        initial_states.insert(components[1].id, "STOPPED".to_string());

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now();

        let transitions = vec![StateTransitionRow {
            component_id: components[0].id,
            from_state: "STOPPED".to_string(),
            to_state: "RUNNING".to_string(),
            trigger: "start_cmd".to_string(),
            created_at: from + Duration::minutes(30),
        }];

        let snapshots = calculate_snapshots(
            &components,
            &initial_states,
            &transitions,
            from,
            to,
            Resolution::Minute,
        );

        // Should have ~60 snapshots (1 per minute for 1 hour)
        assert!(snapshots.len() > 50);

        // First snapshot should have both STOPPED
        assert_eq!(snapshots[0].components[0].state, "STOPPED");
        assert_eq!(snapshots[0].components[1].state, "STOPPED");

        // Last snapshot should have Database RUNNING, Backend STOPPED
        let last = snapshots.last().unwrap();
        let db_state = last
            .components
            .iter()
            .find(|c| c.name == "Database")
            .unwrap();
        let be_state = last
            .components
            .iter()
            .find(|c| c.name == "Backend")
            .unwrap();
        assert_eq!(db_state.state, "RUNNING");
        assert_eq!(be_state.state, "STOPPED");
    }
}
