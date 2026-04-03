use std::sync::Arc;
use uuid::Uuid;

use crate::db::DbUuid;
use crate::repository::core_queries;
use crate::AppState;
use appcontrol_common::{is_valid_transition, ComponentState};

#[derive(Debug, thiserror::Error)]
pub enum FsmError {
    #[error("Invalid transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("Component not found: {0}")]
    ComponentNotFound(Uuid),
    #[error("Database error: {0}")]
    Database(String),
}

/// Get the current state of a component from the cached `current_state` column.
pub async fn get_current_state(
    pool: &crate::db::DbPool,
    component_id: impl Into<Uuid>,
) -> Result<ComponentState, FsmError> {
    let component_id: Uuid = component_id.into();

    let state_str = core_queries::get_component_current_state(pool, component_id)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    match state_str {
        Some(s) => parse_state(&s),
        None => Err(FsmError::ComponentNotFound(component_id)),
    }
}

/// Get the current state for multiple components in a single query.
pub async fn get_current_states(
    pool: &crate::db::DbPool,
    component_ids: &[Uuid],
) -> Result<Vec<(Uuid, ComponentState)>, FsmError> {
    let rows = core_queries::get_component_states_bulk(pool, component_ids)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    rows.into_iter()
        .map(|(id, s)| parse_state(&s).map(|state| (id.into_inner(), state)))
        .collect()
}

/// Transition a component to a new state, validating the FSM rules.
pub async fn transition_component(
    state: &Arc<AppState>,
    component_id: impl Into<Uuid>,
    new_state: ComponentState,
) -> Result<(), FsmError> {
    let component_id: Uuid = component_id.into();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Read current state with row lock, including names for event broadcasting
    let row = core_queries::fetch_component_for_transition(&mut tx, component_id)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    let (current_str, raw_app_id, component_name, app_name) =
        row.ok_or(FsmError::ComponentNotFound(component_id))?;
    let app_id: Uuid = raw_app_id.into();
    let current = parse_state(&current_str)?;

    if !is_valid_transition(current, new_state) {
        return Err(FsmError::InvalidTransition {
            from: current.to_string(),
            to: new_state.to_string(),
        });
    }

    // Insert state transition (append-only audit trail)
    core_queries::insert_state_transition(
        &mut tx,
        component_id,
        &current.to_string(),
        &new_state.to_string(),
        "api",
    )
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    // Update cached current_state on the components row (fast read path)
    core_queries::update_component_state(&mut tx, component_id, &new_state.to_string())
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Commit the transaction
    tx.commit()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    metrics::counter!(
        "state_transitions_total",
        "from" => current.to_string(),
        "to" => new_state.to_string()
    )
    .increment(1);

    // Push WebSocket event (outside transaction)
    state.ws_hub.broadcast(
        app_id,
        appcontrol_common::WsEvent::StateChange {
            component_id,
            app_id,
            component_name: Some(component_name.clone()),
            app_name: Some(app_name.clone()),
            from: current,
            to: new_state,
            at: chrono::Utc::now(),
        },
    );

    // Fire notification asynchronously
    let db = state.db.clone();
    let event = crate::core::notifications::NotificationEvent::StateChange {
        component_id,
        app_id,
        from: current.to_string(),
        to: new_state.to_string(),
    };
    tokio::spawn(async move {
        if let Err(e) = crate::core::notifications::dispatch_event(&db, app_id, event).await {
            tracing::warn!("Notification dispatch failed: {}", e);
        }
    });

    Ok(())
}

/// Force-transition a component to a new state, bypassing FSM validation.
pub async fn force_transition_component(
    state: &Arc<AppState>,
    component_id: impl Into<Uuid>,
    new_state: ComponentState,
) -> Result<(), FsmError> {
    let component_id: Uuid = component_id.into();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    let row = core_queries::fetch_component_for_transition(&mut tx, component_id)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    let (current_str, raw_app_id, component_name, app_name) =
        row.ok_or(FsmError::ComponentNotFound(component_id))?;
    let app_id: Uuid = raw_app_id.into();
    let current = parse_state(&current_str)?;

    // No FSM validation — force the transition
    core_queries::insert_state_transition(
        &mut tx,
        component_id,
        &current.to_string(),
        &new_state.to_string(),
        "force",
    )
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    core_queries::update_component_state(&mut tx, component_id, &new_state.to_string())
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    metrics::counter!(
        "state_transitions_total",
        "from" => current.to_string(),
        "to" => new_state.to_string()
    )
    .increment(1);

    state.ws_hub.broadcast(
        app_id,
        appcontrol_common::WsEvent::StateChange {
            component_id,
            app_id,
            component_name: Some(component_name.clone()),
            app_name: Some(app_name.clone()),
            from: current,
            to: new_state,
            at: chrono::Utc::now(),
        },
    );

    let db = state.db.clone();
    let event = crate::core::notifications::NotificationEvent::StateChange {
        component_id,
        app_id,
        from: current.to_string(),
        to: new_state.to_string(),
    };
    tokio::spawn(async move {
        if let Err(e) = crate::core::notifications::dispatch_event(&db, app_id, event).await {
            tracing::warn!("Notification dispatch failed: {}", e);
        }
    });

    Ok(())
}

/// Process an incoming check result and update state if needed.
pub async fn process_check_result(
    state: &Arc<AppState>,
    component_id: Uuid,
    exit_code: i32,
) -> Result<Option<ComponentState>, FsmError> {
    let current = get_current_state(&state.db, component_id).await?;

    if let Some(new_state) = appcontrol_common::next_state_from_check(current, exit_code) {
        transition_component(state, component_id, new_state).await?;
        Ok(Some(new_state))
    } else {
        Ok(None)
    }
}

/// Store a check result in check_events with optional metrics.
pub async fn store_check_event(
    pool: &crate::db::DbPool,
    check_result: &appcontrol_common::CheckResult,
) -> Result<(), FsmError> {
    let check_type = match check_result.check_type {
        appcontrol_common::CheckType::Health => "health",
        appcontrol_common::CheckType::Integrity => "integrity",
        appcontrol_common::CheckType::PostStart => "post_start",
        appcontrol_common::CheckType::Infrastructure => "infrastructure",
    };

    core_queries::store_check_event(
        pool,
        check_result.component_id,
        check_type,
        check_result.exit_code as i16,
        &check_result.stdout,
        check_result.duration_ms as i32,
        &check_result.metrics,
    )
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    Ok(())
}

fn parse_state(s: &str) -> Result<ComponentState, FsmError> {
    match s {
        "UNKNOWN" => Ok(ComponentState::Unknown),
        "RUNNING" => Ok(ComponentState::Running),
        "DEGRADED" => Ok(ComponentState::Degraded),
        "FAILED" => Ok(ComponentState::Failed),
        "STOPPED" => Ok(ComponentState::Stopped),
        "STARTING" => Ok(ComponentState::Starting),
        "STOPPING" => Ok(ComponentState::Stopping),
        "UNREACHABLE" => Ok(ComponentState::Unreachable),
        _ => Err(FsmError::InvalidTransition {
            from: s.to_string(),
            to: "unknown".to_string(),
        }),
    }
}
