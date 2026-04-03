//! Organization management API.
//!
//! Only platform super-admins can create and manage organizations.
//! Each organization is an isolated tenant with its own PKI, users, sites, and apps.

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
use crate::repository::org_queries;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
    /// Email of the initial org admin (created as local user)
    pub admin_email: String,
    /// Display name for the org admin
    pub admin_display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrgRequest {
    pub name: Option<String>,
}

// Re-export OrgRow from repository for backward compatibility
pub use crate::repository::org_queries::OrgRow;

/// Check if the user is a platform super-admin.
fn require_super_admin(
    _user: &AuthUser,
    db_platform_role: &Option<String>,
) -> Result<(), ApiError> {
    match db_platform_role {
        Some(role) if role == "super_admin" => Ok(()),
        _ => Err(ApiError::Forbidden),
    }
}

pub async fn list_organizations(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = org_queries::get_platform_role(&state.db, user.user_id).await;
    require_super_admin(&user, &platform_role)?;

    let orgs = org_queries::list_organizations(&state.db).await?;

    Ok(Json(json!({ "organizations": orgs })))
}

pub async fn get_organization(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = org_queries::get_platform_role(&state.db, user.user_id).await;

    // Super-admins can view any org; regular admins can view their own
    if platform_role.as_deref() != Some("super_admin") && *user.organization_id != id {
        return Err(ApiError::Forbidden);
    }

    let org = org_queries::get_organization(&state.db, id)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!(org)))
}

pub async fn create_organization(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateOrgRequest>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = org_queries::get_platform_role(&state.db, user.user_id).await;
    require_super_admin(&user, &platform_role)?;

    validate_length("name", &req.name, 1, 200)?;
    validate_length("slug", &req.slug, 1, 100)?;
    validate_length("admin_email", &req.admin_email, 3, 300)?;
    validate_length("admin_display_name", &req.admin_display_name, 1, 200)?;

    // Validate slug format (alphanumeric + hyphens only)
    if !req
        .slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(ApiError::Validation(
            "slug must contain only alphanumeric characters and hyphens".to_string(),
        ));
    }

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_organization",
        "organization",
        Uuid::nil(),
        json!({ "name": &req.name, "slug": &req.slug, "admin_email": &req.admin_email }),
    )
    .await
    .ok();

    // Create org + admin user in a single transaction (via repository)
    let result = org_queries::create_organization_with_admin(
        &state.db,
        &req.name,
        &req.slug,
        &req.admin_email,
        &req.admin_display_name,
    )
    .await?;

    Ok(Json(json!({
        "organization": result.org,
        "admin_user_id": result.admin_id,
        "admin_email": req.admin_email,
    })))
}

pub async fn update_organization(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOrgRequest>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = org_queries::get_platform_role(&state.db, user.user_id).await;
    require_super_admin(&user, &platform_role)?;

    validate_optional_length("name", &req.name, 200)?;

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_organization",
        "organization",
        id,
        json!({ "name": &req.name }),
    )
    .await
    .ok();

    let org = org_queries::update_organization(&state.db, id, &req.name)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!(org)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_slug_validation() {
        let valid = "my-org-01";
        assert!(valid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));

        let invalid = "my org!";
        assert!(!invalid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
