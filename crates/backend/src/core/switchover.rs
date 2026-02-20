use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SwitchoverError {
    #[error("No active switchover for application")]
    NoActiveSwitchover,
    #[error("Invalid phase transition")]
    InvalidPhase,
    #[error("Database error: {0}")]
    Database(String),
}

/// Start a new switchover process (6 phases: PREPARE → VALIDATE → STOP_SOURCE → SYNC → START_TARGET → COMMIT).
pub async fn start_switchover(
    pool: &sqlx::PgPool,
    app_id: Uuid,
    target_site_id: Uuid,
    mode: &str,
    component_ids: Option<Vec<Uuid>>,
    initiated_by: Uuid,
) -> Result<Uuid, SwitchoverError> {
    let switchover_id = Uuid::new_v4();

    // Insert into switchover_log (append-only)
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, 'PREPARE', 'in_progress',
                $3::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .bind(serde_json::json!({
        "target_site_id": target_site_id,
        "mode": mode,
        "component_ids": component_ids,
        "initiated_by": initiated_by,
    }))
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(switchover_id)
}

/// Advance to the next phase.
pub async fn advance_phase(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    // Get current phase
    let current = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"
        SELECT switchover_id, phase, status
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, current_phase, _status) = current;

    let next_phase = match current_phase.as_str() {
        "PREPARE" => "VALIDATE",
        "VALIDATE" => "STOP_SOURCE",
        "STOP_SOURCE" => "SYNC",
        "SYNC" => "START_TARGET",
        "START_TARGET" => "COMMIT",
        _ => return Err(SwitchoverError::InvalidPhase),
    };

    // Mark current phase as completed
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, $3, 'completed', '{}'::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .bind(&current_phase)
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    // Start next phase
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, $3, 'in_progress', '{}'::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .bind(next_phase)
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "previous_phase": current_phase,
        "current_phase": next_phase,
        "status": "in_progress",
    }))
}

/// Rollback the switchover.
pub async fn rollback(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let current = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT switchover_id, phase
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;

    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, 'ROLLBACK', 'completed',
                $3::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .bind(serde_json::json!({"rolled_back_from": phase}))
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "status": "rolled_back",
        "rolled_back_from": phase,
    }))
}

/// Commit the switchover (final phase).
pub async fn commit(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let current = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT switchover_id, phase
        FROM switchover_log
        WHERE application_id = $1 AND status = 'in_progress'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?
    .ok_or(SwitchoverError::NoActiveSwitchover)?;

    let (switchover_id, phase) = current;

    if phase != "COMMIT" {
        return Err(SwitchoverError::InvalidPhase);
    }

    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES (gen_random_uuid(), $1, $2, 'COMMIT', 'completed', '{}'::jsonb)
        "#,
    )
    .bind(switchover_id)
    .bind(app_id)
    .execute(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "status": "committed",
    }))
}

/// Get switchover status.
pub async fn get_status(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Value, SwitchoverError> {
    let logs = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT switchover_id, phase, status, created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .bind(app_id)
    .fetch_all(pool)
    .await
    .map_err(|e| SwitchoverError::Database(e.to_string()))?;

    if logs.is_empty() {
        return Ok(serde_json::json!({"status": "no_switchover"}));
    }

    let (switchover_id, current_phase, current_status, _) = &logs[0];

    let phases: Vec<Value> = logs.iter().map(|(_, phase, status, at)| {
        serde_json::json!({"phase": phase, "status": status, "at": at})
    }).collect();

    Ok(serde_json::json!({
        "switchover_id": switchover_id,
        "current_phase": current_phase,
        "current_status": current_status,
        "history": phases,
    }))
}
