//! 6-phase DR switchover engine.

pub mod validate;
pub mod execute;
pub mod rollback;

use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbJson, DbUuid};
use crate::AppState;

#[derive(Debug, thiserror::Error)]
pub enum SwitchoverError {
    #[error("No active switchover for application")]
    NoActiveSwitchover,
    #[error("Invalid phase transition")]
    InvalidPhase,
    #[error("Database error: {0}")]
    Database(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Sequencer error: {0}")]
    Sequencer(String),
}

impl From<super::sequencer::SequencerError> for SwitchoverError {
    fn from(e: super::sequencer::SequencerError) -> Self {
        SwitchoverError::Sequencer(e.to_string())
    }
}

/// Start a new switchover process.
pub async fn start_switchover(
    pool: &crate::db::DbPool,
    app_id: Uuid,
    target_site_id: Uuid,
    mode: &str,
    component_ids: Option<Vec<Uuid>>,
    initiated_by: Uuid,
) -> Result<Uuid, SwitchoverError> {
    let switchover_id = Uuid::new_v4();
    let row_id = DbUuid::new_v4();
    let details_json = DbJson::from(serde_json::json!({
        "target_site_id": target_site_id, "mode": mode,
        "component_ids": component_ids, "initiated_by": initiated_by,
    }));
    sqlx::query(
        r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, 'PREPARE', 'in_progress', $4)"#,
    )
    .bind(row_id).bind(DbUuid::from(switchover_id)).bind(DbUuid::from(app_id)).bind(&details_json)
    .execute(pool).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(switchover_id)
}

/// Advance to the next phase with real orchestration.
pub async fn advance_phase(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();
    let pool = &state.db;

    let current = sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"SELECT switchover_id, phase, status FROM switchover_log
        WHERE application_id = $1 AND status = 'in_progress'
        ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(DbUuid::from(app_id)).fetch_optional(pool).await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, active_phase, _status) = current;

    let phase_result = match active_phase.as_str() {
        "PREPARE" => Ok(serde_json::json!({"status": "prepared"})),
        "VALIDATE" => validate::execute_validate(state, app_id, *switchover_id).await,
        "STOP_SOURCE" => execute::execute_stop_source(state, app_id, *switchover_id).await,
        "SYNC" => execute::execute_sync(state, app_id, *switchover_id).await,
        "START_TARGET" => execute::execute_start_target(state, app_id, *switchover_id).await,
        "COMMIT" => Ok(serde_json::json!({"finalized": true})),
        _ => return Err(SwitchoverError::InvalidPhase),
    };

    let next_phase = match active_phase.as_str() {
        "PREPARE" => Some("VALIDATE"),
        "VALIDATE" => Some("STOP_SOURCE"),
        "STOP_SOURCE" => Some("SYNC"),
        "SYNC" => Some("START_TARGET"),
        "START_TARGET" => Some("COMMIT"),
        "COMMIT" => None,
        _ => None,
    };

    match phase_result {
        Ok(details) => {
            sqlx::query(
                r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES ($1, $2, $3, $4, 'completed', $5)"#,
            )
            .bind(DbUuid::new_v4()).bind(crate::db::bind_id(switchover_id))
            .bind(DbUuid::from(app_id)).bind(&active_phase).bind(DbJson::from(details.clone()))
            .execute(pool).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

            if let Some(next) = next_phase {
                sqlx::query(
                    r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                    VALUES ($1, $2, $3, $4, 'in_progress', $5)"#,
                )
                .bind(DbUuid::new_v4()).bind(crate::db::bind_id(switchover_id))
                .bind(DbUuid::from(app_id)).bind(next).bind(DbJson::from(serde_json::json!({})))
                .execute(pool).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
            }

            let db = state.db.clone();
            let notification_phase = next_phase.map(|s| s.to_string()).unwrap_or_else(|| "DONE".to_string());
            let notification_status = if next_phase.is_some() { "in_progress" } else { "completed" };
            let event = super::notifications::NotificationEvent::Switchover {
                app_id, switchover_id: *switchover_id,
                phase: notification_phase, status: notification_status.to_string(),
            };
            tokio::spawn(async move { let _ = super::notifications::dispatch_event(&db, app_id, event).await; });

            Ok(serde_json::json!({
                "switchover_id": switchover_id, "completed_phase": active_phase,
                "next_phase": next_phase, "status": notification_status, "details": details,
            }))
        }
        Err(e) => {
            let error_details = serde_json::json!({"error": e.to_string()});
            let _ = sqlx::query(
                r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
                VALUES ($1, $2, $3, $4, 'failed', $5)"#,
            )
            .bind(DbUuid::new_v4()).bind(crate::db::bind_id(switchover_id))
            .bind(DbUuid::from(app_id)).bind(&active_phase).bind(DbJson::from(error_details))
            .execute(pool).await;
            Err(e)
        }
    }
}

pub use rollback::{rollback, commit, get_status};

/// Retrieve the details JSON from the PREPARE phase entry for a switchover.
pub(crate) async fn get_switchover_details(
    pool: &crate::db::DbPool,
    switchover_id: Uuid,
) -> Result<Value, SwitchoverError> {
    sqlx::query_scalar::<_, DbJson>(
        r#"SELECT details FROM switchover_log
        WHERE switchover_id = $1 AND phase = 'PREPARE'
        ORDER BY created_at ASC LIMIT 1"#,
    )
    .bind(DbUuid::from(switchover_id)).fetch_optional(pool).await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .map(|dj| dj.0)
    .ok_or(SwitchoverError::NoActiveSwitchover)
}
