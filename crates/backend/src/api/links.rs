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
use crate::error::{validate_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::repository::misc_queries as link_repo;
use crate::repository::queries as repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct CreateLinkRequest {
    pub label: String,
    pub url: String,
    pub link_type: Option<String>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLinkRequest {
    pub label: Option<String>,
    pub url: Option<String>,
    pub link_type: Option<String>,
    pub display_order: Option<i32>,
}

/// List all links for a component.
pub async fn list_links(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id = repo::get_component_app_id(&state.db, component_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let links = link_repo::list_component_links(&state.db, component_id).await?;

    Ok(Json(json!({ "links": links })))
}

/// Create a new component link.
pub async fn create_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Json(body): Json<CreateLinkRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let app_id = repo::get_component_app_id(&state.db, component_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_length("label", &body.label, 1, 200)?;
    validate_length("url", &body.url, 1, 2000)?;

    let link_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_link",
        "component_link",
        link_id,
        json!({"label": body.label, "component_id": component_id}),
    )
    .await?;

    let link = link_repo::create_component_link(
        &state.db,
        link_id,
        component_id,
        &body.label,
        &body.url,
        body.link_type.as_deref().unwrap_or("documentation"),
        body.display_order.unwrap_or(0),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!(link))))
}

/// Update a component link.
pub async fn update_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, link_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateLinkRequest>,
) -> Result<Json<Value>, ApiError> {
    let app_id = repo::get_component_app_id(&state.db, component_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    if let Some(ref label) = body.label {
        validate_length("label", label, 1, 200)?;
    }
    if let Some(ref url) = body.url {
        validate_length("url", url, 1, 2000)?;
    }

    log_action(
        &state.db,
        user.user_id,
        "update_link",
        "component_link",
        link_id,
        json!({"component_id": component_id}),
    )
    .await?;

    let link = link_repo::update_component_link(
        &state.db,
        component_id,
        link_id,
        body.label.as_deref(),
        body.url.as_deref(),
        body.link_type.as_deref(),
        body.display_order,
    )
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(link)))
}

/// Delete a component link.
pub async fn delete_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let app_id = repo::get_component_app_id(&state.db, component_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_link",
        "component_link",
        link_id,
        json!({"component_id": component_id}),
    )
    .await?;

    if !link_repo::delete_component_link(&state.db, link_id, component_id).await? {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
