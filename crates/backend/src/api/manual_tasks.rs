//! Manual task components: pause-the-DAG checkpoints for actions that
//! happen outside AppControl (page on-call, click in F5, ask the DBA, …).
//!
//! Routes:
//! - GET  /components/:id/manual-task               — current pending validation + history
//! - POST /components/:id/manual-task/validate      — close the pending row with status + comment
//!
//! The sequencer side (see core/sequencer.rs) creates the pending row when
//! it tries to start a `component_type = 'manual_task'` component, then
//! polls until the operator submits a validation here.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::{ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct ValidateManualTaskRequest {
    /// 'validated' (default), 'skipped', or 'failed'. Skipping unblocks the
    /// DAG without claiming the underlying task succeeded; failed keeps the
    /// DAG paused and surfaces it as an FSM-level FAILED so the operator
    /// notices.
    #[serde(default = "default_validate_status")]
    pub status: String,
    pub comment: Option<String>,
}

fn default_validate_status() -> String {
    "validated".to_string()
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ManualTaskValidationRow {
    #[cfg(feature = "postgres")]
    pub id: Uuid,
    #[cfg(feature = "postgres")]
    pub component_id: Uuid,
    #[cfg(feature = "postgres")]
    pub application_id: Uuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub id: crate::db::DbUuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub component_id: crate::db::DbUuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub application_id: crate::db::DbUuid,
    pub started_at: DateTime<Utc>,
    #[cfg(feature = "postgres")]
    pub started_by: Option<Uuid>,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub started_by: Option<crate::db::DbUuid>,
    pub validated_at: Option<DateTime<Utc>>,
    #[cfg(feature = "postgres")]
    pub validated_by: Option<Uuid>,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub validated_by: Option<crate::db::DbUuid>,
    pub status: String,
    pub comment: Option<String>,
    pub duration_seconds: Option<i32>,
}

/// GET /api/v1/me/pending-manual-tasks
///
/// Returns every pending manual task in the user's org for which the user
/// has at least Operate permission on the parent app. Drives the dashboard
/// notification widget so operators see "you have N tasks waiting" without
/// browsing each app one by one.
pub async fn list_pending_for_user(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let all =
        crate::repository::manual_tasks::list_pending_for_org(&state.db, *user.organization_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter by app-level Operate permission. Sequential because per-app
    // permission resolution is small and N is the number of apps with
    // pending tasks (rare to exceed a handful at once); if it ever becomes
    // a hot path we can swap for a join in the SQL.
    let mut visible = Vec::with_capacity(all.len());
    let mut perm_cache: std::collections::HashMap<Uuid, PermissionLevel> =
        std::collections::HashMap::new();
    for task in all {
        let perm = match perm_cache.get(&task.application_id) {
            Some(p) => *p,
            None => {
                let p = effective_permission(
                    &state.db,
                    user.user_id,
                    task.application_id,
                    user.is_admin(),
                )
                .await;
                perm_cache.insert(task.application_id, p);
                p
            }
        };
        if perm >= PermissionLevel::Operate {
            visible.push(task);
        }
    }

    Ok(Json(json!({ "tasks": visible, "count": visible.len() })))
}

/// GET /api/v1/components/:id/manual-task
/// Returns the latest 20 validations + the description from the component.
pub async fn get_manual_task(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let component = state
        .component_repo
        .get_component(component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let history = crate::repository::manual_tasks::list_recent(&state.db, component_id, 20)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "component_id": component_id,
        "manual_description": component.manual_description,
        "history": history,
    })))
}

/// POST /api/v1/components/:id/manual-task/validate
/// Close the currently-pending validation row.
pub async fn validate_manual_task(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Json(body): Json<ValidateManualTaskRequest>,
) -> Result<Json<Value>, ApiError> {
    let component = state
        .component_repo
        .get_component(component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    // Operate is the right floor here: validating a manual task is an
    // operational decision, same level as Start/Stop.
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let status = body.status.as_str();
    if !matches!(status, "validated" | "skipped" | "failed") {
        return Err(ApiError::Validation(format!(
            "status must be one of 'validated', 'skipped', 'failed' (got '{}')",
            status
        )));
    }

    log_action(
        &state.db,
        user.user_id,
        "validate_manual_task",
        "component",
        component_id,
        json!({ "status": status, "has_comment": body.comment.is_some() }),
    )
    .await?;

    let updated = crate::repository::manual_tasks::close_pending(
        &state.db,
        component_id,
        *user.user_id,
        status,
        body.comment.as_deref(),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    if updated == 0 {
        return Err(ApiError::Conflict(
            "No pending validation for this component — start it first".to_string(),
        ));
    }

    Ok(Json(json!({
        "status": status,
        "validated_by": *user.user_id,
        "validated_at": Utc::now(),
    })))
}
