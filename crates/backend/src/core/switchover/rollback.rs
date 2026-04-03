//! Switchover rollback, commit, and status.

use serde_json::Value;
use uuid::Uuid;

use crate::db::{DbJson, DbUuid};

use super::SwitchoverError;

/// Rollback the switchover.
pub async fn rollback(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let current = sqlx::query_as::<_, (DbUuid, String)>(
        r#"SELECT switchover_id, phase FROM switchover_log
        WHERE application_id = $1 ORDER BY created_at DESC LIMIT 1"#,
    ).bind(DbUuid::from(app_id)).fetch_optional(pool).await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;

    sqlx::query(
        r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, 'ROLLBACK', 'completed', $4)"#,
    )
    .bind(DbUuid::new_v4()).bind(crate::db::bind_id(switchover_id))
    .bind(DbUuid::from(app_id)).bind(DbJson::from(serde_json::json!({"rolled_back_from": phase})))
    .execute(pool).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({"switchover_id": switchover_id, "status": "rolled_back", "rolled_back_from": phase}))
}

/// Commit the switchover (final phase).
pub async fn commit(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let current = sqlx::query_as::<_, (DbUuid, String)>(
        r#"SELECT switchover_id, phase FROM switchover_log
        WHERE application_id = $1 AND status = 'in_progress'
        ORDER BY created_at DESC LIMIT 1"#,
    ).bind(DbUuid::from(app_id)).fetch_optional(pool).await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;
    if phase != "COMMIT" { return Err(SwitchoverError::InvalidPhase); }

    sqlx::query(
        r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, 'COMMIT', 'completed', $4)"#,
    )
    .bind(DbUuid::new_v4()).bind(crate::db::bind_id(switchover_id))
    .bind(DbUuid::from(app_id)).bind(DbJson::from(serde_json::json!({})))
    .execute(pool).await.map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({"switchover_id": switchover_id, "status": "committed"}))
}

/// Get switchover status.
pub async fn get_status(
    pool: &crate::db::DbPool,
    app_id: impl Into<Uuid>,
) -> Result<Value, SwitchoverError> {
    let app_id: Uuid = app_id.into();

    let logs = sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT switchover_id, phase, status, created_at FROM switchover_log
        WHERE application_id = $1 ORDER BY created_at DESC LIMIT 20"#,
    ).bind(DbUuid::from(app_id)).fetch_all(pool).await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

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
