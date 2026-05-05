//! Read/write `manual_task_validations` (V054).
//!
//! Two queries used at runtime:
//!  * `open_pending` — sequencer creates a row when starting a manual_task
//!    component. Returns the row id (used in audit + plumbing).
//!  * `close_pending` — frontend Validate / Skip closes it.
//!  * `latest_status` — sequencer polls until a row is non-pending.
//!  * `list_recent` — DetailPanel history view.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::DbPool;

/// One historical row from `manual_task_validations`. Mirrors the SQL columns
/// directly. We keep a single backend-side struct (no DbUuid wrapping) by
/// converting once at the SQL layer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ManualTaskValidation {
    pub id: Uuid,
    pub component_id: Uuid,
    pub application_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub started_by: Option<Uuid>,
    pub validated_at: Option<DateTime<Utc>>,
    pub validated_by: Option<Uuid>,
    pub status: String,
    pub comment: Option<String>,
    pub duration_seconds: Option<i32>,
}

#[cfg(feature = "postgres")]
pub async fn open_pending(
    pool: &DbPool,
    component_id: Uuid,
    application_id: Uuid,
    started_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    // If there's already a pending row for this component, reuse it instead
    // of creating duplicates — cancel-and-retry shouldn't pile rows up.
    if let Some((existing,)) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM manual_task_validations WHERE component_id = $1 AND status = 'pending' LIMIT 1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await?
    {
        return Ok(existing);
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO manual_task_validations (id, component_id, application_id, started_by, status) \
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(crate::db::bind_id(id))
    .bind(crate::db::bind_id(component_id))
    .bind(crate::db::bind_id(application_id))
    .bind(crate::db::bind_id(started_by))
    .execute(pool)
    .await?;
    Ok(id)
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn open_pending(
    pool: &DbPool,
    component_id: Uuid,
    application_id: Uuid,
    started_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    use crate::db::DbUuid;
    if let Some((existing,)) = sqlx::query_as::<_, (DbUuid,)>(
        "SELECT id FROM manual_task_validations WHERE component_id = $1 AND status = 'pending' LIMIT 1",
    )
    .bind(DbUuid::from(component_id))
    .fetch_optional(pool)
    .await?
    {
        return Ok(existing.into_inner());
    }
    let id = DbUuid::new_v4();
    let plain_id = id.into_inner();
    sqlx::query(
        "INSERT INTO manual_task_validations (id, component_id, application_id, started_by, status) \
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(id)
    .bind(DbUuid::from(component_id))
    .bind(DbUuid::from(application_id))
    .bind(DbUuid::from(started_by))
    .execute(pool)
    .await?;
    Ok(plain_id)
}

/// Close the currently-pending row. Returns rows_affected so the API can
/// distinguish "I validated something" from "nothing was pending".
#[cfg(feature = "postgres")]
pub async fn close_pending(
    pool: &DbPool,
    component_id: Uuid,
    validated_by: Uuid,
    status: &str,
    comment: Option<&str>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE manual_task_validations \
         SET validated_at = now(), \
             validated_by = $2, \
             status = $3, \
             comment = $4, \
             duration_seconds = EXTRACT(EPOCH FROM (now() - started_at))::int \
         WHERE component_id = $1 AND status = 'pending'",
    )
    .bind(crate::db::bind_id(component_id))
    .bind(crate::db::bind_id(validated_by))
    .bind(status)
    .bind(comment)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn close_pending(
    pool: &DbPool,
    component_id: Uuid,
    validated_by: Uuid,
    status: &str,
    comment: Option<&str>,
) -> Result<u64, sqlx::Error> {
    use crate::db::DbUuid;
    let result = sqlx::query(
        "UPDATE manual_task_validations \
         SET validated_at = datetime('now'), \
             validated_by = $2, \
             status = $3, \
             comment = $4, \
             duration_seconds = CAST((julianday('now') - julianday(started_at)) * 86400 AS INTEGER) \
         WHERE component_id = $1 AND status = 'pending'",
    )
    .bind(DbUuid::from(component_id))
    .bind(DbUuid::from(validated_by))
    .bind(status)
    .bind(comment)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Sequencer-side polling helper: returns the current status of the
/// most-recent pending row (or None if none exists). Used to decide whether
/// to keep waiting, advance the FSM, or fail.
#[cfg(feature = "postgres")]
pub async fn latest_pending_status(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM manual_task_validations \
         WHERE component_id = $1 \
         ORDER BY started_at DESC LIMIT 1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn latest_pending_status(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    use crate::db::DbUuid;
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM manual_task_validations \
         WHERE component_id = $1 \
         ORDER BY started_at DESC LIMIT 1",
    )
    .bind(DbUuid::from(component_id))
    .fetch_optional(pool)
    .await
}

#[cfg(feature = "postgres")]
pub async fn list_recent(
    pool: &DbPool,
    component_id: Uuid,
    limit: i64,
) -> Result<Vec<ManualTaskValidation>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        component_id: Uuid,
        application_id: Uuid,
        started_at: DateTime<Utc>,
        started_by: Option<Uuid>,
        validated_at: Option<DateTime<Utc>>,
        validated_by: Option<Uuid>,
        status: String,
        comment: Option<String>,
        duration_seconds: Option<i32>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, component_id, application_id, started_at, started_by, \
                validated_at, validated_by, status, comment, duration_seconds \
         FROM manual_task_validations \
         WHERE component_id = $1 \
         ORDER BY started_at DESC \
         LIMIT $2",
    )
    .bind(crate::db::bind_id(component_id))
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ManualTaskValidation {
            id: r.id,
            component_id: r.component_id,
            application_id: r.application_id,
            started_at: r.started_at,
            started_by: r.started_by,
            validated_at: r.validated_at,
            validated_by: r.validated_by,
            status: r.status,
            comment: r.comment,
            duration_seconds: r.duration_seconds,
        })
        .collect())
}

/// One pending manual task across the whole org, enriched with the
/// component / app names so the dashboard widget can render them without
/// extra round-trips.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingTaskListing {
    pub validation_id: Uuid,
    pub component_id: Uuid,
    pub component_name: String,
    pub component_display_name: Option<String>,
    pub application_id: Uuid,
    pub application_name: String,
    pub started_at: DateTime<Utc>,
    pub manual_description: Option<String>,
}

/// List every currently-pending manual task within the user's org, joined
/// with component + app names. The caller is expected to additionally
/// filter by per-app permission (see `effective_permission`) — we do the
/// org-level scoping here in SQL so unrelated tenants are never visible.
#[cfg(feature = "postgres")]
pub async fn list_pending_for_org(
    pool: &DbPool,
    organization_id: Uuid,
) -> Result<Vec<PendingTaskListing>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        validation_id: Uuid,
        component_id: Uuid,
        component_name: String,
        component_display_name: Option<String>,
        application_id: Uuid,
        application_name: String,
        started_at: DateTime<Utc>,
        manual_description: Option<String>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT mtv.id   AS validation_id, \
                c.id     AS component_id, \
                c.name   AS component_name, \
                c.display_name AS component_display_name, \
                a.id     AS application_id, \
                a.name   AS application_name, \
                mtv.started_at, \
                c.manual_description \
         FROM manual_task_validations mtv \
         JOIN components   c ON c.id = mtv.component_id \
         JOIN applications a ON a.id = c.application_id \
         WHERE mtv.status = 'pending' \
           AND a.organization_id = $1 \
         ORDER BY mtv.started_at",
    )
    .bind(crate::db::bind_id(organization_id))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PendingTaskListing {
            validation_id: r.validation_id,
            component_id: r.component_id,
            component_name: r.component_name,
            component_display_name: r.component_display_name,
            application_id: r.application_id,
            application_name: r.application_name,
            started_at: r.started_at,
            manual_description: r.manual_description,
        })
        .collect())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn list_pending_for_org(
    pool: &DbPool,
    organization_id: Uuid,
) -> Result<Vec<PendingTaskListing>, sqlx::Error> {
    use crate::db::DbUuid;
    #[derive(sqlx::FromRow)]
    struct Row {
        validation_id: DbUuid,
        component_id: DbUuid,
        component_name: String,
        component_display_name: Option<String>,
        application_id: DbUuid,
        application_name: String,
        started_at: DateTime<Utc>,
        manual_description: Option<String>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT mtv.id   AS validation_id, \
                c.id     AS component_id, \
                c.name   AS component_name, \
                c.display_name AS component_display_name, \
                a.id     AS application_id, \
                a.name   AS application_name, \
                mtv.started_at, \
                c.manual_description \
         FROM manual_task_validations mtv \
         JOIN components   c ON c.id = mtv.component_id \
         JOIN applications a ON a.id = c.application_id \
         WHERE mtv.status = 'pending' \
           AND a.organization_id = $1 \
         ORDER BY mtv.started_at",
    )
    .bind(DbUuid::from(organization_id))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PendingTaskListing {
            validation_id: r.validation_id.into_inner(),
            component_id: r.component_id.into_inner(),
            component_name: r.component_name,
            component_display_name: r.component_display_name,
            application_id: r.application_id.into_inner(),
            application_name: r.application_name,
            started_at: r.started_at,
            manual_description: r.manual_description,
        })
        .collect())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn list_recent(
    pool: &DbPool,
    component_id: Uuid,
    limit: i64,
) -> Result<Vec<ManualTaskValidation>, sqlx::Error> {
    use crate::db::DbUuid;
    #[derive(sqlx::FromRow)]
    struct Row {
        id: DbUuid,
        component_id: DbUuid,
        application_id: DbUuid,
        started_at: DateTime<Utc>,
        started_by: Option<DbUuid>,
        validated_at: Option<DateTime<Utc>>,
        validated_by: Option<DbUuid>,
        status: String,
        comment: Option<String>,
        duration_seconds: Option<i32>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, component_id, application_id, started_at, started_by, \
                validated_at, validated_by, status, comment, duration_seconds \
         FROM manual_task_validations \
         WHERE component_id = $1 \
         ORDER BY started_at DESC \
         LIMIT $2",
    )
    .bind(DbUuid::from(component_id))
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ManualTaskValidation {
            id: r.id.into_inner(),
            component_id: r.component_id.into_inner(),
            application_id: r.application_id.into_inner(),
            started_at: r.started_at,
            started_by: r.started_by.map(|u| u.into_inner()),
            validated_at: r.validated_at,
            validated_by: r.validated_by.map(|u| u.into_inner()),
            status: r.status,
            comment: r.comment,
            duration_seconds: r.duration_seconds,
        })
        .collect())
}
