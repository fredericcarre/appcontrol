//! Sites CRUD API.
//!
//! Sites represent physical or logical locations (datacenters, DR sites, environments).
//! Applications, gateways, and agents are organized by site.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::repository::misc_queries;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub name: String,
    pub code: String,
    /// Site type: primary, dr, staging, development
    pub site_type: Option<String>,
    pub location: Option<String>,
    /// Optional hosting ID to assign this site to a hosting group
    pub hosting_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSiteRequest {
    pub name: Option<String>,
    pub location: Option<String>,
    pub is_active: Option<bool>,
    /// Assign a hosting. Set to a hosting UUID to assign, or "unset_hosting": true to unassign.
    pub hosting_id: Option<Uuid>,
    /// Set to true to remove the hosting assignment.
    pub unset_hosting: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListSitesQuery {
    pub site_type: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn list_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<ListSitesQuery>,
) -> Result<Json<Value>, ApiError> {
    let sites = state
        .site_repo
        .list_sites(
            *user.organization_id,
            query.site_type.as_deref(),
            query.is_active,
        )
        .await?;

    // Fetch hosting info for all sites
    let hosting_map =
        misc_queries::get_sites_hosting_info(&state.db, *user.organization_id).await?;

    let result: Vec<Value> = sites
        .into_iter()
        .map(|s| {
            let (hosting_id, hosting_name) = hosting_map
                .get(&s.id.to_string())
                .cloned()
                .unwrap_or((None, None));
            json!({
                "id": s.id,
                "organization_id": s.organization_id,
                "name": s.name,
                "code": s.code,
                "site_type": s.site_type,
                "location": s.location,
                "is_active": s.is_active,
                "created_at": s.created_at,
                "hosting_id": hosting_id,
                "hosting_name": hosting_name,
            })
        })
        .collect();

    Ok(Json(json!({ "sites": result })))
}

pub async fn get_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let site = state
        .site_repo
        .get_site(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let (hosting_id, hosting_name) =
        match misc_queries::get_site_hosting_info(&state.db, id).await? {
            Some((hid, hname)) => (hid.map(|h| h.to_string()), hname),
            None => (None, None),
        };

    Ok(Json(json!({
        "id": site.id,
        "organization_id": site.organization_id,
        "name": site.name,
        "code": site.code,
        "site_type": site.site_type,
        "location": site.location,
        "is_active": site.is_active,
        "created_at": site.created_at,
        "hosting_id": hosting_id,
        "hosting_name": hosting_name,
    })))
}

pub async fn create_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateSiteRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_length("name", &req.name, 1, 200)?;
    validate_length("code", &req.code, 1, 20)?;
    validate_optional_length("location", &req.location, 200)?;

    let site_type = req.site_type.as_deref().unwrap_or("primary");
    if !["primary", "dr", "staging", "development"].contains(&site_type) {
        return Err(ApiError::Validation(
            "site_type must be one of: primary, dr, staging, development".to_string(),
        ));
    }

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_site",
        "site",
        Uuid::nil(),
        json!({ "name": &req.name, "code": &req.code, "site_type": site_type }),
    )
    .await
    .ok();

    let site = state
        .site_repo
        .create_site(
            *user.organization_id,
            &req.name,
            &req.code,
            site_type,
            req.location.as_deref(),
        )
        .await?;

    // Assign hosting if provided
    if let Some(hosting_id) = req.hosting_id {
        misc_queries::set_site_hosting(&state.db, site.id, Some(hosting_id)).await?;
    }

    let (hosting_id, hosting_name) = if let Some(hid) = req.hosting_id {
        let h = state
            .hosting_repo
            .get_hosting(hid, *user.organization_id)
            .await?;
        (Some(hid.to_string()), h.map(|h| h.name))
    } else {
        (None, None)
    };

    Ok(Json(json!({
        "id": site.id,
        "organization_id": site.organization_id,
        "name": site.name,
        "code": site.code,
        "site_type": site.site_type,
        "location": site.location,
        "is_active": site.is_active,
        "created_at": site.created_at,
        "hosting_id": hosting_id,
        "hosting_name": hosting_name,
    })))
}

pub async fn update_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSiteRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_optional_length("name", &req.name, 200)?;
    validate_optional_length("location", &req.location, 200)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_site",
        "site",
        id,
        json!({ "name": &req.name, "location": &req.location, "is_active": req.is_active }),
    )
    .await
    .ok();

    let site = state
        .site_repo
        .update_site(
            id,
            *user.organization_id,
            req.name.as_deref(),
            req.location.as_deref(),
            req.is_active,
        )
        .await?
        .ok_or_not_found()?;

    // Handle hosting assignment
    if req.unset_hosting == Some(true) {
        misc_queries::set_site_hosting(&state.db, id, None).await?;
    } else if let Some(hosting_id) = req.hosting_id {
        misc_queries::set_site_hosting(&state.db, id, Some(hosting_id)).await?;
    }

    let (hosting_id, hosting_name) =
        match misc_queries::get_site_hosting_info(&state.db, id).await? {
            Some((hid, hname)) => (hid.map(|h| h.to_string()), hname),
            None => (None, None),
        };

    Ok(Json(json!({
        "id": site.id,
        "organization_id": site.organization_id,
        "name": site.name,
        "code": site.code,
        "site_type": site.site_type,
        "location": site.location,
        "is_active": site.is_active,
        "created_at": site.created_at,
        "hosting_id": hosting_id,
        "hosting_name": hosting_name,
    })))
}

pub async fn delete_site(
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
        "delete_site",
        "site",
        id,
        json!({}),
    )
    .await
    .ok();

    // Check for applications linked to this site
    let app_count = state.site_repo.count_apps_in_site(id).await?;
    if app_count > 0 {
        return Err(ApiError::Conflict(format!(
            "Cannot delete site: {} application(s) are linked to it",
            app_count
        )));
    }

    let deleted = state
        .site_repo
        .delete_site(id, *user.organization_id)
        .await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }

    Ok(Json(json!({ "status": "deleted" })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_site_type_validation() {
        assert!(["primary", "dr", "staging", "development"].contains(&"primary"));
        assert!(!["primary", "dr", "staging", "development"].contains(&"invalid"));
    }
}
