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
    pool: &crate::db::DbPool,
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
    pool: &crate::db::DbPool,
    component_ids: &[Uuid],
) -> Result<Vec<(Uuid, ComponentState)>, FsmError> {
    let rows = fetch_component_states(pool, component_ids)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    rows.into_iter()
        .map(|(id, s)| parse_state(&s).map(|state| (id, state)))
        .collect()
}

#[cfg(feature = "postgres")]
async fn fetch_component_states(
    pool: &crate::db::DbPool,
    component_ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, current_state FROM components WHERE id = ANY($1)",
    )
    .bind(component_ids)
    .fetch_all(pool)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_component_states(
    pool: &crate::db::DbPool,
    component_ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=component_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        "SELECT id, current_state FROM components WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String)>(&query);
    for id in component_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String, String)> = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id_str, state)| Uuid::parse_str(&id_str).ok().map(|id| (id, state)))
        .collect())
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
    // PostgreSQL: SELECT ... FOR UPDATE prevents concurrent transitions.
    // SQLite: File-level locking via WAL mode handles concurrency.
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Read current state with row lock, including names for event broadcasting
    let row = fetch_component_for_transition(&mut tx, component_id)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    let (current_str, app_id, component_name, app_name) =
        row.ok_or(FsmError::ComponentNotFound(component_id))?;
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
    update_component_state(&mut tx, component_id, &new_state.to_string())
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    // Commit the transaction — both writes succeed or neither does
    tx.commit()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    metrics::counter!(
        "state_transitions_total",
        "from" => current.to_string(),
        "to" => new_state.to_string()
    )
    .increment(1);

    // Push WebSocket event (outside transaction — non-critical)
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

/// Force-transition a component to a new state, bypassing FSM validation.
///
/// Used for emergency operations (force kill) where the normal FSM rules
/// would block the transition. Still records the state change in the
/// append-only state_transitions table for full audit trail.
pub async fn force_transition_component(
    state: &Arc<AppState>,
    component_id: Uuid,
    new_state: ComponentState,
) -> Result<(), FsmError> {
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    let row = fetch_component_for_transition(&mut tx, component_id)
        .await
        .map_err(|e| FsmError::Database(e.to_string()))?;

    let (current_str, app_id, component_name, app_name) =
        row.ok_or(FsmError::ComponentNotFound(component_id))?;
    let current = parse_state(&current_str)?;

    // No FSM validation — force the transition

    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
        VALUES ($1, $2, $3, 'force')
        "#,
    )
    .bind(component_id)
    .bind(current.to_string())
    .bind(new_state.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| FsmError::Database(e.to_string()))?;

    update_component_state(&mut tx, component_id, &new_state.to_string())
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
/// Also stores the check event in check_events for audit trail.
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
/// This is separate from FSM processing to handle the full CheckResult data.
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

    sqlx::query(
        r#"INSERT INTO check_events (component_id, check_type, exit_code, stdout, duration_ms, metrics)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(check_result.component_id)
    .bind(check_type)
    .bind(check_result.exit_code as i16)
    .bind(&check_result.stdout)
    .bind(check_result.duration_ms as i32)
    .bind(&check_result.metrics)
    .execute(pool)
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

// ============================================================================
// Database-specific helper functions for FSM transactions
// ============================================================================

/// Fetch component data for state transition with row-level locking.
/// PostgreSQL uses FOR UPDATE, SQLite relies on WAL mode file-level locking.
#[cfg(feature = "postgres")]
async fn fetch_component_for_transition<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    component_id: Uuid,
) -> Result<Option<(String, Uuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, Uuid, String, String)>(
        r#"SELECT c.current_state, c.application_id, c.name, a.name
           FROM components c
           JOIN applications a ON c.application_id = a.id
           WHERE c.id = $1 FOR UPDATE OF c"#,
    )
    .bind(component_id)
    .fetch_optional(&mut **tx)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_component_for_transition<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    component_id: Uuid,
) -> Result<Option<(String, Uuid, String, String)>, sqlx::Error> {
    // SQLite: No FOR UPDATE needed - WAL mode provides serializable isolation
    #[derive(sqlx::FromRow)]
    struct Row {
        current_state: String,
        application_id: String,
        component_name: String,
        app_name: String,
    }
    let row = sqlx::query_as::<_, Row>(
        r#"SELECT c.current_state, c.application_id, c.name as component_name, a.name as app_name
           FROM components c
           JOIN applications a ON c.application_id = a.id
           WHERE c.id = $1"#,
    )
    .bind(component_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.map(|r| {
        let app_id = Uuid::parse_str(&r.application_id).unwrap_or(Uuid::nil());
        (r.current_state, app_id, r.component_name, r.app_name)
    }))
}

/// Update component state with database-appropriate timestamp function.
#[cfg(feature = "postgres")]
async fn update_component_state<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    component_id: Uuid,
    new_state: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE components SET current_state = $2, updated_at = now() WHERE id = $1")
        .bind(component_id)
        .bind(new_state)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn update_component_state<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    component_id: Uuid,
    new_state: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE components SET current_state = $2, updated_at = datetime('now') WHERE id = $1",
    )
    .bind(component_id.to_string())
    .bind(new_state)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
