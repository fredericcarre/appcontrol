use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::repository::misc_queries as group_repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: Option<i32>,
}

/// List all component groups for an application.
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let groups = group_repo::list_component_groups(&state.db, app_id).await?;

    Ok(Json(json!({ "groups": groups })))
}

/// Create a new component group.
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let group_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_group",
        "component_group",
        group_id,
        json!({"name": body.name, "app_id": app_id}),
    )
    .await?;

    let group = group_repo::create_component_group(
        &state.db,
        group_id,
        app_id,
        &body.name,
        body.description.as_deref(),
        body.color.as_deref().unwrap_or("#6366F1"),
        body.display_order.unwrap_or(0),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!(group))))
}

/// Update a component group.
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, group_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    if let Some(ref name) = body.name {
        validate_length("name", name, 1, 200)?;
    }
    validate_optional_length("description", &body.description, 2000)?;

    log_action(
        &state.db,
        user.user_id,
        "update_group",
        "component_group",
        group_id,
        json!({"app_id": app_id}),
    )
    .await?;

    let group = group_repo::update_component_group(
        &state.db,
        app_id,
        group_id,
        body.name.as_deref(),
        body.description.as_deref(),
        body.color.as_deref(),
        body.display_order,
    )
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(group)))
}

/// Delete a component group.
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, group_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_group",
        "component_group",
        group_id,
        json!({"app_id": app_id}),
    )
    .await?;

    if !group_repo::delete_component_group(&state.db, group_id, app_id).await? {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
