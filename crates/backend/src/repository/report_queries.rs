//! Query functions for report domain. All sqlx queries live here.

#![allow(unused_imports, dead_code, clippy::too_many_arguments)]
use crate::db::{DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Global Audit Log queries
// ============================================================================

pub type GlobalAuditRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    Uuid,
    serde_json::Value,
    chrono::DateTime<chrono::Utc>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

#[cfg(feature = "postgres")]
pub async fn fetch_global_audit_logs(
    db: &DbPool,
    org_id: Uuid,
    app_id: Option<Uuid>,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GlobalAuditRow>, sqlx::Error> {
    sqlx::query_as::<_, GlobalAuditRow>(
        r#"
        SELECT
            al.id, al.user_id,
            COALESCE(u.email, 'system') as user_email,
            al.action, al.resource_type, al.resource_id, al.details, al.created_at,
            app.name as app_name, comp.name as component_name,
            ag.hostname as agent_hostname, gw.name as gateway_name
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        LEFT JOIN applications app ON app.id = al.resource_id AND al.resource_type = 'application'
        LEFT JOIN components comp ON comp.id = al.resource_id AND al.resource_type = 'component'
        LEFT JOIN agents ag ON ag.id = al.resource_id AND al.resource_type = 'agent'
        LEFT JOIN gateways gw ON gw.id = al.resource_id AND al.resource_type = 'gateway'
        WHERE u.organization_id = $1
          AND ($2::uuid IS NULL OR al.resource_id = $2)
          AND ($3::uuid IS NULL OR al.user_id = $3)
        ORDER BY al.created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(org_id)
    .bind(app_id)
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_global_audit_logs(
    db: &DbPool,
    org_id: Uuid,
    app_id: Option<Uuid>,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GlobalAuditRow>, sqlx::Error> {
    sqlx::query_as::<_, GlobalAuditRow>(
        r#"
        SELECT
            al.id, al.user_id,
            COALESCE(u.email, 'system') as user_email,
            al.action, al.resource_type, al.resource_id, al.details, al.created_at,
            app.name as app_name, comp.name as component_name,
            ag.hostname as agent_hostname, gw.name as gateway_name
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        LEFT JOIN applications app ON app.id = al.resource_id AND al.resource_type = 'application'
        LEFT JOIN components comp ON comp.id = al.resource_id AND al.resource_type = 'component'
        LEFT JOIN agents ag ON ag.id = al.resource_id AND al.resource_type = 'agent'
        LEFT JOIN gateways gw ON gw.id = al.resource_id AND al.resource_type = 'gateway'
        WHERE u.organization_id = $1
          AND ($2 IS NULL OR al.resource_id = $2)
          AND ($3 IS NULL OR al.user_id = $3)
        ORDER BY al.created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(org_id)
    .bind(app_id)
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

// ============================================================================
// Availability Stats
// ============================================================================

#[cfg(feature = "postgres")]
pub async fn fetch_availability_stats(
    db: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(DbUuid, String, i64, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, i64, i64)>(
        r#"SELECT component_id, date::text,
               COALESCE(running_seconds, 0) as running_seconds,
               COALESCE(total_seconds, 86400) as total_seconds
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= $2::date AND date <= $3::date
        ORDER BY date"#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_availability_stats(
    db: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(DbUuid, String, i64, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, i64, i64)>(
        r#"SELECT component_id, CAST(date AS TEXT),
               COALESCE(running_seconds, 0) as running_seconds,
               COALESCE(total_seconds, 86400) as total_seconds
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= date($2) AND date <= date($3)
        ORDER BY date"#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

// ============================================================================
// Switchover Logs
// ============================================================================

#[cfg(feature = "postgres")]
pub async fn fetch_switchover_logs(
    db: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, phase, status, details::text, created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 100"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_switchover_logs(
    db: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, phase, status, CAST(details AS TEXT), created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 100"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

// ============================================================================
// Topology Components
// ============================================================================

#[cfg(feature = "postgres")]
pub async fn fetch_topology_components(
    db: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, f64, f64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, f64, f64)>(
        r#"SELECT id, name, component_type,
               COALESCE(position_x, 0)::float8,
               COALESCE(position_y, 0)::float8
        FROM components WHERE application_id = $1 ORDER BY name"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_topology_components(
    db: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, f64, f64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, f64, f64)>(
        r#"SELECT id, name, component_type,
               COALESCE(position_x, 0.0),
               COALESCE(position_y, 0.0)
        FROM components WHERE application_id = $1 ORDER BY name"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

// ============================================================================
// Average RTO
// ============================================================================

#[cfg(feature = "postgres")]
pub async fn fetch_avg_rto(db: &DbPool, app_id: Uuid) -> Option<f64> {
    sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT AVG(EXTRACT(EPOCH FROM (
            (SELECT MAX(created_at) FROM switchover_log sl2 WHERE sl2.switchover_id = sl.switchover_id AND sl2.phase = 'COMMIT')
            - sl.created_at
        )))
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.phase = 'PREPARE'"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(db)
    .await
    .unwrap_or(None)
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_avg_rto(db: &DbPool, app_id: Uuid) -> Option<f64> {
    sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT AVG(
            (julianday((SELECT MAX(created_at) FROM switchover_log sl2 WHERE sl2.switchover_id = sl.switchover_id AND sl2.phase = 'COMMIT'))
             - julianday(sl.created_at)) * 86400.0
        )
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.phase = 'PREPARE'"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(db)
    .await
    .unwrap_or(None)
}

// ============================================================================
// Availability Summary
// ============================================================================

#[cfg(feature = "postgres")]
pub async fn fetch_availability_summary(
    db: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COALESCE(SUM(running_seconds), 0), COALESCE(SUM(total_seconds), 1)
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= $2::date AND date <= $3::date"#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await
    .unwrap_or((0, 1))
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_availability_summary(
    db: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT COALESCE(SUM(running_seconds), 0), COALESCE(SUM(total_seconds), 1)
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= date($2) AND date <= date($3)"#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await
    .unwrap_or((0, 1))
}

// ============================================================================
// MTTR Recoveries
// ============================================================================

#[cfg(feature = "postgres")]
pub async fn fetch_mttr_recoveries(
    db: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(Uuid, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, i64)>(
        r#"
        WITH failed_events AS (
            SELECT st.component_id, c.name as component_name, st.created_at as failed_at,
                   ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1 AND st.to_state = 'FAILED'
              AND st.created_at >= $2 AND st.created_at <= $3
        ),
        recovery_events AS (
            SELECT st.component_id, st.created_at as recovered_at,
                   ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1 AND st.to_state = 'RUNNING'
              AND st.from_state IN ('FAILED', 'STARTING')
              AND st.created_at >= $2 AND st.created_at <= $3
        )
        SELECT f.component_id, f.component_name, f.failed_at, r.recovered_at,
               EXTRACT(EPOCH FROM (r.recovered_at - f.failed_at))::bigint as recovery_seconds
        FROM failed_events f
        JOIN recovery_events r ON f.component_id = r.component_id
        WHERE r.recovered_at > f.failed_at
          AND NOT EXISTS (
            SELECT 1 FROM state_transitions st2
            WHERE st2.component_id = f.component_id AND st2.to_state = 'FAILED'
              AND st2.created_at > f.failed_at AND st2.created_at < r.recovered_at
          )
        ORDER BY f.failed_at DESC LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_mttr_recoveries(
    db: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(Uuid, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, i64)>(
        r#"
        WITH failed_events AS (
            SELECT st.component_id, c.name as component_name, st.created_at as failed_at,
                   ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1 AND st.to_state = 'FAILED'
              AND st.created_at >= $2 AND st.created_at <= $3
        ),
        recovery_events AS (
            SELECT st.component_id, st.created_at as recovered_at,
                   ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1 AND st.to_state = 'RUNNING'
              AND st.from_state IN ('FAILED', 'STARTING')
              AND st.created_at >= $2 AND st.created_at <= $3
        )
        SELECT f.component_id, f.component_name, f.failed_at, r.recovered_at,
               CAST((julianday(r.recovered_at) - julianday(f.failed_at)) * 86400 AS INTEGER) as recovery_seconds
        FROM failed_events f
        JOIN recovery_events r ON f.component_id = r.component_id
        WHERE r.recovered_at > f.failed_at
          AND NOT EXISTS (
            SELECT 1 FROM state_transitions st2
            WHERE st2.component_id = f.component_id AND st2.to_state = 'FAILED'
              AND st2.created_at > f.failed_at AND st2.created_at < r.recovered_at
          )
        ORDER BY f.failed_at DESC LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

// ============================================================================
// Additional report queries (migrated from api/reports.rs)
// ============================================================================

pub async fn fetch_incidents(
    db: &DbPool, app_id: Uuid, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(DbUuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT st.component_id, c.name, st.to_state, st.created_at
        FROM state_transitions st JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.to_state = 'FAILED'
          AND st.created_at >= $2 AND st.created_at <= $3 ORDER BY st.created_at DESC"#,
    ).bind(crate::db::bind_id(app_id)).bind(from).bind(to).fetch_all(db).await
}

pub async fn get_app_info_for_report(db: &DbPool, app_id: Uuid) -> Result<Option<(String, Option<DbUuid>, Option<String>)>, sqlx::Error> {
    sqlx::query_as::<_, (String, Option<DbUuid>, Option<String>)>(
        "SELECT a.name, a.site_id, s.name FROM applications a LEFT JOIN sites s ON a.site_id = s.id WHERE a.id = $1"
    ).bind(crate::db::bind_id(app_id)).fetch_optional(db).await
}

pub async fn get_switchover_log_entries(db: &DbPool, app_id: Uuid) -> Result<Vec<(DbUuid, String, String, Value, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, Value, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT switchover_id, phase, status, details, created_at FROM switchover_log
        WHERE application_id = $1 ORDER BY switchover_id, created_at ASC"#,
    ).bind(crate::db::bind_id(app_id)).fetch_all(db).await
}

pub async fn get_all_sites(db: &DbPool) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>("SELECT id, name FROM sites").fetch_all(db).await
}

pub async fn get_transitions_in_range(
    db: &DbPool, app_id: Uuid, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>, states: &[&str],
) -> Result<Vec<(String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    let states_str = states.iter().map(|s| format!("'{}'", s)).collect::<Vec<_>>().join(",");
    let sql = format!(
        r#"SELECT c.name, st.from_state, st.to_state, st.created_at FROM state_transitions st
        JOIN components c ON c.id = st.component_id WHERE c.application_id = $1
          AND st.created_at >= $2 AND st.created_at <= $3 AND st.to_state IN ({}) ORDER BY st.created_at ASC"#, states_str
    );
    sqlx::query_as::<_, (String, String, String, chrono::DateTime<chrono::Utc>)>(&sql)
        .bind(crate::db::bind_id(app_id)).bind(from).bind(to).fetch_all(db).await
}

pub async fn get_user_email(db: &DbPool, user_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(crate::db::bind_id(user_id)).fetch_optional(db).await.ok().flatten()
}

#[allow(clippy::type_complexity)]
pub async fn get_commands_in_range(
    db: &DbPool, app_id: Uuid, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, String, Option<String>, Option<String>, Option<String>, Option<String>, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, Option<String>, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT st.to_state, c.name, c.start_cmd, c.stop_cmd, a.hostname, g.name, st.created_at
        FROM state_transitions st JOIN components c ON c.id = st.component_id
        LEFT JOIN agents a ON a.id = c.agent_id LEFT JOIN gateways g ON g.id = a.gateway_id
        WHERE c.application_id = $1 AND st.created_at >= $2 AND st.created_at <= $3
          AND st.to_state IN ('STARTING', 'STOPPING') ORDER BY st.created_at ASC"#,
    ).bind(crate::db::bind_id(app_id)).bind(from).bind(to).fetch_all(db).await
}

pub async fn get_app_dependencies(db: &DbPool, app_id: Uuid) -> Result<Vec<(DbUuid, DbUuid)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT d.from_component_id, d.to_component_id FROM dependencies d WHERE d.application_id = $1",
    ).bind(crate::db::bind_id(app_id)).fetch_all(db).await
}

pub async fn fetch_audit_log(
    db: &DbPool, app_id: Uuid, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(DbUuid, DbUuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, user_id, action, resource_type, created_at FROM action_log
        WHERE resource_id = $1 AND created_at >= $2 AND created_at <= $3 ORDER BY created_at DESC LIMIT 500"#,
    ).bind(crate::db::bind_id(app_id)).bind(from).bind(to).fetch_all(db).await
}

pub async fn count_action_log(db: &DbPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM action_log WHERE resource_id = $1")
        .bind(crate::db::bind_id(app_id)).fetch_one(db).await.unwrap_or(0)
}

pub async fn get_app_name(db: &DbPool, app_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT name FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id)).fetch_optional(db).await.ok().flatten()
}

pub async fn count_incidents(db: &DbPool, app_id: Uuid, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> i64 {
    sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM state_transitions st JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.to_state = 'FAILED' AND st.created_at >= $2 AND st.created_at <= $3"#,
    ).bind(crate::db::bind_id(app_id)).bind(from).bind(to).fetch_one(db).await.unwrap_or(0)
}

pub async fn count_switchovers(db: &DbPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT switchover_id) FROM switchover_log WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id)).fetch_one(db).await.unwrap_or(0)
}

pub async fn count_audit_entries(db: &DbPool, app_id: Uuid, from: chrono::DateTime<chrono::Utc>, to: chrono::DateTime<chrono::Utc>) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM action_log WHERE resource_id = $1 AND created_at >= $2 AND created_at <= $3")
        .bind(crate::db::bind_id(app_id)).bind(from).bind(to).fetch_one(db).await.unwrap_or(0)
}

#[allow(clippy::type_complexity)]
pub async fn fetch_activity_transitions(
    db: &DbPool, app_id: Uuid, cursor: chrono::DateTime<chrono::Utc>, limit: i64,
) -> Result<Vec<(Uuid, String, String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, String, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT st.component_id, c.name, st.from_state, st.to_state, st.trigger, st.created_at
        FROM state_transitions st JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.created_at < $2 ORDER BY st.created_at DESC LIMIT $3"#,
    ).bind(crate::db::bind_id(app_id)).bind(cursor).bind(limit).fetch_all(db).await
}

#[allow(clippy::type_complexity)]
pub async fn fetch_activity_actions(
    db: &DbPool, app_id: Uuid, cursor: chrono::DateTime<chrono::Utc>, limit: i64,
) -> Result<Vec<(Uuid, String, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>(
        r#"SELECT al.user_id, COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action, al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al LEFT JOIN users u ON u.id = al.user_id
        WHERE al.resource_id = $1 AND al.created_at < $2 ORDER BY al.created_at DESC LIMIT $3"#,
    ).bind(crate::db::bind_id(app_id)).bind(cursor).bind(limit).fetch_all(db).await
}

#[allow(clippy::type_complexity)]
pub async fn fetch_activity_component_actions(
    db: &DbPool, app_id: Uuid, cursor: chrono::DateTime<chrono::Utc>, limit: i64,
) -> Result<Vec<(Uuid, String, String, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, String, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>(
        r#"SELECT al.user_id, COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action,
               COALESCE(c.name, CAST(al.resource_id AS TEXT)), al.details, al.created_at, al.status, al.error_message
        FROM action_log al LEFT JOIN users u ON u.id = al.user_id
        JOIN components c ON c.id = al.resource_id AND c.application_id = $1
        WHERE al.resource_type = 'component' AND al.created_at < $2 ORDER BY al.created_at DESC LIMIT $3"#,
    ).bind(crate::db::bind_id(app_id)).bind(cursor).bind(limit).fetch_all(db).await
}

#[allow(clippy::type_complexity)]
pub async fn fetch_activity_commands(
    db: &DbPool, app_id: Uuid, cursor: chrono::DateTime<chrono::Utc>, limit: i64,
) -> Result<Vec<(Uuid, Uuid, String, String, Option<i16>, Option<i32>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<i16>, Option<i32>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT ce.request_id, ce.component_id, c.name, ce.command_type,
               ce.exit_code, ce.duration_ms, ce.dispatched_at, ce.completed_at
        FROM command_executions ce JOIN components c ON c.id = ce.component_id AND c.application_id = $1
        WHERE ce.dispatched_at < $2 ORDER BY ce.dispatched_at DESC LIMIT $3"#,
    ).bind(crate::db::bind_id(app_id)).bind(cursor).bind(limit).fetch_all(db).await
}

pub async fn fetch_activity_switchovers(
    db: &DbPool, app_id: Uuid, cursor: chrono::DateTime<chrono::Utc>, limit: i64,
) -> Result<Vec<(DbUuid, String, String, Value, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, Value, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT sl.switchover_id, sl.phase, sl.status, sl.details, sl.created_at
        FROM switchover_log sl WHERE sl.application_id = $1 AND sl.created_at < $2
        ORDER BY sl.created_at DESC LIMIT $3"#,
    ).bind(crate::db::bind_id(app_id)).bind(cursor).bind(limit).fetch_all(db).await
}

pub async fn get_state_breakdown(db: &DbPool, app_id: Uuid) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (String, i64)>(
        r#"SELECT COALESCE(c.current_state, 'UNKNOWN'), COUNT(*) FROM components c
        WHERE c.application_id = $1 GROUP BY c.current_state ORDER BY COUNT(*) DESC"#,
    ).bind(crate::db::bind_id(app_id)).fetch_all(db).await
}

pub async fn get_error_components(db: &DbPool, app_id: Uuid) -> Result<Vec<(DbUuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT c.id, c.name, COALESCE(c.current_state, 'UNKNOWN'), c.updated_at FROM components c
        WHERE c.application_id = $1 AND c.current_state IN ('FAILED', 'UNREACHABLE', 'DEGRADED') ORDER BY c.updated_at DESC"#,
    ).bind(crate::db::bind_id(app_id)).fetch_all(db).await
}

pub async fn get_app_agents(db: &DbPool, app_id: Uuid) -> Result<Vec<(DbUuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT DISTINCT a.id, a.hostname, a.is_active, a.last_heartbeat_at FROM agents a
        JOIN components c ON c.agent_id = a.id WHERE c.application_id = $1 ORDER BY a.hostname"#,
    ).bind(crate::db::bind_id(app_id)).fetch_all(db).await
}

pub async fn get_recent_incidents(db: &DbPool, app_id: Uuid, limit: i64) -> Result<Vec<(DbUuid, String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT st.component_id, c.name, st.from_state, st.to_state, st.created_at
        FROM state_transitions st JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.to_state = 'FAILED' ORDER BY st.created_at DESC LIMIT $2"#,
    ).bind(crate::db::bind_id(app_id)).bind(limit).fetch_all(db).await
}

pub async fn count_components(db: &DbPool, app_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM components WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id)).fetch_one(db).await.unwrap_or(0)
}
