use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct StartSwitchoverRequest {
    pub target_site_id: Uuid,
    pub mode: String,                     // FULL, SELECTIVE, PROGRESSIVE
    pub component_ids: Option<Vec<Uuid>>, // for SELECTIVE mode
}

pub async fn start_switchover(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<StartSwitchoverRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Switchover is a runtime mutation — gated by activation level.
    let pr_sha = headers
        .get(crate::core::activation::PR_APPROVAL_HEADER)
        .and_then(|v| v.to_str().ok());
    crate::core::activation::check_runtime_ops_allowed(&state.db, app_id, pr_sha).await?;

    log_action(
        &state.db,
        user.user_id,
        "start_switchover",
        "application",
        app_id,
        json!({"target_site": body.target_site_id, "mode": body.mode}),
    )
    .await?;

    let switchover_id = crate::core::switchover::start_switchover(
        &state.db,
        app_id,
        body.target_site_id,
        &body.mode,
        body.component_ids,
        *user.user_id,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        json!({ "switchover_id": switchover_id, "phase": "PREPARE", "status": "in_progress" }),
    ))
}

pub async fn next_phase(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "switchover_next_phase",
        "application",
        app_id,
        json!({}),
    )
    .await?;

    let result = crate::core::switchover::advance_phase(&state, app_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    Ok(Json(json!(result)))
}

pub async fn rollback(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "switchover_rollback",
        "application",
        app_id,
        json!({}),
    )
    .await?;

    let result = crate::core::switchover::rollback(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    Ok(Json(json!(result)))
}

pub async fn commit(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "switchover_commit",
        "application",
        app_id,
        json!({}),
    )
    .await?;

    let result = crate::core::switchover::commit(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    Ok(Json(json!(result)))
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let result = crate::core::switchover::get_status(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!(result)))
}
