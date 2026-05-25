//! Map annotations API — free-form human commentary on map elements.
//!
//! Used during the human review phase (methodology §4.4), post-incident
//! debriefs, and architecture discussions. Annotations are NOT
//! operational: they don't drive any FSM and don't gate any operation.
//! They are visible to anyone with View permission on the application
//! the target belongs to.
//!
//! Targets: `application`, `component`, `dependency`.
//! Kinds: `note`, `review`, `todo`, `warning`.
//!
//! Routes (wired in api/mod.rs):
//!
//!   GET    /api/v1/annotations?target_type=component&target_id=<uuid>
//!   POST   /api/v1/annotations
//!   PUT    /api/v1/annotations/:id        (author only or admin)
//!   POST   /api/v1/annotations/:id/resolve
//!   DELETE /api/v1/annotations/:id        (author only or admin)

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::{DbJson, DbUuid};
use crate::error::ApiError;
use crate::middleware::audit::{complete_action_success, log_action};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ListAnnotationsQuery {
    pub target_type: String,
    pub target_id: Uuid,
    #[serde(default)]
    pub include_resolved: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAnnotationRequest {
    pub target_type: String,
    pub target_id: Uuid,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub body: String,
    #[serde(default)]
    pub metadata: Value,
}

fn default_kind() -> String {
    "note".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateAnnotationRequest {
    pub kind: Option<String>,
    pub body: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AnnotationRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub target_type: String,
    pub target_id: DbUuid,
    pub kind: String,
    pub body: String,
    pub metadata: DbJson,
    pub author_id: Option<DbUuid>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolved_by: Option<DbUuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn validate_target_type(t: &str) -> Result<(), ApiError> {
    match t {
        "application" | "component" | "dependency" => Ok(()),
        _ => Err(ApiError::Validation(format!(
            "target_type must be one of application | component | dependency, got '{}'",
            t
        ))),
    }
}

fn validate_kind(k: &str) -> Result<(), ApiError> {
    match k {
        "note" | "review" | "todo" | "warning" => Ok(()),
        _ => Err(ApiError::Validation(format!(
            "kind must be one of note | review | todo | warning, got '{}'",
            k
        ))),
    }
}

/// Resolve which application owns the target, so we can check permission.
async fn resolve_target_app(
    pool: &crate::db::DbPool,
    target_type: &str,
    target_id: Uuid,
) -> Result<Uuid, ApiError> {
    match target_type {
        "application" => Ok(target_id),
        "component" => {
            #[cfg(feature = "postgres")]
            let row: Option<(Uuid,)> =
                sqlx::query_as("SELECT application_id FROM components WHERE id = $1")
                    .bind(target_id)
                    .fetch_optional(pool)
                    .await?;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let row: Option<(DbUuid,)> =
                sqlx::query_as("SELECT application_id FROM components WHERE id = ?")
                    .bind(DbUuid::from(target_id))
                    .fetch_optional(pool)
                    .await?;
            row.map(|(id,)| {
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let id = id.into_inner();
                id
            })
            .ok_or(ApiError::NotFound)
        }
        "dependency" => {
            #[cfg(feature = "postgres")]
            let row: Option<(Uuid,)> = sqlx::query_as(
                "SELECT c.application_id FROM dependencies d
                 INNER JOIN components c ON c.id = d.from_component_id
                 WHERE d.id = $1",
            )
            .bind(target_id)
            .fetch_optional(pool)
            .await?;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let row: Option<(DbUuid,)> = sqlx::query_as(
                "SELECT c.application_id FROM dependencies d
                 INNER JOIN components c ON c.id = d.from_component_id
                 WHERE d.id = ?",
            )
            .bind(DbUuid::from(target_id))
            .fetch_optional(pool)
            .await?;
            row.map(|(id,)| {
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let id = id.into_inner();
                id
            })
            .ok_or(ApiError::NotFound)
        }
        _ => Err(ApiError::Validation("invalid target_type".to_string())),
    }
}

/// GET /api/v1/annotations?target_type=component&target_id=<uuid>
pub async fn list_annotations(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<ListAnnotationsQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_target_type(&q.target_type)?;
    let app_id = resolve_target_app(&state.db, &q.target_type, q.target_id).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let mut sql = String::from(
        "SELECT id, organization_id, target_type, target_id, kind, body, metadata, \
         author_id, resolved_at, resolved_by, created_at, updated_at \
         FROM map_annotations WHERE target_type = ",
    );
    #[cfg(feature = "postgres")]
    sql.push_str("$1 AND target_id = $2");
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sql.push_str("? AND target_id = ?");

    if !q.include_resolved {
        sql.push_str(" AND resolved_at IS NULL");
    }
    sql.push_str(" ORDER BY created_at DESC");

    #[cfg(feature = "postgres")]
    let rows: Vec<AnnotationRow> = sqlx::query_as(&sql)
        .bind(&q.target_type)
        .bind(q.target_id)
        .fetch_all(&state.db)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rows: Vec<AnnotationRow> = sqlx::query_as(&sql)
        .bind(&q.target_type)
        .bind(DbUuid::from(q.target_id))
        .fetch_all(&state.db)
        .await?;

    Ok(Json(json!({ "annotations": rows, "total": rows.len() })))
}

/// POST /api/v1/annotations
pub async fn create_annotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateAnnotationRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_target_type(&body.target_type)?;
    validate_kind(&body.kind)?;
    if body.body.trim().is_empty() {
        return Err(ApiError::Validation("body is required".to_string()));
    }

    let app_id = resolve_target_app(&state.db, &body.target_type, body.target_id).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let id = Uuid::new_v4();
    let org_id: Uuid = *user.organization_id;
    let metadata = if body.metadata.is_null() {
        Value::Object(Default::default())
    } else {
        body.metadata
    };

    let action_id = log_action(
        &state.db,
        user.user_id,
        "annotation.create",
        &body.target_type,
        body.target_id,
        json!({"kind": body.kind, "id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO map_annotations
            (id, organization_id, target_type, target_id, kind, body, metadata, author_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(org_id)
    .bind(&body.target_type)
    .bind(body.target_id)
    .bind(&body.kind)
    .bind(&body.body)
    .bind(&metadata)
    .bind(*user.user_id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO map_annotations
            (id, organization_id, target_type, target_id, kind, body, metadata, author_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(DbUuid::from(id))
    .bind(DbUuid::from(org_id))
    .bind(&body.target_type)
    .bind(DbUuid::from(body.target_id))
    .bind(&body.kind)
    .bind(&body.body)
    .bind(DbJson::from(metadata))
    .bind(DbUuid::from(*user.user_id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "created", "id": id })))
}

/// PUT /api/v1/annotations/:id
pub async fn update_annotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAnnotationRequest>,
) -> Result<Json<Value>, ApiError> {
    let existing = fetch_annotation(&state.db, id).await?;

    // Only the author or an admin may edit.
    let author = existing.author_id.map(|a| *a);
    if !user.is_admin() && author != Some(*user.user_id) {
        return Err(ApiError::Forbidden);
    }

    if let Some(k) = &body.kind {
        validate_kind(k)?;
    }

    let action_id = log_action(
        &state.db,
        user.user_id,
        "annotation.update",
        &existing.target_type,
        *existing.target_id,
        json!({"id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE map_annotations SET
            kind = COALESCE($1, kind),
            body = COALESCE($2, body),
            metadata = COALESCE($3, metadata),
            updated_at = NOW()
          WHERE id = $4",
    )
    .bind(&body.kind)
    .bind(&body.body)
    .bind(&body.metadata)
    .bind(id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE map_annotations SET
            kind = COALESCE(?, kind),
            body = COALESCE(?, body),
            metadata = COALESCE(?, metadata),
            updated_at = CURRENT_TIMESTAMP
          WHERE id = ?",
    )
    .bind(&body.kind)
    .bind(&body.body)
    .bind(body.metadata.map(DbJson::from))
    .bind(DbUuid::from(id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "updated", "id": id })))
}

/// POST /api/v1/annotations/:id/resolve
pub async fn resolve_annotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let existing = fetch_annotation(&state.db, id).await?;
    let app_id = resolve_target_app(&state.db, &existing.target_type, *existing.target_id).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let action_id = log_action(
        &state.db,
        user.user_id,
        "annotation.resolve",
        &existing.target_type,
        *existing.target_id,
        json!({"id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE map_annotations SET resolved_at = NOW(), resolved_by = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(*user.user_id)
    .bind(id)
    .execute(&state.db)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE map_annotations SET resolved_at = CURRENT_TIMESTAMP, resolved_by = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(DbUuid::from(*user.user_id))
    .bind(DbUuid::from(id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "resolved", "id": id })))
}

/// DELETE /api/v1/annotations/:id
pub async fn delete_annotation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let existing = fetch_annotation(&state.db, id).await?;
    let author = existing.author_id.map(|a| *a);
    if !user.is_admin() && author != Some(*user.user_id) {
        return Err(ApiError::Forbidden);
    }

    let action_id = log_action(
        &state.db,
        user.user_id,
        "annotation.delete",
        &existing.target_type,
        *existing.target_id,
        json!({"id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query("DELETE FROM map_annotations WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("DELETE FROM map_annotations WHERE id = ?")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "deleted", "id": id })))
}

async fn fetch_annotation(
    pool: &crate::db::DbPool,
    id: Uuid,
) -> Result<AnnotationRow, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<AnnotationRow> = sqlx::query_as(
        "SELECT id, organization_id, target_type, target_id, kind, body, metadata, \
         author_id, resolved_at, resolved_by, created_at, updated_at \
         FROM map_annotations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<AnnotationRow> = sqlx::query_as(
        "SELECT id, organization_id, target_type, target_id, kind, body, metadata, \
         author_id, resolved_at, resolved_by, created_at, updated_at \
         FROM map_annotations WHERE id = ?",
    )
    .bind(DbUuid::from(id))
    .fetch_optional(pool)
    .await?;

    row.ok_or(ApiError::NotFound)
}
