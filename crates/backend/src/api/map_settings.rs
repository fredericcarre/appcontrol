//! Per-application map display options.
//!
//! Lets operators tailor the visual density of the map for each application:
//! show/hide host name, metrics widget, site bindings, weather icon, cluster
//! badge, links, etc. Options are stored as JSON on the `applications` row
//! (`map_display_options` column added by V051) so the catalogue can grow
//! without further migrations.
//!
//! Routes:
//! - GET /apps/:id/map-settings — current options (defaults to "{}" if never
//!   customised).
//! - PUT /apps/:id/map-settings — replace options. Requires Edit on the app.
//!
//! Frontend treats absent keys as enabled, so an app with `{}` keeps full
//! visual fidelity. Operators flip flags off to declutter big maps.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

/// GET /api/v1/apps/:id/map-settings
pub async fn get_map_settings(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let opts = crate::repository::map_settings::get(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(Json(opts))
}

/// PUT /api/v1/apps/:id/map-settings
pub async fn put_map_settings(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    if !body.is_object() {
        return Err(ApiError::Validation(
            "map_display_options must be a JSON object".to_string(),
        ));
    }

    log_action(
        &state.db,
        user.user_id,
        "update_map_settings",
        "application",
        id,
        body.clone(),
    )
    .await?;

    crate::repository::map_settings::set(&state.db, id, &body)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(body))
}
