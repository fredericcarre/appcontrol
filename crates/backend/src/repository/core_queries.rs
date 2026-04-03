//! Query functions for core domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{self, DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Permissions queries (core/permissions.rs)
// ============================================================================

/// Get direct user permission level on an application.
pub async fn get_direct_user_permission(
    pool: &DbPool,
    app_id: Uuid,
    user_id: Uuid,
) -> Option<String> {
    let sql = format!(
        "SELECT permission_level FROM app_permissions_users \
         WHERE application_id = $1 AND user_id = $2 \
         AND (expires_at IS NULL OR expires_at > {})",
        db::sql::now()
    );

    #[cfg(feature = "postgres")]
    return sqlx::query_scalar::<_, String>(&sql)
        .bind(crate::db::bind_id(app_id))
        .bind(crate::db::bind_id(user_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_scalar::<_, String>(&sql)
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(user_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
}

/// Get all team permission levels for a user on an application.
pub async fn get_team_permissions(
    pool: &DbPool,
    app_id: Uuid,
    user_id: Uuid,
) -> Vec<(String,)> {
    let sql = format!(
        "SELECT apt.permission_level \
         FROM app_permissions_teams apt \
         JOIN team_members tm ON tm.team_id = apt.team_id \
         WHERE apt.application_id = $1 AND tm.user_id = $2 \
         AND (apt.expires_at IS NULL OR apt.expires_at > {})",
        db::sql::now()
    );

    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, (String,)>(&sql)
        .bind(crate::db::bind_id(app_id))
        .bind(crate::db::bind_id(user_id))
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, (String,)>(&sql)
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(user_id))
        .fetch_all(pool)
        .await
        .unwrap_or_default();
}

/// Check if any workspace_sites exist for an organization (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn has_any_workspace_sites(pool: &DbPool, organization_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM workspace_sites ws
            JOIN workspaces w ON w.id = ws.workspace_id
            WHERE w.organization_id = $1
        )
        "#,
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// Check if any workspace_sites exist for an organization (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn has_any_workspace_sites(pool: &DbPool, organization_id: Uuid) -> bool {
    let count = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT COUNT(*) FROM workspace_sites ws
        JOIN workspaces w ON w.id = ws.workspace_id
        WHERE w.organization_id = $1
        "#,
    )
    .bind(DbUuid::from(organization_id))
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    count > 0
}

/// Check if a user has site access via workspace membership (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn has_site_access(pool: &DbPool, site_id: Uuid, user_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM workspace_sites ws
            JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
            WHERE ws.site_id = $1
              AND (
                  wm.user_id = $2
                  OR wm.team_id IN (
                      SELECT team_id FROM team_members WHERE user_id = $2
                  )
              )
        )
        "#,
    )
    .bind(crate::db::bind_id(site_id))
    .bind(crate::db::bind_id(user_id))
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// Check if a user has site access via workspace membership (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn has_site_access(pool: &DbPool, site_id: Uuid, user_id: Uuid) -> bool {
    let count = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT COUNT(*) FROM workspace_sites ws
        JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
        WHERE ws.site_id = $1
          AND (
              wm.user_id = $2
              OR wm.team_id IN (
                  SELECT team_id FROM team_members WHERE user_id = $2
              )
          )
        "#,
    )
    .bind(DbUuid::from(site_id))
    .bind(DbUuid::from(user_id))
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    count > 0
}

/// Get component info for permission check (app_id, gateway_id, org_id).
pub async fn get_component_permission_info(
    pool: &DbPool,
    component_id: Uuid,
) -> Option<(DbUuid, Option<DbUuid>, DbUuid)> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, (DbUuid, Option<DbUuid>, DbUuid)>(
        r#"
        SELECT c.application_id, a.gateway_id, app.organization_id
        FROM components c
        JOIN applications app ON app.id = c.application_id
        LEFT JOIN agents a ON a.id = c.agent_id
        WHERE c.id = $1
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, (DbUuid, Option<DbUuid>, DbUuid)>(
        r#"
        SELECT c.application_id, a.gateway_id, app.organization_id
        FROM components c
        JOIN applications app ON app.id = c.application_id
        LEFT JOIN agents a ON a.id = c.agent_id
        WHERE c.id = $1
        "#,
    )
    .bind(DbUuid::from(component_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
}

/// Get site_id for an application.
pub async fn get_app_site_id(pool: &DbPool, app_id: Uuid) -> Option<Uuid> {
    #[cfg(feature = "postgres")]
    return sqlx::query_scalar::<_, DbUuid>("SELECT site_id FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(DbUuid::into_inner);

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_scalar::<_, DbUuid>("SELECT site_id FROM applications WHERE id = $1")
        .bind(DbUuid::from(app_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(DbUuid::into_inner);
}

// ============================================================================
// FSM queries (core/fsm.rs)
// ============================================================================

/// Get current state string for a component.
pub async fn get_component_current_state(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_scalar::<_, String>(
        "SELECT current_state FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_scalar::<_, String>(
        "SELECT current_state FROM components WHERE id = $1",
    )
    .bind(DbUuid::from(component_id))
    .fetch_optional(pool)
    .await;
}

/// Get current states for multiple components (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn get_component_states_bulk(
    pool: &DbPool,
    component_ids: &[Uuid],
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        "SELECT id, current_state FROM components WHERE id = ANY($1)",
    )
    .bind(component_ids)
    .fetch_all(pool)
    .await
}

/// Get current states for multiple components (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn get_component_states_bulk(
    pool: &DbPool,
    component_ids: &[Uuid],
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
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
        .filter_map(|(id_str, state)| {
            Uuid::parse_str(&id_str)
                .ok()
                .map(|id| (DbUuid::from(id), state))
        })
        .collect())
}

/// Fetch component for state transition with row lock (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_component_for_transition<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    component_id: Uuid,
) -> Result<Option<(String, DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, DbUuid, String, String)>(
        r#"SELECT c.current_state, c.application_id, c.name, a.name
           FROM components c
           JOIN applications a ON c.application_id = a.id
           WHERE c.id = $1 FOR UPDATE OF c"#,
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(&mut **tx)
    .await
}

/// Fetch component for state transition (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_component_for_transition<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    component_id: Uuid,
) -> Result<Option<(String, DbUuid, String, String)>, sqlx::Error> {
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
        let app_id = DbUuid::from(Uuid::parse_str(&r.application_id).unwrap_or(Uuid::nil()));
        (r.current_state, app_id, r.component_name, r.app_name)
    }))
}

/// Insert state transition record (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn insert_state_transition<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    component_id: Uuid,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(from_state)
    .bind(to_state)
    .bind(trigger)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Insert state transition record (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn insert_state_transition<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Sqlite>,
    component_id: Uuid,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(component_id))
    .bind(from_state)
    .bind(to_state)
    .bind(trigger)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Update component state with timestamp (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn update_component_state<'a>(
    tx: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    component_id: Uuid,
    new_state: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE components SET current_state = $2, updated_at = now() WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .bind(new_state)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Update component state with timestamp (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_component_state<'a>(
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

/// Store a check event.
pub async fn store_check_event(
    pool: &DbPool,
    component_id: Uuid,
    check_type: &str,
    exit_code: i16,
    stdout: &Option<String>,
    duration_ms: i32,
    metrics: &Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO check_events (component_id, check_type, exit_code, stdout, duration_ms, metrics)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(component_id)
    .bind(check_type)
    .bind(exit_code)
    .bind(stdout)
    .bind(duration_ms)
    .bind(metrics)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Operation lock queries (core/operation_lock.rs)
// ============================================================================

/// Try to insert an operation lock (returns Some row if acquired, None if conflict).
pub async fn try_insert_operation_lock(
    pool: &DbPool,
    app_id: Uuid,
    operation: &str,
    user_id: Uuid,
    instance_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO operation_locks (app_id, operation, user_id, backend_instance)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (app_id) DO NOTHING
        RETURNING app_id
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(operation)
    .bind(crate::db::bind_id(user_id))
    .bind(instance_id)
    .fetch_optional(pool)
    .await?;

    Ok(result.is_some())
}

/// Get active operation for an app.
pub async fn get_active_operation(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<
    Option<(
        String,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        Uuid,
        String,
        Option<String>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            Uuid,
            String,
            Option<String>,
        ),
    >(
        r#"
        SELECT operation, started_at, last_heartbeat, user_id, status, backend_instance
        FROM operation_locks
        WHERE app_id = $1
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_optional(pool)
    .await
}

/// Get operation lock status for cancellation check.
pub async fn get_lock_status(pool: &DbPool, app_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT status FROM operation_locks WHERE app_id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Request cancellation of an operation.
pub async fn request_cancel_operation(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE operation_locks
        SET status = 'cancelling'
        WHERE app_id = $1 AND status = 'running'
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Force delete an operation lock.
pub async fn delete_operation_lock(pool: &DbPool, app_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM operation_locks WHERE app_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Clean up stale lock for a specific app (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn cleanup_stale_lock(
    pool: &DbPool,
    app_id: Uuid,
    threshold_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM operation_locks
        WHERE app_id = $1
          AND last_heartbeat < NOW() - INTERVAL '1 second' * $2
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(threshold_secs)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Clean up stale lock for a specific app (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn cleanup_stale_lock(
    pool: &DbPool,
    app_id: Uuid,
    threshold_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM operation_locks
        WHERE app_id = $1
          AND last_heartbeat < datetime('now', '-' || $2 || ' seconds')
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(threshold_secs)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Clean up all stale locks (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn cleanup_all_stale_locks(
    pool: &DbPool,
    threshold_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM operation_locks
        WHERE last_heartbeat < NOW() - INTERVAL '1 second' * $1
        "#,
    )
    .bind(threshold_secs)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Clean up all stale locks (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn cleanup_all_stale_locks(
    pool: &DbPool,
    threshold_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM operation_locks
        WHERE last_heartbeat < datetime('now', '-' || $1 || ' seconds')
        "#,
    )
    .bind(threshold_secs)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// List all operation locks.
pub async fn list_all_operation_locks(
    pool: &DbPool,
) -> Result<
    Vec<(
        Uuid,
        String,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        Uuid,
        String,
        Option<String>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            Uuid,
            String,
            Option<String>,
        ),
    >(
        r#"
        SELECT app_id, operation, started_at, last_heartbeat, user_id, status, backend_instance
        FROM operation_locks
        ORDER BY started_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Update operation heartbeat (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn update_heartbeat(pool: &DbPool, app_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE operation_locks SET last_heartbeat = NOW() WHERE app_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Update operation heartbeat (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_heartbeat(pool: &DbPool, app_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE operation_locks SET last_heartbeat = datetime('now') WHERE app_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    Ok(())
}
