//! Hostings CRUD API.
//!
//! A hosting is a logical grouping of sites (e.g., a datacenter or cloud region).
//! Sites can be assigned to a hosting to represent physical co-location.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateHostingRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHostingRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// GET /api/v1/hostings
pub async fn list_hostings(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let hostings = state
        .hosting_repo
        .list_hostings(*user.organization_id)
        .await?;

    let result: Vec<Value> = hostings
        .into_iter()
        .map(|h| {
            json!({
                "id": h.id,
                "organization_id": h.organization_id,
                "name": h.name,
                "description": h.description,
                "created_at": h.created_at,
                "updated_at": h.updated_at,
            })
        })
        .collect();

    Ok(Json(json!({ "hostings": result })))
}

/// GET /api/v1/hostings/:id
pub async fn get_hosting(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let hosting = state
        .hosting_repo
        .get_hosting(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!({
        "id": hosting.id,
        "organization_id": hosting.organization_id,
        "name": hosting.name,
        "description": hosting.description,
        "created_at": hosting.created_at,
        "updated_at": hosting.updated_at,
    })))
}

/// GET /api/v1/hostings/:id/sites
pub async fn list_hosting_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(hosting_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Verify hosting exists and belongs to org
    let _hosting = state
        .hosting_repo
        .get_hosting(hosting_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let hosting_sites =
        crate::repository::misc_queries::list_sites_for_hosting(&state.db, hosting_id).await?;

    Ok(Json(json!({ "sites": hosting_sites })))
}

/// POST /api/v1/hostings
pub async fn create_hosting(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateHostingRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_length("name", &req.name, 1, 200)?;
    validate_optional_length("description", &req.description, 1000)?;

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_hosting",
        "hosting",
        Uuid::nil(),
        json!({ "name": &req.name }),
    )
    .await
    .ok();

    let hosting = state
        .hosting_repo
        .create_hosting(
            *user.organization_id,
            &req.name,
            req.description.as_deref(),
        )
        .await?;

    Ok(Json(json!({
        "id": hosting.id,
        "organization_id": hosting.organization_id,
        "name": hosting.name,
        "description": hosting.description,
        "created_at": hosting.created_at,
        "updated_at": hosting.updated_at,
    })))
}

/// PUT /api/v1/hostings/:id
pub async fn update_hosting(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateHostingRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_optional_length("name", &req.name, 200)?;
    validate_optional_length("description", &req.description, 1000)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_hosting",
        "hosting",
        id,
        json!({ "name": &req.name, "description": &req.description }),
    )
    .await
    .ok();

    let hosting = state
        .hosting_repo
        .update_hosting(
            id,
            *user.organization_id,
            req.name.as_deref(),
            req.description.as_deref(),
        )
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!({
        "id": hosting.id,
        "organization_id": hosting.organization_id,
        "name": hosting.name,
        "description": hosting.description,
        "created_at": hosting.created_at,
        "updated_at": hosting.updated_at,
    })))
}

/// DELETE /api/v1/hostings/:id
pub async fn delete_hosting(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "delete_hosting",
        "hosting",
        id,
        json!({}),
    )
    .await
    .ok();

    // Check for sites linked to this hosting
    let site_count = state.hosting_repo.count_sites_in_hosting(id).await?;
    if site_count > 0 {
        return Err(ApiError::Conflict(format!(
            "Cannot delete hosting: {} site(s) are linked to it. Unassign them first.",
            site_count
        )));
    }

    let deleted = state
        .hosting_repo
        .delete_hosting(id, *user.organization_id)
        .await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }

    Ok(Json(json!({ "status": "deleted" })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hosting_request_validation() {
        // Basic sanity test for the request structures
        let req = super::CreateHostingRequest {
            name: "DC Paris".to_string(),
            description: Some("Datacenter in Paris region".to_string()),
        };
        assert_eq!(req.name, "DC Paris");
        assert!(req.description.is_some());
    }
}
