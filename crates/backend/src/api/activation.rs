//! Application activation level API.
//!
//! Exposes endpoints to read and update the graduated adoption ladder
//! described in `core/activation.rs`. The corresponding routes are wired
//! in `api/mod.rs`:
//!
//! * `GET  /api/v1/apps/:id/activation` — current level + name + description
//! * `PUT  /api/v1/apps/:id/activation` — change the level (requires manage
//!   permission on the application or org admin)

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
use crate::core::activation::{
    get_application_level, set_application_level, ActivationLevel,
};
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit::{complete_action_success, log_action};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ActivationStatus {
    pub level: i16,
    pub name: &'static str,
    pub description: &'static str,
    pub allows_checks: bool,
    pub allows_ops: bool,
    pub requires_pr_approval: bool,
}

impl From<ActivationLevel> for ActivationStatus {
    fn from(lvl: ActivationLevel) -> Self {
        ActivationStatus {
            level: lvl.as_i16(),
            name: lvl.name(),
            description: lvl.description(),
            allows_checks: lvl >= ActivationLevel::Diagnostic,
            allows_ops: lvl >= ActivationLevel::PrOnly,
            requires_pr_approval: lvl == ActivationLevel::PrOnly,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SetActivationRequest {
    pub level: i16,
}

/// GET /api/v1/apps/:id/activation
pub async fn get_activation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Any user with View permission on the app may read its activation level.
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let level = get_application_level(&state.db, id).await?;
    let status = ActivationStatus::from(level);

    Ok(Json(json!({
        "application_id": id,
        "activation": status,
        "available_levels": [
            ActivationStatus::from(ActivationLevel::Captation),
            ActivationStatus::from(ActivationLevel::Advisory),
            ActivationStatus::from(ActivationLevel::Diagnostic),
            ActivationStatus::from(ActivationLevel::PrOnly),
            ActivationStatus::from(ActivationLevel::Direct),
        ],
    })))
}

/// PUT /api/v1/apps/:id/activation
pub async fn set_activation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetActivationRequest>,
) -> Result<Json<Value>, ApiError> {
    // Changing the activation level is a governance act — requires Manage.
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let new_level = ActivationLevel::from_i16(body.level).map_err(|_| {
        ApiError::Validation(format!(
            "level must be between 0 and 4, got {}",
            body.level
        ))
    })?;

    // Audit BEFORE mutating — critical rule #3.
    let previous = get_application_level(&state.db, id).await?;
    let action_id = log_action(
        &state.db,
        *user.user_id,
        "activation.set",
        "application",
        id,
        json!({
            "previous_level": previous.as_i16(),
            "previous_name": previous.name(),
            "new_level": new_level.as_i16(),
            "new_name": new_level.name(),
        }),
    )
    .await?;

    set_application_level(&state.db, id, new_level).await?;
    complete_action_success(&state.db, action_id).await?;

    Ok(Json(json!({
        "application_id": id,
        "activation": ActivationStatus::from(new_level),
        "previous": ActivationStatus::from(previous),
    })))
}
