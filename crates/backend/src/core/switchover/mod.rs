//! 6-phase DR switchover engine.

pub mod validate;
pub mod execute;
pub mod rollback;

use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbJson, DbUuid};
use crate::repository::switchover_queries as repo;
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
    let details_json = serde_json::json!({
        "target_site_id": target_site_id, "mode": mode,
        "component_ids": component_ids, "initiated_by": initiated_by,
    });
    repo::insert_switchover_log(pool, switchover_id, app_id, "PREPARE", "in_progress", details_json)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(switchover_id)
}

/// Advance to the next phase with real orchestration.
pub async fn advance_phase(
    state: &Arc<AppState>,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();
    let pool = &state.db;

    let current = repo::get_active_switchover(pool, app_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?
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
            repo::insert_switchover_log(pool, *switchover_id, app_id, &active_phase, "completed", details.clone())
                .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

            if let Some(next) = next_phase {
                repo::insert_switchover_log(pool, *switchover_id, app_id, next, "in_progress", serde_json::json!({}))
                    .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;
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
            let _ = repo::insert_switchover_log(pool, *switchover_id, app_id, &active_phase, "failed", error_details).await;
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
    repo::get_switchover_details_from_prepare(pool, switchover_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?
        .map(|dj| dj.0)
        .ok_or(SwitchoverError::NoActiveSwitchover)
}
