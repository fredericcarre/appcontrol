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
use crate::error::ApiError;
use crate::repository::misc_queries;
use crate::repository::misc_queries::{HistoryComponentRow, HistoryTransitionRow};
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
// Handler
// ---------------------------------------------------------------------------

/// GET /apps/:app_id/history
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

    let max_range = Duration::days(30);
    if to - from > max_range {
        return Err(ApiError::Validation(
            "Time range cannot exceed 30 days".to_string(),
        ));
    }

    let event_limit = params.event_limit.unwrap_or(500).min(1000);
    let resolution = params.resolution;

    // 3. Get all components for this app
    let components = misc_queries::history_list_components(&state.db, app_id).await?;

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

    let component_ids: Vec<Uuid> = components.iter().map(|c| c.id).collect();
    let component_names: HashMap<Uuid, String> =
        components.iter().map(|c| (c.id, c.name.clone())).collect();

    // 4. Get initial state at 'from' for each component
    let initial_states_vec = misc_queries::history_initial_states(&state.db, &component_ids, from).await?;
    let initial_states: HashMap<Uuid, String> = initial_states_vec.into_iter().collect();

    // 5. Get all state transitions in the time range
    let transitions =
        misc_queries::history_transition_rows(&state.db, &component_ids, from, to).await?;

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
        app_id,
        &component_ids,
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

/// Calculate snapshots at each resolution interval.
fn calculate_snapshots(
    components: &[HistoryComponentRow],
    initial_states: &HashMap<Uuid, String>,
    transitions: &[HistoryTransitionRow],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    resolution: Resolution,
) -> Vec<TimeSnapshot> {
    let interval = resolution.as_duration();
    let mut snapshots = Vec::new();
    let mut current_states: HashMap<Uuid, String> = initial_states.clone();

    let mut transition_idx = 0;

    let mut current_time = from;
    while current_time <= to {
        while transition_idx < transitions.len()
            && transitions[transition_idx].created_at <= current_time
        {
            let t = &transitions[transition_idx];
            current_states.insert(t.component_id, t.to_state.clone());
            transition_idx += 1;
        }

        let component_snapshots: Vec<ComponentSnapshot> = components
            .iter()
            .map(|c| ComponentSnapshot {
                id: c.id,
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

    if snapshots.len() > 200 {
        let step = snapshots.len() / 200;
        snapshots = snapshots.into_iter().step_by(step).collect();
    }

    snapshots
}

/// Get all events (state transitions, user actions, commands) in the time range.
async fn get_events(
    db: &crate::db::DbPool,
    app_id: Uuid,
    component_ids: &[Uuid],
    component_names: &HashMap<Uuid, String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<HistoryEvent>, ApiError> {
    let mut events = Vec::new();

    // State transitions
    let transitions =
        misc_queries::history_state_transitions(db, component_ids, from, to, limit).await?;

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
    let app_actions =
        misc_queries::history_app_actions(db, app_id, from, to, limit).await?;

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
    let component_actions =
        misc_queries::history_component_actions(db, component_ids, from, to, limit).await?;

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
    let commands =
        misc_queries::history_command_executions(db, component_ids, from, to, limit).await?;

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
            HistoryComponentRow {
                id: Uuid::new_v4(),
                name: "Database".to_string(),
            },
            HistoryComponentRow {
                id: Uuid::new_v4(),
                name: "Backend".to_string(),
            },
        ];

        let mut initial_states = HashMap::new();
        initial_states.insert(components[0].id, "STOPPED".to_string());
        initial_states.insert(components[1].id, "STOPPED".to_string());

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now();

        let transitions = vec![HistoryTransitionRow {
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

        assert!(snapshots.len() > 50);

        assert_eq!(snapshots[0].components[0].state, "STOPPED");
        assert_eq!(snapshots[0].components[1].state, "STOPPED");

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
