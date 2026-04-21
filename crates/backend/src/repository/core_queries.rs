//! Query functions for core domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{self, DbJson, DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Permissions queries (core/permissions.rs)
// ============================================================================

/// Get direct user permission level on an application.
#[allow(clippy::too_many_arguments)]
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
pub async fn get_team_permissions(pool: &DbPool, app_id: Uuid, user_id: Uuid) -> Vec<(String,)> {
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
    .bind(crate::db::bind_id(organization_id))
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
    return sqlx::query_scalar::<_, DbUuid>(
        "SELECT site_id FROM applications WHERE id = $1 AND site_id IS NOT NULL",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(DbUuid::into_inner);

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_scalar::<_, DbUuid>(
        "SELECT site_id FROM applications WHERE id = $1 AND site_id IS NOT NULL",
    )
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
    return sqlx::query_scalar::<_, String>("SELECT current_state FROM components WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .fetch_optional(pool)
        .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_scalar::<_, String>("SELECT current_state FROM components WHERE id = $1")
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
    tx: &mut crate::db::DbTransaction<'a>,
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
    tx: &mut crate::db::DbTransaction<'a>,
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
    tx: &mut crate::db::DbTransaction<'a>,
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

// ── SQLite pool-based variants (no transaction, minimal lock duration) ──

/// Fetch component for transition using pool directly (SQLite only).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_component_for_transition_pool(
    pool: &crate::db::DbPool,
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
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let app_id = DbUuid::from(Uuid::parse_str(&r.application_id).unwrap_or(Uuid::nil()));
        (r.current_state, app_id, r.component_name, r.app_name)
    }))
}

/// Update component state using pool directly (SQLite only).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_component_state_pool(
    pool: &crate::db::DbPool,
    component_id: Uuid,
    new_state: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE components SET current_state = $2, updated_at = datetime('now') WHERE id = $1",
    )
    .bind(component_id.to_string())
    .bind(new_state)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert state transition using pool directly (SQLite only).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn insert_state_transition_pool(
    pool: &crate::db::DbPool,
    component_id: Uuid,
    from_state: &str,
    to_state: &str,
    trigger: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(component_id))
    .bind(from_state)
    .bind(to_state)
    .bind(trigger)
    .execute(pool)
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
    #[cfg(feature = "postgres")]
    {
        sqlx::query(
            r#"INSERT INTO check_events (component_id, check_type, exit_code, stdout, duration_ms, metrics)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(crate::db::bind_id(component_id))
        .bind(check_type)
        .bind(exit_code)
        .bind(stdout)
        .bind(duration_ms)
        .bind(metrics)
        .execute(pool)
        .await?;
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        // SQLite: metrics must be stored as TEXT string, not JSON blob.
        // sqlx encodes serde_json::Value as binary BLOB in SQLite, but the
        // metrics column is TEXT and DbJson reads expect TEXT format.
        let metrics_str = metrics
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        sqlx::query(
            r#"INSERT INTO check_events (component_id, check_type, exit_code, stdout, duration_ms, metrics)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(crate::db::bind_id(component_id))
        .bind(check_type)
        .bind(exit_code)
        .bind(stdout)
        .bind(duration_ms)
        .bind(metrics_str)
        .execute(pool)
        .await?;
    }
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
    #[cfg(feature = "postgres")]
    {
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
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_as::<
            _,
            (
                String,
                chrono::DateTime<chrono::Utc>,
                chrono::DateTime<chrono::Utc>,
                DbUuid,
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
        .await?;
        Ok(row.map(|(op, started, hb, uid, status, instance)| {
            (op, started, hb, uid.into_inner(), status, instance)
        }))
    }
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
pub async fn request_cancel_operation(pool: &DbPool, app_id: Uuid) -> Result<u64, sqlx::Error> {
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
    #[cfg(feature = "postgres")]
    {
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
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                chrono::DateTime<chrono::Utc>,
                chrono::DateTime<chrono::Utc>,
                DbUuid,
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
        .await?;
        Ok(rows
            .into_iter()
            .map(|(aid, op, started, hb, uid, status, instance)| {
                (
                    aid.into_inner(),
                    op,
                    started,
                    hb,
                    uid.into_inner(),
                    status,
                    instance,
                )
            })
            .collect())
    }
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

// ============================================================================
// Auto-failover queries (core/auto_failover.rs)
// ============================================================================

/// Row type for failover candidates.
#[derive(Debug, sqlx::FromRow)]
pub struct FailoverCandidate {
    pub application_id: DbUuid,
    pub application_name: String,
    pub active_profile_id: DbUuid,
    pub active_profile_name: String,
    pub dr_profile_id: DbUuid,
    pub dr_profile_name: String,
}

/// Get all failover candidates (apps with active primary + auto_failover DR profile).
pub async fn get_failover_candidates(pool: &DbPool) -> Result<Vec<FailoverCandidate>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = r#"
        SELECT
            app.id as application_id, app.name as application_name,
            active.id as active_profile_id, active.name as active_profile_name,
            dr.id as dr_profile_id, dr.name as dr_profile_name
        FROM applications app
        JOIN binding_profiles active ON active.application_id = app.id AND active.is_active = true
        JOIN binding_profiles dr ON dr.application_id = app.id
            AND dr.profile_type = 'dr' AND dr.auto_failover = true AND dr.is_active = false
        WHERE active.profile_type = 'primary'
    "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = r#"
        SELECT
            app.id as application_id, app.name as application_name,
            active.id as active_profile_id, active.name as active_profile_name,
            dr.id as dr_profile_id, dr.name as dr_profile_name
        FROM applications app
        JOIN binding_profiles active ON active.application_id = app.id AND active.is_active = 1
        JOIN binding_profiles dr ON dr.application_id = app.id
            AND dr.profile_type = 'dr' AND dr.auto_failover = 1 AND dr.is_active = 0
        WHERE active.profile_type = 'primary'
    "#;

    sqlx::query_as(sql).fetch_all(pool).await
}

/// Row type for agent health status in auto-failover.
#[derive(Debug, sqlx::FromRow)]
pub struct AgentHealth {
    pub agent_id: DbUuid,
    pub agent_hostname: String,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
}

/// Get agents for a profile.
pub async fn get_profile_agents(
    pool: &DbPool,
    profile_id: Uuid,
) -> Result<Vec<AgentHealth>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT DISTINCT
            a.id as agent_id, a.hostname as agent_hostname,
            a.last_heartbeat_at, a.is_active
        FROM binding_profile_mappings m
        JOIN agents a ON a.id = m.agent_id
        WHERE m.profile_id = $1
        "#,
    )
    .bind(DbUuid::from(profile_id))
    .fetch_all(pool)
    .await
}

/// Upsert failover health status for an agent (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn upsert_failover_health(
    pool: &DbPool,
    profile_id: &Uuid,
    agent_id: DbUuid,
    is_reachable: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO failover_health_status (profile_id, agent_id, is_reachable, last_check_at, unreachable_since)
        VALUES ($1, $2, $3, $4, CASE WHEN $3 THEN NULL ELSE COALESCE(
            (SELECT unreachable_since FROM failover_health_status WHERE profile_id = $1 AND agent_id = $2),
            $4
        ) END)
        ON CONFLICT (profile_id, agent_id) DO UPDATE SET
            is_reachable = EXCLUDED.is_reachable,
            last_check_at = EXCLUDED.last_check_at,
            unreachable_since = CASE WHEN EXCLUDED.is_reachable THEN NULL ELSE COALESCE(failover_health_status.unreachable_since, EXCLUDED.last_check_at) END
        "#
    )
    .bind(crate::db::bind_id(*profile_id))
    .bind(crate::db::bind_id(agent_id))
    .bind(is_reachable)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert failover health status for an agent (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn upsert_failover_health(
    pool: &DbPool,
    profile_id: &Uuid,
    agent_id: DbUuid,
    is_reachable: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    let is_reachable_int: i32 = if is_reachable { 1 } else { 0 };
    sqlx::query(
        r#"
        INSERT INTO failover_health_status (id, profile_id, agent_id, is_reachable, last_check_at, unreachable_since)
        VALUES ($1, $2, $3, $4, $5, CASE WHEN $4 THEN NULL ELSE COALESCE(
            (SELECT unreachable_since FROM failover_health_status WHERE profile_id = $2 AND agent_id = $3),
            $5
        ) END)
        ON CONFLICT (profile_id, agent_id) DO UPDATE SET
            is_reachable = EXCLUDED.is_reachable,
            last_check_at = EXCLUDED.last_check_at,
            unreachable_since = CASE WHEN EXCLUDED.is_reachable THEN NULL ELSE COALESCE(failover_health_status.unreachable_since, EXCLUDED.last_check_at) END
        "#
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(*profile_id))
    .bind(crate::db::bind_id(agent_id))
    .bind(is_reachable_int)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Check if all unreachable agents have been unreachable long enough.
pub async fn check_unreachable_duration(
    pool: &DbPool,
    profile_id: Uuid,
    threshold: chrono::DateTime<chrono::Utc>,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = r#"
        SELECT COUNT(*) = 0
        FROM failover_health_status
        WHERE profile_id = $1 AND is_reachable = false AND unreachable_since > $2
    "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = r#"
        SELECT COUNT(*) = 0
        FROM failover_health_status
        WHERE profile_id = $1 AND is_reachable = 0 AND unreachable_since > $2
    "#;

    sqlx::query_scalar(sql)
        .bind(DbUuid::from(profile_id))
        .bind(threshold)
        .fetch_one(pool)
        .await
}

/// Log auto-failover action (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn log_auto_failover_action(
    pool: &DbPool,
    user_id: DbUuid,
    app_id: Uuid,
    details: crate::db::DbJson,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO action_log (user_id, action, resource_type, resource_id, details)
        VALUES ($1, 'auto_failover', 'application', $2, $3)
        "#,
    )
    .bind(crate::db::bind_id(user_id))
    .bind(DbUuid::from(app_id))
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

/// Log auto-failover action (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn log_auto_failover_action(
    pool: &DbPool,
    user_id: DbUuid,
    app_id: Uuid,
    details: crate::db::DbJson,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details)
        VALUES ($1, $2, 'auto_failover', 'application', $3, $4)
        "#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(user_id))
    .bind(DbUuid::from(app_id))
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

/// Deactivate a binding profile.
pub async fn deactivate_profile(pool: &DbPool, profile_id: Uuid) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = "UPDATE binding_profiles SET is_active = false WHERE id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = "UPDATE binding_profiles SET is_active = 0 WHERE id = $1";

    sqlx::query(sql)
        .bind(DbUuid::from(profile_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Activate a binding profile.
pub async fn activate_profile(pool: &DbPool, profile_id: Uuid) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = "UPDATE binding_profiles SET is_active = true WHERE id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = "UPDATE binding_profiles SET is_active = 1 WHERE id = $1";

    sqlx::query(sql)
        .bind(DbUuid::from(profile_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Apply DR profile mappings to components (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn apply_profile_mappings(
    pool: &DbPool,
    app_id: Uuid,
    profile_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE components c
        SET agent_id = m.agent_id
        FROM binding_profile_mappings m
        WHERE c.application_id = $1
          AND m.profile_id = $2
          AND c.name = m.component_name
        "#,
    )
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(profile_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Apply DR profile mappings to components (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn apply_profile_mappings(
    pool: &DbPool,
    app_id: Uuid,
    profile_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE components
        SET agent_id = (
            SELECT m.agent_id
            FROM binding_profile_mappings m
            WHERE m.profile_id = $2
              AND m.component_name = components.name
        )
        WHERE application_id = $1
          AND EXISTS (
            SELECT 1 FROM binding_profile_mappings m
            WHERE m.profile_id = $2
              AND m.component_name = components.name
          )
        "#,
    )
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(profile_id))
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Sequencer queries (core/sequencer.rs)
// ============================================================================

/// Get a component's display name (display_name or name fallback).
pub async fn get_component_display_name(pool: &DbPool, component_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(display_name, name) FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| component_id.to_string())
}

/// Get a component's name.
pub async fn get_component_name_by_id(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT name FROM components WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .fetch_optional(pool)
        .await
}

/// Get referenced_app_id for a component (None if not an application-type component).
pub async fn get_component_referenced_app_id(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT referenced_app_id FROM components WHERE id = $1 AND referenced_app_id IS NOT NULL",
        )
        .bind(crate::db::bind_id(component_id))
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT referenced_app_id FROM components WHERE id = $1 AND referenced_app_id IS NOT NULL",
        )
        .bind(DbUuid::from(component_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|v| v.into_inner()))
    }
}

/// Get the sum of start_timeout_seconds for components with start_cmd in an app.
pub async fn get_app_start_timeout_sum(pool: &DbPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(start_timeout_seconds), 120) FROM components
         WHERE application_id = $1 AND start_cmd IS NOT NULL",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(pool)
    .await
    .unwrap_or(120)
}

/// Get the sum of stop_timeout_seconds for components with stop_cmd in an app.
pub async fn get_app_stop_timeout_sum(pool: &DbPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(stop_timeout_seconds), 60) FROM components
         WHERE application_id = $1 AND stop_cmd IS NOT NULL",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(pool)
    .await
    .unwrap_or(60)
}

/// Count components by aggregate state for a referenced app.
pub struct AppStateCount {
    pub total: i64,
    pub running: i64,
    pub degraded: i64,
    pub failed: i64,
}

pub async fn get_app_state_counts(pool: &DbPool, app_id: Uuid) -> AppStateCount {
    #[derive(sqlx::FromRow)]
    struct Row {
        total: i64,
        running: i64,
        degraded: i64,
        failed: i64,
    }

    #[cfg(feature = "postgres")]
    let sql = "SELECT \
            COUNT(*) as total, \
            COUNT(*) FILTER (WHERE current_state = 'RUNNING') as running, \
            COUNT(*) FILTER (WHERE current_state = 'DEGRADED') as degraded, \
            COUNT(*) FILTER (WHERE current_state = 'FAILED') as failed \
         FROM components WHERE application_id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql = "SELECT \
            COUNT(*) as total, \
            SUM(CASE WHEN current_state = 'RUNNING' THEN 1 ELSE 0 END) as running, \
            SUM(CASE WHEN current_state = 'DEGRADED' THEN 1 ELSE 0 END) as degraded, \
            SUM(CASE WHEN current_state = 'FAILED' THEN 1 ELSE 0 END) as failed \
         FROM components WHERE application_id = $1";

    let row = sqlx::query_as::<_, Row>(sql)
        .bind(crate::db::bind_id(app_id))
        .fetch_one(pool)
        .await
        .unwrap_or(Row {
            total: 0,
            running: 0,
            degraded: 0,
            failed: 0,
        });

    AppStateCount {
        total: row.total,
        running: row.running,
        degraded: row.degraded,
        failed: row.failed,
    }
}

/// Count running/active components in a referenced app (for stop decisions).
pub async fn count_running_components_in_app(pool: &DbPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM components
         WHERE application_id = $1
         AND current_state IN ('RUNNING', 'DEGRADED', 'STARTING', 'STOPPING')
         AND stop_cmd IS NOT NULL",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// Get start component info: start_cmd, timeout, agent_id, referenced_app_id.
pub struct StartComponentInfo {
    pub start_cmd: Option<String>,
    pub start_timeout_seconds: i32,
    pub agent_id: Option<Uuid>,
    pub referenced_app_id: Option<Uuid>,
}

pub async fn get_start_component_info(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<StartComponentInfo, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        start_cmd: Option<String>,
        start_timeout_seconds: i32,
        agent_id: Option<DbUuid>,
        referenced_app_id: Option<DbUuid>,
    }

    let row = sqlx::query_as::<_, Row>(
        "SELECT start_cmd, start_timeout_seconds, agent_id, referenced_app_id FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_one(pool)
    .await?;

    Ok(StartComponentInfo {
        start_cmd: row.start_cmd,
        start_timeout_seconds: row.start_timeout_seconds,
        agent_id: row.agent_id.map(|v| v.into_inner()),
        referenced_app_id: row.referenced_app_id.map(|v| v.into_inner()),
    })
}

/// Get stop component info: stop_cmd, timeout, agent_id, referenced_app_id, application_id.
pub struct StopComponentInfo {
    pub stop_cmd: Option<String>,
    pub stop_timeout_seconds: i32,
    pub agent_id: Option<Uuid>,
    pub referenced_app_id: Option<Uuid>,
    pub application_id: Uuid,
}

pub async fn get_stop_component_info(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<StopComponentInfo, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        stop_cmd: Option<String>,
        stop_timeout_seconds: i32,
        agent_id: Option<DbUuid>,
        referenced_app_id: Option<DbUuid>,
        application_id: DbUuid,
    }

    let row = sqlx::query_as::<_, Row>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id, referenced_app_id, application_id FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_one(pool)
    .await?;

    Ok(StopComponentInfo {
        stop_cmd: row.stop_cmd,
        stop_timeout_seconds: row.stop_timeout_seconds,
        agent_id: row.agent_id.map(|v| v.into_inner()),
        referenced_app_id: row.referenced_app_id.map(|v| v.into_inner()),
        application_id: row.application_id.into_inner(),
    })
}

/// Get stop_cmd, stop_timeout_seconds, agent_id for a component (dispatch_stop helper).
pub struct DispatchStopInfo {
    pub stop_cmd: Option<String>,
    pub stop_timeout_seconds: i32,
    pub agent_id: Option<Uuid>,
}

pub async fn get_dispatch_stop_info(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<DispatchStopInfo, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        stop_cmd: Option<String>,
        stop_timeout_seconds: i32,
        agent_id: Option<DbUuid>,
    }

    let row = sqlx::query_as::<_, Row>(
        "SELECT stop_cmd, stop_timeout_seconds, agent_id FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_one(pool)
    .await?;

    Ok(DispatchStopInfo {
        stop_cmd: row.stop_cmd,
        stop_timeout_seconds: row.stop_timeout_seconds,
        agent_id: row.agent_id.map(|v| v.into_inner()),
    })
}

/// Get the application_id for a component.
pub async fn get_component_app_id_uuid(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let id = sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .fetch_one(pool)
        .await?;
    Ok(id.into_inner())
}

/// Record a dispatched command in the command_executions table.
pub async fn record_command_dispatch(
    pool: &DbPool,
    request_id: Uuid,
    component_id: Uuid,
    agent_id: Uuid,
    command_type: &str,
) {
    #[cfg(feature = "postgres")]
    let result = sqlx::query(
        "INSERT INTO command_executions (request_id, component_id, agent_id, command_type, status)
         VALUES ($1, $2, $3, $4, 'dispatched')
         ON CONFLICT (request_id) DO NOTHING",
    )
    .bind(crate::db::bind_id(request_id))
    .bind(crate::db::bind_id(component_id))
    .bind(crate::db::bind_id(agent_id))
    .bind(command_type)
    .execute(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query(
        "INSERT INTO command_executions (id, request_id, component_id, agent_id, command_type, status)
         VALUES ($1, $2, $3, $4, $5, 'dispatched')
         ON CONFLICT (request_id) DO NOTHING",
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(request_id))
    .bind(crate::db::bind_id(component_id))
    .bind(crate::db::bind_id(agent_id))
    .bind(command_type)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(
            request_id = %request_id,
            "Failed to record command dispatch: {}", e
        );
    }
}

/// Record a command result in the command_executions table.
pub async fn record_command_result(
    pool: &DbPool,
    request_id: Uuid,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) {
    let status = if exit_code == 0 {
        "completed"
    } else {
        "failed"
    };
    if let Err(e) = sqlx::query(&format!(
        "UPDATE command_executions \
             SET exit_code = $2, stdout = $3, stderr = $4, status = $5, completed_at = {} \
             WHERE request_id = $1",
        crate::db::sql::now()
    ))
    .bind(crate::db::bind_id(request_id))
    .bind(exit_code as i16)
    .bind(stdout)
    .bind(stderr)
    .bind(status)
    .execute(pool)
    .await
    {
        tracing::warn!(
            request_id = %request_id,
            "Failed to record command result: {}", e
        );
    }
}

/// Log a switchover event.
pub async fn log_switchover_event(
    pool: &DbPool,
    switchover_id: Uuid,
    app_id: Uuid,
    phase: &str,
    status: &str,
    details: crate::db::DbJson,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(switchover_id))
    .bind(DbUuid::from(app_id))
    .bind(phase)
    .bind(status)
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Diagnostic queries (core/diagnostic.rs)
// ============================================================================

/// Fetch component IDs and names for an application.
pub async fn get_components_for_diagnostic(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Fetch latest check results for given component IDs (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_latest_checks(
    pool: &DbPool,
    comp_ids: &[Uuid],
) -> Result<Vec<(DbUuid, String, i16)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, i16)>(
        r#"
        SELECT component_id, check_type, exit_code
        FROM (
            SELECT component_id, check_type, exit_code,
                   ROW_NUMBER() OVER (PARTITION BY component_id, check_type ORDER BY created_at DESC) as rn
            FROM check_events
            WHERE component_id = ANY($1)
              AND check_type IN ('health', 'integrity', 'infrastructure')
        ) ranked
        WHERE rn = 1
        "#,
    )
    .bind(comp_ids)
    .fetch_all(pool)
    .await
}

/// Fetch latest check results for given component IDs (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_latest_checks(
    pool: &DbPool,
    comp_ids: &[Uuid],
) -> Result<Vec<(DbUuid, String, i16)>, sqlx::Error> {
    if comp_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=comp_ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        r#"
        SELECT component_id, check_type, exit_code
        FROM (
            SELECT component_id, check_type, exit_code,
                   ROW_NUMBER() OVER (PARTITION BY component_id, check_type ORDER BY created_at DESC) as rn
            FROM check_events
            WHERE component_id IN ({})
              AND check_type IN ('health', 'integrity', 'infrastructure')
        ) ranked
        WHERE rn = 1
        "#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, (String, String, i16)>(&query);
    for id in comp_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String, String, i16)> = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id_str, check_type, exit_code)| {
            Uuid::parse_str(&id_str)
                .ok()
                .map(|id| (DbUuid::from(id), check_type, exit_code))
        })
        .collect())
}

// ============================================================================
// Orchestration queries (api/orchestration.rs)
// ============================================================================

/// Fetch components with agent info for pre-flight check.
pub async fn get_components_for_preflight(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        Option<DbUuid>,
        Option<String>,
        Option<DbUuid>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            Option<DbUuid>,
            Option<String>,
            Option<DbUuid>,
        ),
    >(
        r#"
        SELECT c.id, c.name, c.agent_id, a.hostname, a.gateway_id
        FROM components c
        LEFT JOIN agents a ON c.agent_id = a.id
        WHERE c.application_id = $1 AND c.is_optional = false
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Fetch gateway name.
pub async fn get_gateway_name_by_id(pool: &DbPool, gateway_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT name FROM gateways WHERE id = $1")
        .bind(crate::db::bind_id(gateway_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Fetch component names and states for an application.
pub async fn get_component_states(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"
        SELECT c.id, c.name, c.current_state
        FROM components c
        WHERE c.application_id = $1
        ORDER BY c.name
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Fetch non-optional component states for wait_running.
pub async fn get_required_component_states(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"
        SELECT c.id, c.name, c.current_state
        FROM components c
        WHERE c.application_id = $1 AND c.is_optional = false
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Fetch component states with agent_id for health check.
pub async fn get_component_states_with_agent(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, Option<DbUuid>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, Option<DbUuid>)>(
        r#"
        SELECT c.id, c.name, c.current_state, c.agent_id
        FROM components c
        WHERE c.application_id = $1 AND c.is_optional = false
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

// ============================================================================
// Rebuild queries (core/rebuild.rs)
// ============================================================================

/// Rebuild target row type.
pub type RebuildTarget = (
    DbUuid,
    String,
    bool,
    Option<String>,
    Option<String>,
    Option<DbUuid>,
);

/// Fetch rebuild targets for specific component IDs.
pub async fn fetch_rebuild_target_by_id(
    pool: &DbPool,
    id: Uuid,
) -> Result<Option<RebuildTarget>, sqlx::Error> {
    sqlx::query_as::<_, RebuildTarget>(
        r#"
        SELECT id, name, rebuild_protected,
               COALESCE(
                   (SELECT so.rebuild_cmd_override FROM site_overrides so WHERE so.component_id = c.id LIMIT 1),
                   rebuild_cmd
               ) as effective_rebuild_cmd,
               rebuild_infra_cmd,
               rebuild_agent_id
        FROM components c WHERE id = $1
        "#,
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(pool)
    .await
}

/// Fetch all rebuild targets for an application.
pub async fn fetch_rebuild_targets_for_app(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<RebuildTarget>, sqlx::Error> {
    sqlx::query_as::<_, RebuildTarget>(
        r#"
        SELECT id, name, rebuild_protected,
               COALESCE(
                   (SELECT so.rebuild_cmd_override FROM site_overrides so WHERE so.component_id = c.id LIMIT 1),
                   rebuild_cmd
               ) as effective_rebuild_cmd,
               rebuild_infra_cmd,
               rebuild_agent_id
        FROM components c WHERE application_id = $1
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Insert rebuild action log (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn insert_rebuild_action_log(
    pool: &DbPool,
    initiated_by: Uuid,
    app_id: Uuid,
    component_count: usize,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO action_log (user_id, action, resource_type, resource_id, details) VALUES ($1, 'rebuild_execute', 'application', $2, $3)",
    )
    .bind(initiated_by)
    .bind(crate::db::bind_id(app_id))
    .bind(serde_json::json!({"components": component_count, "status": "started"}))
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert rebuild action log (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn insert_rebuild_action_log(
    pool: &DbPool,
    initiated_by: Uuid,
    app_id: Uuid,
    component_count: usize,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details) VALUES ($1, $2, 'rebuild_execute', 'application', $3, $4)",
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(initiated_by)
    .bind(crate::db::bind_id(app_id))
    .bind(serde_json::json!({"components": component_count, "status": "started"}).to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Get the agent_id assigned to a component.
pub async fn get_component_agent_id(pool: &DbPool, component_id: Uuid) -> Option<DbUuid> {
    sqlx::query_scalar::<_, DbUuid>(
        "SELECT agent_id FROM components WHERE id = $1 AND agent_id IS NOT NULL",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Poll command execution status.
pub async fn get_command_execution_status(
    pool: &DbPool,
    request_id: Uuid,
) -> Result<Option<(String, Option<i16>, Option<String>)>, sqlx::Error> {
    sqlx::query_as::<_, (String, Option<i16>, Option<String>)>(
        "SELECT status, exit_code, stderr FROM command_executions WHERE request_id = $1",
    )
    .bind(crate::db::bind_id(request_id))
    .fetch_optional(pool)
    .await
}

// ============================================================================
// Heartbeat monitor queries (core/heartbeat_monitor.rs)
// ============================================================================

/// Mark stale gateways as suspended (PostgreSQL only).
#[cfg(feature = "postgres")]
pub async fn mark_stale_gateways_suspended(
    pool: &DbPool,
    timeout_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE gateways
        SET status = 'suspended'
        WHERE status = 'active'
          AND last_heartbeat_at IS NOT NULL
          AND last_heartbeat_at < now() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(timeout_secs)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Reactivate gateways that have reconnected (PostgreSQL only).
#[cfg(feature = "postgres")]
pub async fn reactivate_reconnected_gateways(
    pool: &DbPool,
    timeout_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE gateways
        SET status = 'active'
        WHERE status = 'suspended'
          AND last_heartbeat_at IS NOT NULL
          AND last_heartbeat_at >= now() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(timeout_secs)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Insert state transition to UNREACHABLE (PostgreSQL only).
#[cfg(feature = "postgres")]
pub async fn insert_unreachable_transition(
    pool: &DbPool,
    component_id: DbUuid,
    current_state: &str,
    agent_id_str: &str,
    trigger: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
        VALUES ($1, $2, 'UNREACHABLE', $4,
                jsonb_build_object('previous_state', $2, 'agent_id', $3::text))
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(current_state)
    .bind(agent_id_str)
    .bind(trigger)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update component current_state to UNREACHABLE (PostgreSQL only).
#[cfg(feature = "postgres")]
pub async fn set_component_unreachable(
    pool: &DbPool,
    component_id: DbUuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE components SET current_state = 'UNREACHABLE' WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark stale gateways as suspended (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn mark_stale_gateways_suspended(
    pool: &DbPool,
    timeout_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE gateways
        SET status = 'suspended'
        WHERE status = 'active'
          AND last_heartbeat_at IS NOT NULL
          AND last_heartbeat_at < datetime('now', '-' || $1 || ' seconds')
        "#,
    )
    .bind(timeout_secs)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Reactivate gateways that have reconnected (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn reactivate_reconnected_gateways(
    pool: &DbPool,
    timeout_secs: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE gateways
        SET status = 'active'
        WHERE status = 'suspended'
          AND last_heartbeat_at IS NOT NULL
          AND last_heartbeat_at >= datetime('now', '-' || $1 || ' seconds')
        "#,
    )
    .bind(timeout_secs)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Insert state transition to UNREACHABLE (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn insert_unreachable_transition(
    pool: &DbPool,
    component_id: DbUuid,
    current_state: &str,
    agent_id_str: &str,
    trigger: &str,
) -> Result<(), sqlx::Error> {
    let details = serde_json::json!({
        "previous_state": current_state,
        "agent_id": agent_id_str,
    });
    sqlx::query(
        r#"
        INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger, details)
        VALUES ($1, $2, $3, 'UNREACHABLE', $4, $5)
        "#,
    )
    .bind(DbUuid::new_v4())
    .bind(crate::db::bind_id(component_id))
    .bind(current_state)
    .bind(trigger)
    .bind(details.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Update component current_state to UNREACHABLE (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn set_component_unreachable(
    pool: &DbPool,
    component_id: DbUuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE components SET current_state = 'UNREACHABLE' WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch stale components (whose agents have exceeded heartbeat timeout) — SQLite.
/// Excludes components whose application has an active operation lock (start/stop in progress).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_stale_components<
    T: for<'r> sqlx::FromRow<'r, crate::db::DbRow> + Send + Unpin,
>(
    pool: &DbPool,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        r#"
        SELECT c.id AS component_id, c.name AS component_name,
               c.agent_id AS agent_id, c.application_id AS application_id,
               app.name AS app_name, NOT a.is_active AS agent_blocked
        FROM components c
        JOIN agents a ON a.id = c.agent_id
        JOIN applications app ON app.id = c.application_id
        JOIN organizations o ON o.id = a.organization_id
        LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE c.agent_id IS NOT NULL
          AND (
            (a.is_active = 1
             AND a.last_heartbeat_at IS NOT NULL
             AND a.last_heartbeat_at < datetime('now', '-' || o.heartbeat_timeout_seconds || ' seconds'))
            OR
            (a.is_active = 0
             AND (a.last_heartbeat_at IS NULL
                  OR a.last_heartbeat_at < datetime('now', '-' || o.heartbeat_timeout_seconds || ' seconds')))
          )
          AND NOT EXISTS (
            SELECT 1 FROM operation_locks ol
            WHERE ol.app_id = c.application_id
          )
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Fetch agents with UNREACHABLE components that have reconnected — SQLite.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_agents_to_resync<
    T: for<'r> sqlx::FromRow<'r, crate::db::DbRow> + Send + Unpin,
>(
    pool: &DbPool,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        r#"
        SELECT a.id AS agent_id, COUNT(c.id) AS unreachable_count
        FROM agents a
        JOIN organizations o ON o.id = a.organization_id
        JOIN components c ON c.agent_id = a.id
        LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE a.is_active = 1
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at >= datetime('now', '-' || o.heartbeat_timeout_seconds || ' seconds')
          AND c.current_state = 'UNREACHABLE'
          AND (g.id IS NULL OR g.is_active = 1)
          AND (g.id IS NULL OR g.status = 'active')
        GROUP BY a.id
        HAVING COUNT(c.id) > 0
        "#,
    )
    .fetch_all(pool)
    .await
}

// ============================================================================
// Operation scheduler queries (core/operation_scheduler.rs)
// ============================================================================

/// Get application name by ID (for scheduler).
pub async fn get_application_name(pool: &DbPool, app_id: DbUuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT name FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

// get_component_display_name already exists above in sequencer queries section

/// Insert operation schedule execution (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn insert_schedule_execution(
    pool: &DbPool,
    execution_id: Uuid,
    schedule_id: DbUuid,
    action_log_id: Uuid,
    status: &str,
    message: Option<&str>,
    duration_ms: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO operation_schedule_executions (id, schedule_id, action_log_id, status, message, duration_ms)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(execution_id)
    .bind(crate::db::bind_id(schedule_id))
    .bind(action_log_id)
    .bind(status)
    .bind(message)
    .bind(duration_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert operation schedule execution (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn insert_schedule_execution(
    pool: &DbPool,
    execution_id: Uuid,
    schedule_id: DbUuid,
    action_log_id: Uuid,
    status: &str,
    message: Option<&str>,
    duration_ms: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO operation_schedule_executions (id, schedule_id, action_log_id, status, message, duration_ms)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(execution_id.to_string())
    .bind(schedule_id.to_string())
    .bind(action_log_id.to_string())
    .bind(status)
    .bind(message)
    .bind(duration_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update schedule after run (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn update_operation_schedule_after_run(
    pool: &DbPool,
    schedule_id: DbUuid,
    status: &str,
    message: Option<&str>,
    next_run: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE operation_schedules
        SET last_run_at = now(),
            last_run_status = $2,
            last_run_message = $3,
            next_run_at = $4,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(crate::db::bind_id(schedule_id))
    .bind(status)
    .bind(message)
    .bind(next_run)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch stale components (whose agents have exceeded heartbeat timeout) — PostgreSQL only.
/// Excludes components whose application has an active operation lock (start/stop in progress).
#[cfg(feature = "postgres")]
pub async fn fetch_stale_components<
    T: for<'r> sqlx::FromRow<'r, crate::db::DbRow> + Send + Unpin,
>(
    pool: &DbPool,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        r#"
        SELECT c.id AS component_id, c.name AS component_name,
               c.agent_id AS agent_id, c.application_id AS application_id,
               app.name AS app_name, NOT a.is_active AS agent_blocked
        FROM components c
        JOIN agents a ON a.id = c.agent_id
        JOIN applications app ON app.id = c.application_id
        JOIN organizations o ON o.id = a.organization_id
        LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE c.agent_id IS NOT NULL
          AND (
            (a.is_active = true
             AND a.last_heartbeat_at IS NOT NULL
             AND a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval)
            OR
            (a.is_active = false
             AND (a.last_heartbeat_at IS NULL
                  OR a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval))
          )
          AND NOT EXISTS (
            SELECT 1 FROM operation_locks ol
            WHERE ol.app_id = c.application_id
          )
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Fetch agents with UNREACHABLE components that have reconnected — PostgreSQL only.
#[cfg(feature = "postgres")]
pub async fn fetch_agents_to_resync<
    T: for<'r> sqlx::FromRow<'r, crate::db::DbRow> + Send + Unpin,
>(
    pool: &DbPool,
) -> Result<Vec<T>, sqlx::Error> {
    sqlx::query_as::<_, T>(
        r#"
        SELECT a.id AS agent_id, COUNT(c.id) AS unreachable_count
        FROM agents a
        JOIN organizations o ON o.id = a.organization_id
        JOIN components c ON c.agent_id = a.id
        LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE a.is_active = true
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at >= now() - (o.heartbeat_timeout_seconds || ' seconds')::interval
          AND c.current_state = 'UNREACHABLE'
          AND (g.id IS NULL OR g.is_active = true)
          AND (g.id IS NULL OR g.status = 'active')
        GROUP BY a.id
        HAVING COUNT(c.id) > 0
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Update schedule after run (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_operation_schedule_after_run(
    pool: &DbPool,
    schedule_id: DbUuid,
    status: &str,
    message: Option<&str>,
    next_run: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE operation_schedules
        SET last_run_at = datetime('now'),
            last_run_status = $2,
            last_run_message = $3,
            next_run_at = $4,
            updated_at = datetime('now')
        WHERE id = $1
        "#,
    )
    .bind(schedule_id.to_string())
    .bind(status)
    .bind(message)
    .bind(next_run.map(|dt| dt.to_rfc3339()))
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Certificate rotation queries
// ============================================================================

/// Check if a rotation is already in progress for an organization.
pub async fn find_active_rotation(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Option<(Uuid,)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT rotation_id FROM rotation_progress WHERE organization_id = $1 AND status = 'in_progress'"#,
    ).bind(crate::db::bind_id(org_id)).fetch_optional(pool).await
}

/// Get current CA cert from an organization.
pub async fn get_current_ca(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Option<(Option<String>,)>, sqlx::Error> {
    sqlx::query_as("SELECT ca_cert_pem FROM organizations WHERE id = $1")
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await
}

/// Count agents with certificates.
pub async fn count_certified_agents(pool: &DbPool, org_id: Uuid) -> Result<(i64,), sqlx::Error> {
    sqlx::query_as("SELECT COUNT(*) FROM agents WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL")
        .bind(crate::db::bind_id(org_id)).fetch_one(pool).await
}

/// Count gateways with certificates.
pub async fn count_certified_gateways(pool: &DbPool, org_id: Uuid) -> Result<(i64,), sqlx::Error> {
    sqlx::query_as("SELECT COUNT(*) FROM gateways WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL")
        .bind(crate::db::bind_id(org_id)).fetch_one(pool).await
}

/// Insert a certificate migration record.
pub async fn insert_cert_migration(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    old_fp: &str,
    new_fp: &str,
    hostname: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO certificate_rotations
           (organization_id, rotation_id, agent_id, gateway_id, old_fingerprint, new_fingerprint, status, hostname)
           VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7) ON CONFLICT DO NOTHING"#,
    ).bind(crate::db::bind_id(org_id)).bind(rotation_id).bind(crate::db::bind_opt_id(agent_id)).bind(crate::db::bind_opt_id(gateway_id))
    .bind(old_fp).bind(new_fp).bind(hostname).execute(pool).await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO certificate_rotations
           (id, organization_id, rotation_id, agent_id, gateway_id, old_fingerprint, new_fingerprint, status, hostname)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'completed', $8) ON CONFLICT DO NOTHING"#,
    ).bind(DbUuid::new_v4()).bind(crate::db::bind_id(org_id)).bind(rotation_id).bind(crate::db::bind_opt_id(agent_id)).bind(crate::db::bind_opt_id(gateway_id))
    .bind(old_fp).bind(new_fp).bind(hostname).execute(pool).await?;
    Ok(())
}

/// Insert a certificate migration failure record.
pub async fn insert_cert_migration_failure(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
    agent_id: Option<Uuid>,
    gateway_id: Option<Uuid>,
    old_fp: &str,
    hostname: &str,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"INSERT INTO certificate_rotations
           (organization_id, rotation_id, agent_id, gateway_id, old_fingerprint, status, hostname, error_message)
           VALUES ($1, $2, $3, $4, $5, 'failed', $6, $7) ON CONFLICT DO NOTHING"#,
    ).bind(crate::db::bind_id(org_id)).bind(rotation_id).bind(crate::db::bind_opt_id(agent_id)).bind(crate::db::bind_opt_id(gateway_id))
    .bind(old_fp).bind(hostname).bind(error_message).execute(pool).await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"INSERT INTO certificate_rotations
           (id, organization_id, rotation_id, agent_id, gateway_id, old_fingerprint, status, hostname, error_message)
           VALUES ($1, $2, $3, $4, $5, $6, 'failed', $7, $8) ON CONFLICT DO NOTHING"#,
    ).bind(DbUuid::new_v4()).bind(crate::db::bind_id(org_id)).bind(rotation_id).bind(crate::db::bind_opt_id(agent_id)).bind(crate::db::bind_opt_id(gateway_id))
    .bind(old_fp).bind(hostname).bind(error_message).execute(pool).await?;
    Ok(())
}

/// Increment migrated agents counter.
pub async fn increment_migrated_agents(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE rotation_progress SET migrated_agents = migrated_agents + 1 WHERE organization_id = $1 AND rotation_id = $2")
        .bind(crate::db::bind_id(org_id)).bind(rotation_id).execute(pool).await?;
    Ok(())
}

/// Increment migrated gateways counter.
pub async fn increment_migrated_gateways(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE rotation_progress SET migrated_gateways = migrated_gateways + 1 WHERE organization_id = $1 AND rotation_id = $2")
        .bind(crate::db::bind_id(org_id)).bind(rotation_id).execute(pool).await?;
    Ok(())
}

/// Increment failed agents counter.
pub async fn increment_failed_agents(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE rotation_progress SET failed_agents = failed_agents + 1 WHERE organization_id = $1 AND rotation_id = $2")
        .bind(crate::db::bind_id(org_id)).bind(rotation_id).execute(pool).await?;
    Ok(())
}

/// Increment failed gateways counter.
pub async fn increment_failed_gateways(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE rotation_progress SET failed_gateways = failed_gateways + 1 WHERE organization_id = $1 AND rotation_id = $2")
        .bind(crate::db::bind_id(org_id)).bind(rotation_id).execute(pool).await?;
    Ok(())
}

/// Get rotation progress counts.
pub async fn get_rotation_counts(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<Option<(i32, i32, i32, i32)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT total_agents, total_gateways, migrated_agents, migrated_gateways
           FROM rotation_progress WHERE organization_id = $1 AND rotation_id = $2"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(rotation_id)
    .fetch_optional(pool)
    .await
}

/// Mark rotation as ready.
pub async fn mark_rotation_ready(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE rotation_progress SET status = 'ready', completed_at = {} WHERE organization_id = $1 AND rotation_id = $2 AND status = 'in_progress'",
        crate::db::sql::now()
    )).bind(crate::db::bind_id(org_id)).bind(rotation_id).execute(pool).await?;
    Ok(())
}

/// Get rotation progress details.
#[allow(clippy::type_complexity)]
pub async fn get_rotation_progress_details(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<
    Option<(
        Uuid,
        String,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
        i32,
    )>,
    sqlx::Error,
> {
    sqlx::query_as(
        r#"SELECT rotation_id, status, total_agents, total_gateways,
                  migrated_agents, migrated_gateways, failed_agents, failed_gateways,
                  started_at, completed_at, finalized_at, grace_period_secs
           FROM rotation_progress WHERE organization_id = $1
           ORDER BY started_at DESC LIMIT 1"#,
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_optional(pool)
    .await
}

/// Get CA certs for fingerprinting.
pub async fn get_ca_certs(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT ca_cert_pem, pending_ca_cert_pem FROM organizations WHERE id = $1")
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await
}

/// Get rotation status for finalize/cancel.
pub async fn get_rotation_status(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT rotation_id, status FROM rotation_progress
           WHERE organization_id = $1 ORDER BY started_at DESC LIMIT 1"#,
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_optional(pool)
    .await
}

// ============================================================================
// Resolution queries (migrated from core/resolution.rs)
// ============================================================================

/// Internal row for agent resolution queries
#[derive(Debug, sqlx::FromRow)]
pub struct ResolutionAgentRow {
    pub agent_id: DbUuid,
    pub hostname: String,
    pub gateway_id: Option<DbUuid>,
    pub gateway_name: Option<String>,
    pub ip_addresses: sqlx::types::Json<Vec<String>>,
    pub is_active: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PatternRuleRow {
    pub search_pattern: String,
    pub replace_pattern: String,
}

#[cfg(feature = "postgres")]
pub async fn query_agents_exact_hostname(
    pool: &DbPool,
    org_id: DbUuid,
    host_lower: &str,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    sqlx::query_as::<_, ResolutionAgentRow>(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND LOWER(a.hostname) = $2 AND a.gateway_id = ANY($3)"#,
    ).bind(crate::db::bind_id(org_id)).bind(host_lower).bind(gateway_ids).fetch_all(pool).await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn query_agents_exact_hostname(
    pool: &DbPool,
    org_id: DbUuid,
    host_lower: &str,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    if gateway_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (4..=3 + gateway_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND LOWER(a.hostname) = $2 AND a.gateway_id IN ({})"#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, ResolutionAgentRow>(&query)
        .bind(org_id.to_string())
        .bind(host_lower);
    for gid in gateway_ids {
        q = q.bind(gid.to_string());
    }
    q.fetch_all(pool).await
}

#[cfg(feature = "postgres")]
pub async fn query_agents_fqdn_match(
    pool: &DbPool,
    org_id: DbUuid,
    fqdn_pattern: &str,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    sqlx::query_as::<_, ResolutionAgentRow>(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND LOWER(a.hostname) LIKE $2 || '%' AND a.gateway_id = ANY($3)"#,
    ).bind(crate::db::bind_id(org_id)).bind(fqdn_pattern).bind(gateway_ids).fetch_all(pool).await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn query_agents_fqdn_match(
    pool: &DbPool,
    org_id: DbUuid,
    fqdn_pattern: &str,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    if gateway_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (4..=3 + gateway_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND LOWER(a.hostname) LIKE $2 || '%' AND a.gateway_id IN ({})"#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, ResolutionAgentRow>(&query)
        .bind(org_id.to_string())
        .bind(fqdn_pattern);
    for gid in gateway_ids {
        q = q.bind(gid.to_string());
    }
    q.fetch_all(pool).await
}

#[cfg(feature = "postgres")]
pub async fn query_agents_ip_match(
    pool: &DbPool,
    org_id: DbUuid,
    ip: &str,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    sqlx::query_as::<_, ResolutionAgentRow>(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND a.ip_addresses @> $2::jsonb AND a.gateway_id = ANY($3)"#,
    ).bind(crate::db::bind_id(org_id)).bind(serde_json::json!([ip])).bind(gateway_ids).fetch_all(pool).await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn query_agents_ip_match(
    pool: &DbPool,
    org_id: DbUuid,
    ip: &str,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    if gateway_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (4..=3 + gateway_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND EXISTS (SELECT 1 FROM json_each(a.ip_addresses) WHERE json_each.value = $2)
          AND a.gateway_id IN ({})"#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, ResolutionAgentRow>(&query)
        .bind(org_id.to_string())
        .bind(ip);
    for gid in gateway_ids {
        q = q.bind(gid.to_string());
    }
    q.fetch_all(pool).await
}

#[cfg(feature = "postgres")]
pub async fn query_agents_list(
    pool: &DbPool,
    org_id: DbUuid,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    sqlx::query_as::<_, ResolutionAgentRow>(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND a.gateway_id = ANY($2) ORDER BY a.hostname"#,
    ).bind(crate::db::bind_id(org_id)).bind(gateway_ids).fetch_all(pool).await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn query_agents_list(
    pool: &DbPool,
    org_id: DbUuid,
    gateway_ids: &[Uuid],
) -> Result<Vec<ResolutionAgentRow>, sqlx::Error> {
    if gateway_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (2..=1 + gateway_ids.len())
        .map(|i| format!("${}", i))
        .collect();
    let query = format!(
        r#"SELECT a.id AS agent_id, a.hostname, a.gateway_id, g.name AS gateway_name, a.ip_addresses, a.is_active
        FROM agents a LEFT JOIN gateways g ON a.gateway_id = g.id
        WHERE a.organization_id = $1 AND a.gateway_id IN ({}) ORDER BY a.hostname"#,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as::<_, ResolutionAgentRow>(&query).bind(org_id.to_string());
    for gid in gateway_ids {
        q = q.bind(gid.to_string());
    }
    q.fetch_all(pool).await
}

/// Fetch DR pattern rules for hostname suggestion.
pub async fn fetch_pattern_rules(
    pool: &DbPool,
    org_id: DbUuid,
) -> Result<Vec<PatternRuleRow>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let rules_sql: &str = "SELECT search_pattern, replace_pattern FROM dr_pattern_rules WHERE organization_id = $1 AND is_active = true ORDER BY priority DESC";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rules_sql: &str = "SELECT search_pattern, replace_pattern FROM dr_pattern_rules WHERE organization_id = $1 AND is_active = 1 ORDER BY priority DESC";
    sqlx::query_as(rules_sql)
        .bind(crate::db::bind_id(org_id))
        .fetch_all(pool)
        .await
}

// ============================================================================
// Certificate rotation transactional queries
// ============================================================================

/// Store pending CA and create rotation progress in a transaction.
pub async fn start_rotation_tx(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
    new_ca_cert_pem: &str,
    new_ca_key_pem: &str,
    agent_count: i64,
    gateway_count: i64,
    initiated_by: Uuid,
    grace_period_secs: i32,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(&format!(
        "UPDATE organizations \
             SET pending_ca_cert_pem = $2, pending_ca_key_pem = $3, rotation_started_at = {} \
             WHERE id = $1",
        db::sql::now()
    ))
    .bind(crate::db::bind_id(org_id))
    .bind(new_ca_cert_pem)
    .bind(new_ca_key_pem)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO rotation_progress
           (organization_id, rotation_id, total_agents, total_gateways, initiated_by, grace_period_secs)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(rotation_id)
    .bind(agent_count as i32)
    .bind(gateway_count as i32)
    .bind(initiated_by)
    .bind(grace_period_secs)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Finalize rotation: swap pending CA to primary and mark completed.
pub async fn finalize_rotation_tx(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"UPDATE organizations
           SET ca_cert_pem = pending_ca_cert_pem,
               ca_key_pem = pending_ca_key_pem,
               pending_ca_cert_pem = NULL,
               pending_ca_key_pem = NULL,
               rotation_started_at = NULL
           WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(org_id))
    .execute(&mut *tx)
    .await?;

    sqlx::query(&format!(
        "UPDATE rotation_progress \
             SET status = 'completed', finalized_at = {} \
             WHERE organization_id = $1 AND rotation_id = $2",
        db::sql::now()
    ))
    .bind(crate::db::bind_id(org_id))
    .bind(rotation_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Cancel rotation: clear pending CA and mark cancelled.
pub async fn cancel_rotation_tx(
    pool: &DbPool,
    org_id: Uuid,
    rotation_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"UPDATE organizations
           SET pending_ca_cert_pem = NULL,
               pending_ca_key_pem = NULL,
               rotation_started_at = NULL
           WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(org_id))
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"UPDATE rotation_progress
           SET status = 'cancelled'
           WHERE organization_id = $1 AND rotation_id = $2"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(rotation_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

// ============================================================================
// Cluster aggregation queries (core/fsm.rs::process_member_check_result)
// ============================================================================

/// Fetch (cluster_health_policy, cluster_min_healthy_pct) for a component.
/// Returns None if the component does not exist.
pub async fn fetch_cluster_policy(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<(String, i16)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let row: Option<(String, i16)> = sqlx::query_as(
            "SELECT cluster_health_policy, cluster_min_healthy_pct FROM components WHERE id = $1",
        )
        .bind(crate::db::bind_id(component_id))
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row: Option<(String, i16)> = sqlx::query_as(
            "SELECT cluster_health_policy, cluster_min_healthy_pct FROM components WHERE id = $1",
        )
        .bind(DbUuid::from(component_id))
        .fetch_optional(pool)
        .await?;
        Ok(row)
    }
}
