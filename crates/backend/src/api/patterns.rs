//! Pattern templates library — Phase 5 transversal capitalisation.
//!
//! Stores reusable command / check templates by technology
//! (`spring-boot`, `postgres`, `kafka`…). Each pattern is org-scoped
//! and may carry a back-reference to the incident that motivated it.
//!
//! Routes (wired in `api/mod.rs`):
//!
//!   GET    /api/v1/patterns                      — list patterns of caller's org
//!   POST   /api/v1/patterns                      — create a pattern (Edit perm)
//!   GET    /api/v1/patterns/:id                  — single pattern
//!   PUT    /api/v1/patterns/:id                  — update (Edit perm)
//!   DELETE /api/v1/patterns/:id                  — delete (Edit perm)
//!   POST   /api/v1/patterns/:id/applied          — bump usage_count when a
//!                                                  component adopts the pattern

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::{DbJson, DbUuid};
use crate::error::ApiError;
use crate::middleware::audit::{complete_action_success, log_action};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ListPatternsQuery {
    pub technology: Option<String>,
    #[serde(default)]
    pub enabled_only: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreatePatternRequest {
    pub name: String,
    pub technology: String,
    pub description: Option<String>,
    pub check_cmd_template: Option<String>,
    pub integrity_check_cmd_template: Option<String>,
    pub infra_check_cmd_template: Option<String>,
    pub start_cmd_template: Option<String>,
    pub stop_cmd_template: Option<String>,
    pub rebuild_cmd_template: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_from_incident_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePatternRequest {
    pub name: Option<String>,
    pub technology: Option<String>,
    pub description: Option<String>,
    pub check_cmd_template: Option<String>,
    pub integrity_check_cmd_template: Option<String>,
    pub infra_check_cmd_template: Option<String>,
    pub start_cmd_template: Option<String>,
    pub stop_cmd_template: Option<String>,
    pub rebuild_cmd_template: Option<String>,
    pub tags: Option<Vec<String>>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PatternRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub technology: String,
    pub description: Option<String>,
    pub check_cmd_template: Option<String>,
    pub integrity_check_cmd_template: Option<String>,
    pub infra_check_cmd_template: Option<String>,
    pub start_cmd_template: Option<String>,
    pub stop_cmd_template: Option<String>,
    pub rebuild_cmd_template: Option<String>,
    pub tags: DbJson,
    pub created_from_incident_id: Option<DbUuid>,
    pub is_enabled: bool,
    pub usage_count: i32,
    pub created_by: Option<DbUuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/patterns
pub async fn list_patterns(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(params): Query<ListPatternsQuery>,
) -> Result<Json<Value>, ApiError> {
    let org_id: Uuid = *user.organization_id;

    let mut sql = String::from(
        "SELECT id, organization_id, name, technology, description, \
         check_cmd_template, integrity_check_cmd_template, infra_check_cmd_template, \
         start_cmd_template, stop_cmd_template, rebuild_cmd_template, \
         tags, created_from_incident_id, is_enabled, usage_count, \
         created_by, created_at, updated_at \
         FROM pattern_templates WHERE organization_id = ",
    );
    #[cfg(feature = "postgres")]
    sql.push_str("$1");
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sql.push('?');

    if params.technology.is_some() {
        #[cfg(feature = "postgres")]
        sql.push_str(" AND technology = $2");
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        sql.push_str(" AND technology = ?");
    }

    if params.enabled_only {
        sql.push_str(" AND is_enabled = TRUE");
    }

    sql.push_str(" ORDER BY usage_count DESC, name ASC");

    let mut query = sqlx::query_as::<_, PatternRow>(&sql);

    #[cfg(feature = "postgres")]
    {
        query = query.bind(org_id);
        if let Some(tech) = &params.technology {
            query = query.bind(tech);
        }
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        query = query.bind(DbUuid::from(org_id));
        if let Some(tech) = &params.technology {
            query = query.bind(tech);
        }
    }

    let rows = query.fetch_all(&state.db).await?;
    Ok(Json(json!({ "patterns": rows, "total": rows.len() })))
}

/// POST /api/v1/patterns
pub async fn create_pattern(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreatePatternRequest>,
) -> Result<Json<Value>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::Validation("name is required".into()));
    }
    if body.technology.trim().is_empty() {
        return Err(ApiError::Validation("technology is required".into()));
    }

    let id = Uuid::new_v4();
    let org_id: Uuid = *user.organization_id;
    let user_id: Uuid = *user.user_id;
    let tags_value = serde_json::Value::Array(
        body.tags.iter().cloned().map(serde_json::Value::String).collect(),
    );

    let action_id = log_action(
        &state.db,
        user.user_id,
        "patterns.create",
        "pattern_template",
        id,
        json!({"name": body.name, "technology": body.technology}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO pattern_templates
            (id, organization_id, name, technology, description,
             check_cmd_template, integrity_check_cmd_template, infra_check_cmd_template,
             start_cmd_template, stop_cmd_template, rebuild_cmd_template,
             tags, created_from_incident_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(id)
    .bind(org_id)
    .bind(&body.name)
    .bind(&body.technology)
    .bind(&body.description)
    .bind(&body.check_cmd_template)
    .bind(&body.integrity_check_cmd_template)
    .bind(&body.infra_check_cmd_template)
    .bind(&body.start_cmd_template)
    .bind(&body.stop_cmd_template)
    .bind(&body.rebuild_cmd_template)
    .bind(&tags_value)
    .bind(body.created_from_incident_id)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO pattern_templates
            (id, organization_id, name, technology, description,
             check_cmd_template, integrity_check_cmd_template, infra_check_cmd_template,
             start_cmd_template, stop_cmd_template, rebuild_cmd_template,
             tags, created_from_incident_id, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(DbUuid::from(id))
    .bind(DbUuid::from(org_id))
    .bind(&body.name)
    .bind(&body.technology)
    .bind(&body.description)
    .bind(&body.check_cmd_template)
    .bind(&body.integrity_check_cmd_template)
    .bind(&body.infra_check_cmd_template)
    .bind(&body.start_cmd_template)
    .bind(&body.stop_cmd_template)
    .bind(&body.rebuild_cmd_template)
    .bind(DbJson::from(tags_value))
    .bind(body.created_from_incident_id.map(DbUuid::from))
    .bind(DbUuid::from(user_id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "created", "id": id })))
}

/// GET /api/v1/patterns/:id
pub async fn get_pattern(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let row = fetch_pattern(&state.db, *user.organization_id, id).await?;
    Ok(Json(json!({ "pattern": row })))
}

/// PUT /api/v1/patterns/:id
pub async fn update_pattern(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePatternRequest>,
) -> Result<Json<Value>, ApiError> {
    // Verify ownership before applying any change.
    fetch_pattern(&state.db, *user.organization_id, id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "patterns.update",
        "pattern_template",
        id,
        json!({"id": id}),
    )
    .await?;

    let tags_value = body
        .tags
        .as_ref()
        .map(|t| serde_json::Value::Array(t.iter().cloned().map(serde_json::Value::String).collect()));

    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE pattern_templates SET
            name = COALESCE($1, name),
            technology = COALESCE($2, technology),
            description = COALESCE($3, description),
            check_cmd_template = COALESCE($4, check_cmd_template),
            integrity_check_cmd_template = COALESCE($5, integrity_check_cmd_template),
            infra_check_cmd_template = COALESCE($6, infra_check_cmd_template),
            start_cmd_template = COALESCE($7, start_cmd_template),
            stop_cmd_template = COALESCE($8, stop_cmd_template),
            rebuild_cmd_template = COALESCE($9, rebuild_cmd_template),
            tags = COALESCE($10, tags),
            is_enabled = COALESCE($11, is_enabled),
            updated_at = NOW()
          WHERE id = $12",
    )
    .bind(&body.name)
    .bind(&body.technology)
    .bind(&body.description)
    .bind(&body.check_cmd_template)
    .bind(&body.integrity_check_cmd_template)
    .bind(&body.infra_check_cmd_template)
    .bind(&body.start_cmd_template)
    .bind(&body.stop_cmd_template)
    .bind(&body.rebuild_cmd_template)
    .bind(&tags_value)
    .bind(body.is_enabled)
    .bind(id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE pattern_templates SET
            name = COALESCE(?, name),
            technology = COALESCE(?, technology),
            description = COALESCE(?, description),
            check_cmd_template = COALESCE(?, check_cmd_template),
            integrity_check_cmd_template = COALESCE(?, integrity_check_cmd_template),
            infra_check_cmd_template = COALESCE(?, infra_check_cmd_template),
            start_cmd_template = COALESCE(?, start_cmd_template),
            stop_cmd_template = COALESCE(?, stop_cmd_template),
            rebuild_cmd_template = COALESCE(?, rebuild_cmd_template),
            tags = COALESCE(?, tags),
            is_enabled = COALESCE(?, is_enabled),
            updated_at = CURRENT_TIMESTAMP
          WHERE id = ?",
    )
    .bind(&body.name)
    .bind(&body.technology)
    .bind(&body.description)
    .bind(&body.check_cmd_template)
    .bind(&body.integrity_check_cmd_template)
    .bind(&body.infra_check_cmd_template)
    .bind(&body.start_cmd_template)
    .bind(&body.stop_cmd_template)
    .bind(&body.rebuild_cmd_template)
    .bind(tags_value.map(DbJson::from))
    .bind(body.is_enabled)
    .bind(DbUuid::from(id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "updated", "id": id })))
}

/// DELETE /api/v1/patterns/:id
pub async fn delete_pattern(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    fetch_pattern(&state.db, *user.organization_id, id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "patterns.delete",
        "pattern_template",
        id,
        json!({"id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query("DELETE FROM pattern_templates WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("DELETE FROM pattern_templates WHERE id = ?")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "deleted", "id": id })))
}

/// GET /api/v1/patterns/:id/candidates
///
/// Returns the list of components in the caller's organisation that
/// could benefit from this pattern but do NOT yet have its checks
/// configured. Matching heuristic: same `technology` tag (case-
/// insensitive substring on `components.component_type`).
///
/// Use this to power the "propagate to similar apps" workflow
/// described in methodology §5.5.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PatternCandidate {
    pub component_id: DbUuid,
    pub component_name: String,
    pub application_id: DbUuid,
    pub application_name: String,
    pub component_type: String,
    pub has_check_cmd: bool,
}

pub async fn pattern_candidates(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let pattern = fetch_pattern(&state.db, *user.organization_id, id).await?;
    let tech = format!("%{}%", pattern.technology.to_lowercase());

    #[cfg(feature = "postgres")]
    let rows: Vec<PatternCandidate> = sqlx::query_as(
        "SELECT c.id AS component_id, c.name AS component_name,
                a.id AS application_id, a.name AS application_name,
                c.component_type,
                (c.check_cmd IS NOT NULL) AS has_check_cmd
         FROM components c
         INNER JOIN applications a ON a.id = c.application_id
         WHERE a.organization_id = $1
           AND LOWER(c.component_type) LIKE $2
           AND c.check_cmd IS NULL
         ORDER BY a.name, c.name
         LIMIT 200",
    )
    .bind(*user.organization_id)
    .bind(tech)
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rows: Vec<PatternCandidate> = sqlx::query_as(
        "SELECT c.id AS component_id, c.name AS component_name,
                a.id AS application_id, a.name AS application_name,
                c.component_type,
                (c.check_cmd IS NOT NULL) AS has_check_cmd
         FROM components c
         INNER JOIN applications a ON a.id = c.application_id
         WHERE a.organization_id = ?
           AND LOWER(c.component_type) LIKE ?
           AND c.check_cmd IS NULL
         ORDER BY a.name, c.name
         LIMIT 200",
    )
    .bind(DbUuid::from(*user.organization_id))
    .bind(tech)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({
        "pattern_id": id,
        "technology": pattern.technology,
        "candidates": rows,
        "total": rows.len(),
    })))
}

/// POST /api/v1/patterns/:id/propagate
/// Body: { "component_ids": ["...", "..."] }
///
/// Applies the pattern's templates to each listed component (only
/// fills empty fields — won't overwrite existing commands). Returns
/// per-component results. Bumps the pattern's usage_count by N.
#[derive(Debug, Deserialize)]
pub struct PropagateRequest {
    pub component_ids: Vec<Uuid>,
}

pub async fn pattern_propagate(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<PropagateRequest>,
) -> Result<Json<Value>, ApiError> {
    let pattern = fetch_pattern(&state.db, *user.organization_id, id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "patterns.propagate",
        "pattern_template",
        id,
        json!({"component_count": body.component_ids.len()}),
    )
    .await?;

    let mut updated = 0_usize;
    let mut skipped = 0_usize;
    for component_id in &body.component_ids {
        let n = propagate_one(&state.db, *user.organization_id, *component_id, &pattern).await?;
        if n > 0 {
            updated += 1;
        } else {
            skipped += 1;
        }
    }
    if updated > 0 {
        bump_usage(&state.db, id, updated as i32).await?;
    }

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({
        "status": "ok",
        "pattern_id": id,
        "updated": updated,
        "skipped": skipped,
    })))
}

async fn propagate_one(
    pool: &crate::db::DbPool,
    org_id: Uuid,
    component_id: Uuid,
    pattern: &PatternRow,
) -> Result<u64, ApiError> {
    // Verify the component belongs to the caller's org via its app.
    #[cfg(feature = "postgres")]
    let owner: Option<(Uuid,)> = sqlx::query_as(
        "SELECT a.organization_id FROM components c
         INNER JOIN applications a ON a.id = c.application_id
         WHERE c.id = $1",
    )
    .bind(component_id)
    .fetch_optional(pool)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let owner: Option<(DbUuid,)> = sqlx::query_as(
        "SELECT a.organization_id FROM components c
         INNER JOIN applications a ON a.id = c.application_id
         WHERE c.id = ?",
    )
    .bind(DbUuid::from(component_id))
    .fetch_optional(pool)
    .await?;
    let owner_org = owner.map(|(id,)| {
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let id = id.into_inner();
        id
    });
    if owner_org != Some(org_id) {
        return Ok(0);
    }

    #[cfg(feature = "postgres")]
    let affected = sqlx::query(
        "UPDATE components SET
            check_cmd = COALESCE(check_cmd, $1),
            integrity_check_cmd = COALESCE(integrity_check_cmd, $2),
            infra_check_cmd = COALESCE(infra_check_cmd, $3),
            start_cmd = COALESCE(start_cmd, $4),
            stop_cmd = COALESCE(stop_cmd, $5),
            rebuild_cmd = COALESCE(rebuild_cmd, $6),
            updated_at = NOW()
          WHERE id = $7
            AND (check_cmd IS NULL OR integrity_check_cmd IS NULL OR infra_check_cmd IS NULL
                 OR start_cmd IS NULL OR stop_cmd IS NULL OR rebuild_cmd IS NULL)",
    )
    .bind(&pattern.check_cmd_template)
    .bind(&pattern.integrity_check_cmd_template)
    .bind(&pattern.infra_check_cmd_template)
    .bind(&pattern.start_cmd_template)
    .bind(&pattern.stop_cmd_template)
    .bind(&pattern.rebuild_cmd_template)
    .bind(component_id)
    .execute(pool)
    .await?
    .rows_affected();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let affected = sqlx::query(
        "UPDATE components SET
            check_cmd = COALESCE(check_cmd, ?),
            integrity_check_cmd = COALESCE(integrity_check_cmd, ?),
            infra_check_cmd = COALESCE(infra_check_cmd, ?),
            start_cmd = COALESCE(start_cmd, ?),
            stop_cmd = COALESCE(stop_cmd, ?),
            rebuild_cmd = COALESCE(rebuild_cmd, ?),
            updated_at = CURRENT_TIMESTAMP
          WHERE id = ?
            AND (check_cmd IS NULL OR integrity_check_cmd IS NULL OR infra_check_cmd IS NULL
                 OR start_cmd IS NULL OR stop_cmd IS NULL OR rebuild_cmd IS NULL)",
    )
    .bind(&pattern.check_cmd_template)
    .bind(&pattern.integrity_check_cmd_template)
    .bind(&pattern.infra_check_cmd_template)
    .bind(&pattern.start_cmd_template)
    .bind(&pattern.stop_cmd_template)
    .bind(&pattern.rebuild_cmd_template)
    .bind(DbUuid::from(component_id))
    .execute(pool)
    .await?
    .rows_affected();

    Ok(affected)
}

async fn bump_usage(pool: &crate::db::DbPool, id: Uuid, n: i32) -> Result<(), ApiError> {
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE pattern_templates SET usage_count = usage_count + $1 WHERE id = $2")
        .bind(n)
        .bind(id)
        .execute(pool)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE pattern_templates SET usage_count = usage_count + ? WHERE id = ?")
        .bind(n)
        .bind(DbUuid::from(id))
        .execute(pool)
        .await?;
    Ok(())
}

/// POST /api/v1/patterns/:id/applied — increment usage_count when a
/// component adopts the pattern. Stateless on purpose: the callers
/// (component update handlers, IA suggestions, scripts) are
/// responsible for tying the pattern to the actual component.
pub async fn pattern_applied(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    fetch_pattern(&state.db, *user.organization_id, id).await?;

    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE pattern_templates SET usage_count = usage_count + 1 WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE pattern_templates SET usage_count = usage_count + 1 WHERE id = ?")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    Ok(Json(json!({ "status": "applied", "id": id })))
}

async fn fetch_pattern(
    pool: &crate::db::DbPool,
    org_id: Uuid,
    id: Uuid,
) -> Result<PatternRow, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<PatternRow> = sqlx::query_as(
        "SELECT id, organization_id, name, technology, description, \
         check_cmd_template, integrity_check_cmd_template, infra_check_cmd_template, \
         start_cmd_template, stop_cmd_template, rebuild_cmd_template, \
         tags, created_from_incident_id, is_enabled, usage_count, \
         created_by, created_at, updated_at \
         FROM pattern_templates WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<PatternRow> = sqlx::query_as(
        "SELECT id, organization_id, name, technology, description, \
         check_cmd_template, integrity_check_cmd_template, infra_check_cmd_template, \
         start_cmd_template, stop_cmd_template, rebuild_cmd_template, \
         tags, created_from_incident_id, is_enabled, usage_count, \
         created_by, created_at, updated_at \
         FROM pattern_templates WHERE id = ? AND organization_id = ?",
    )
    .bind(DbUuid::from(id))
    .bind(DbUuid::from(org_id))
    .fetch_optional(pool)
    .await?;

    row.ok_or(ApiError::NotFound)
}
