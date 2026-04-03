//! Switchover rollback, commit, and status.

use serde_json::Value;
use uuid::Uuid;

use crate::repository::switchover_queries as repo;

use super::SwitchoverError;

/// Rollback the switchover.
pub async fn rollback(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let current = repo::get_latest_switchover(pool, app_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?
        .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;

    repo::insert_switchover_log(pool, *switchover_id, app_id, "ROLLBACK", "completed",
        serde_json::json!({"rolled_back_from": phase}),
    ).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({"switchover_id": switchover_id, "status": "rolled_back", "rolled_back_from": phase}))
}

/// Commit the switchover (final phase).
pub async fn commit(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let current = repo::get_active_switchover_for_commit(pool, app_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?
        .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;
    if phase != "COMMIT" { return Err(SwitchoverError::InvalidPhase); }

    repo::insert_switchover_log(pool, *switchover_id, app_id, "COMMIT", "completed", serde_json::json!({}))
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({"switchover_id": switchover_id, "status": "committed"}))
}

/// Get switchover status.
pub async fn get_status(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let logs = repo::get_switchover_history(pool, app_id)
        .await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    if logs.is_empty() { return Ok(serde_json::json!({"status": "no_switchover"})); }

    let (switchover_id, current_phase, current_status, _) = &logs[0];
    let phases: Vec<Value> = logs.iter().map(|(_, phase, status, at)| {
        serde_json::json!({"phase": phase, "status": status, "at": at})
    }).collect();

    Ok(serde_json::json!({
        "switchover_id": switchover_id, "current_phase": current_phase,
        "current_status": current_status, "history": phases,
    }))
}
