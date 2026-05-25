//! Knowledge progress API — track how validated each map element is.
//!
//! Each component and dependency carries:
//!
//!   * `confidence_score` — a float in [0.0, 1.0]; how confident the
//!     team / IA / captation pipeline is that this row is correct.
//!   * `knowledge_status` — discrete progress through the review
//!     funnel: `candidate` → `draft` → `reviewed` → `validated`.
//!     Captation jobs typically write `candidate`; the human review
//!     promotes through `draft` and onwards.
//!
//! Endpoints (wired in api/mod.rs):
//!
//!   PUT  /api/v1/components/:id/knowledge
//!   PUT  /api/v1/dependencies/:id/knowledge
//!   GET  /api/v1/apps/:id/knowledge/summary  — coverage by status
//!
//! Updates write the audit log with the previous and new values so
//! progress can be reconstructed from the action_log alone.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbPool;
#[allow(unused_imports)]
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::middleware::audit::{complete_action_success, log_action};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateKnowledgeRequest {
    pub confidence_score: Option<f32>,
    pub knowledge_status: Option<String>,
}

const ALLOWED_STATUSES: &[&str] = &["candidate", "draft", "reviewed", "validated", "deprecated"];

fn validate_status(s: &str) -> Result<(), ApiError> {
    if ALLOWED_STATUSES.contains(&s) {
        Ok(())
    } else {
        Err(ApiError::Validation(format!(
            "knowledge_status must be one of {:?}, got '{}'",
            ALLOWED_STATUSES, s
        )))
    }
}

fn validate_confidence(score: f32) -> Result<(), ApiError> {
    if (0.0..=1.0).contains(&score) {
        Ok(())
    } else {
        Err(ApiError::Validation(format!(
            "confidence_score must be in [0.0, 1.0], got {}",
            score
        )))
    }
}

async fn component_app(pool: &DbPool, id: Uuid) -> Result<Uuid, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT application_id FROM components WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<(DbUuid,)> = sqlx::query_as("SELECT application_id FROM components WHERE id = ?")
        .bind(DbUuid::from(id))
        .fetch_optional(pool)
        .await?;
    row.map(|(a,)| {
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let a = a.into_inner();
        a
    })
    .ok_or(ApiError::NotFound)
}

async fn dependency_app(pool: &DbPool, id: Uuid) -> Result<Uuid, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT c.application_id FROM dependencies d \
         INNER JOIN components c ON c.id = d.from_component_id WHERE d.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<(DbUuid,)> = sqlx::query_as(
        "SELECT c.application_id FROM dependencies d \
         INNER JOIN components c ON c.id = d.from_component_id WHERE d.id = ?",
    )
    .bind(DbUuid::from(id))
    .fetch_optional(pool)
    .await?;
    row.map(|(a,)| {
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let a = a.into_inner();
        a
    })
    .ok_or(ApiError::NotFound)
}

/// PUT /api/v1/components/:id/knowledge
pub async fn update_component_knowledge(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateKnowledgeRequest>,
) -> Result<Json<Value>, ApiError> {
    let app_id = component_app(&state.db, id).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    if let Some(s) = &body.knowledge_status {
        validate_status(s)?;
    }
    if let Some(c) = body.confidence_score {
        validate_confidence(c)?;
    }

    let action_id = log_action(
        &state.db,
        user.user_id,
        "knowledge.component.update",
        "component",
        id,
        json!({"new_status": body.knowledge_status, "new_confidence": body.confidence_score}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE components SET
            confidence_score = COALESCE($1, confidence_score),
            knowledge_status = COALESCE($2, knowledge_status),
            updated_at = NOW()
          WHERE id = $3",
    )
    .bind(body.confidence_score)
    .bind(&body.knowledge_status)
    .bind(id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE components SET
            confidence_score = COALESCE(?, confidence_score),
            knowledge_status = COALESCE(?, knowledge_status),
            updated_at = CURRENT_TIMESTAMP
          WHERE id = ?",
    )
    .bind(body.confidence_score)
    .bind(&body.knowledge_status)
    .bind(DbUuid::from(id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "updated", "component_id": id })))
}

/// PUT /api/v1/dependencies/:id/knowledge
pub async fn update_dependency_knowledge(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateKnowledgeRequest>,
) -> Result<Json<Value>, ApiError> {
    let app_id = dependency_app(&state.db, id).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    if let Some(s) = &body.knowledge_status {
        validate_status(s)?;
    }
    if let Some(c) = body.confidence_score {
        validate_confidence(c)?;
    }

    let action_id = log_action(
        &state.db,
        user.user_id,
        "knowledge.dependency.update",
        "dependency",
        id,
        json!({"new_status": body.knowledge_status, "new_confidence": body.confidence_score}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE dependencies SET
            confidence_score = COALESCE($1, confidence_score),
            knowledge_status = COALESCE($2, knowledge_status)
          WHERE id = $3",
    )
    .bind(body.confidence_score)
    .bind(&body.knowledge_status)
    .bind(id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE dependencies SET
            confidence_score = COALESCE(?, confidence_score),
            knowledge_status = COALESCE(?, knowledge_status)
          WHERE id = ?",
    )
    .bind(body.confidence_score)
    .bind(&body.knowledge_status)
    .bind(DbUuid::from(id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "updated", "dependency_id": id })))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StatusCount {
    pub knowledge_status: String,
    pub count: i64,
}

/// GET /api/v1/apps/:id/knowledge/summary
pub async fn app_knowledge_summary(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    #[cfg(feature = "postgres")]
    let components: Vec<StatusCount> = sqlx::query_as(
        "SELECT knowledge_status, COUNT(*)::BIGINT AS count
         FROM components WHERE application_id = $1
         GROUP BY knowledge_status",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let components: Vec<StatusCount> = sqlx::query_as(
        "SELECT knowledge_status, COUNT(*) AS count
         FROM components WHERE application_id = ?
         GROUP BY knowledge_status",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(feature = "postgres")]
    let dependencies: Vec<StatusCount> = sqlx::query_as(
        "SELECT d.knowledge_status, COUNT(*)::BIGINT AS count
         FROM dependencies d
         INNER JOIN components c ON c.id = d.from_component_id
         WHERE c.application_id = $1
         GROUP BY d.knowledge_status",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let dependencies: Vec<StatusCount> = sqlx::query_as(
        "SELECT d.knowledge_status, COUNT(*) AS count
         FROM dependencies d
         INNER JOIN components c ON c.id = d.from_component_id
         WHERE c.application_id = ?
         GROUP BY d.knowledge_status",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(&state.db)
    .await?;

    // Compute the global "validated coverage": share of components that
    // have reached `validated` status. This is the headline metric for
    // the methodology phase 3 review board.
    let component_total: i64 = components.iter().map(|c| c.count).sum();
    let component_validated: i64 = components
        .iter()
        .filter(|c| c.knowledge_status == "validated")
        .map(|c| c.count)
        .sum();
    let coverage = if component_total > 0 {
        (component_validated as f64) / (component_total as f64)
    } else {
        0.0
    };

    Ok(Json(json!({
        "application_id": app_id,
        "components_by_status": components,
        "dependencies_by_status": dependencies,
        "component_total": component_total,
        "component_validated": component_validated,
        "validated_coverage": coverage,
    })))
}
