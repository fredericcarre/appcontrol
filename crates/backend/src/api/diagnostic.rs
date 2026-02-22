use axum::{
    extract::{Extension, Path, State},
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
pub struct RebuildRequest {
    pub component_ids: Option<Vec<Uuid>>,
    pub dry_run: Option<bool>,
}

pub async fn diagnose(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "diagnose",
        "application",
        app_id,
        json!({}),
    )
    .await?;

    let diagnosis = crate::core::diagnostic::diagnose_app(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({ "diagnosis": diagnosis })))
}

pub async fn rebuild(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<RebuildRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let dry_run = body.dry_run.unwrap_or(false);

    log_action(
        &state.db,
        user.user_id,
        "rebuild",
        "application",
        app_id,
        json!({"component_ids": body.component_ids, "dry_run": dry_run}),
    )
    .await?;

    let plan =
        crate::core::rebuild::build_rebuild_plan(&state.db, app_id, body.component_ids.as_deref())
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

    if dry_run {
        return Ok(Json(json!({ "dry_run": true, "plan": plan })));
    }

    Ok(Json(json!({ "status": "rebuilding", "plan": plan })))
}
