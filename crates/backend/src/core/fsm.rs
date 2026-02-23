use std::sync::Arc;
use uuid::Uuid;

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
/// This is O(1) — no scan on the append-only state_transitions table.
pub async fn get_current_state(
    pool: &sqlx::PgPool,
    component_id: Uuid,
) -> Result<ComponentState, FsmError> {
    let state_str =
        sqlx::query_scalar::<_, String>("SELECT current_state FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| FsmError::Database(e.to_string()))?;

    match state_str {
        Some(s) => parse_state(&s),
        None => Err(FsmError::ComponentNotFound(component_id)),
    }
}

/// Get the current state for multiple components in a single query.
pub async fn get_current_states(
    pool: &sqlx::PgPool,
    component_ids: &[Uuid],
) -> Result<Vec<(Uuid, ComponentState)>, FsmError> {
    let rows = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, current_state FROM components WHERE id = ANY($1)",
    )
    .bind(component_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    rows.into_iter()
        .map(|(id, s)| parse_state(&s).map(|state| (id, state)))
        .collect()
}

/// Transition a component to a new state, validating the FSM rules.
/// Uses a database transaction to atomically:
/// 1. Read + validate current state with SELECT ... FOR UPDATE (prevents races)
/// 2. Insert into state_transitions (append-only audit trail)
/// 3. Update cached current_state on the components row
pub async fn transition_component(
    state: &Arc<AppState>,
    component_id: Uuid,
    new_state: ComponentState,
) -> Result<(), FsmError> {
    // Run the state read + validate + write atomically in a transaction.
    // SELECT ... FOR UPDATE prevents concurrent transitions on the same component.
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Read current state with row lock
    let row = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT current_state, application_id FROM components WHERE id = $1 FOR UPDATE",
    )
    .bind(component_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    let (current_str, app_id) = row.ok_or(FsmError::ComponentNotFound(component_id))?;
    let current = parse_state(&current_str)?;

    if !is_valid_transition(current, new_state) {
        // Transaction rolls back on drop
        return Err(FsmError::InvalidTransition {
            from: current.to_string(),
            to: new_state.to_string(),
        });
    }

    // Insert state transition (append-only audit trail)
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
        VALUES ($1, $2, $3, 'api')
        "#,
    )
    .bind(component_id)
    .bind(current.to_string())
    .bind(new_state.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    // Update cached current_state on the components row (fast read path)
    sqlx::query("UPDATE components SET current_state = $2, updated_at = now() WHERE id = $1")
        .bind(component_id)
        .bind(new_state.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Commit the transaction — both writes succeed or neither does
    tx.commit()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Push WebSocket event (outside transaction — non-critical)
    state.ws_hub.broadcast(
        app_id,
        appcontrol_common::WsEvent::StateChange {
            component_id,
            app_id,
            from: current,
            to: new_state,
            at: chrono::Utc::now(),
        },
    );

    // Fire notification asynchronously (webhook/Slack)
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
