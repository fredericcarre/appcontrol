//! Workspace management API endpoints.
//!
//! Workspaces control which sites/zones a user or team can access.
//! This prevents users from seeing or operating on machines outside their scope.

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
use crate::db::DbUuid;
use crate::error::{validate_length, validate_optional_length, ApiError};
use crate::repository::misc_queries;
use crate::AppState;

// Re-export from repository
pub use misc_queries::WorkspaceRow;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateWorkspace {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddWorkspaceSite {
    pub site_id: DbUuid,
}

#[derive(Debug, Deserialize)]
pub struct AddWorkspaceMember {
    #[serde(default)]
    pub user_id: Option<DbUuid>,
    #[serde(default)]
    pub team_id: Option<DbUuid>,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".to_string()
}

// ---------------------------------------------------------------------------
// Workspace CRUD
// ---------------------------------------------------------------------------

/// GET /api/v1/workspaces
pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let workspaces = misc_queries::list_workspaces(&state.db, user.organization_id).await?;
    Ok(Json(json!({ "workspaces": workspaces })))
}

/// POST /api/v1/workspaces
pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let id = Uuid::new_v4();
    misc_queries::create_workspace(
        &state.db,
        id,
        user.organization_id,
        &body.name,
        &body.description,
    )
    .await?;

    // Audit log
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_workspace",
        "workspace",
        id,
        json!({"name": body.name}),
    )
    .await
    .ok();

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": body.name,
            "description": body.description,
        })),
    ))
}

/// DELETE /api/v1/workspaces/:id
pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = misc_queries::delete_workspace(&state.db, id, user.organization_id).await?;

    if rows == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Workspace-Site bindings
// ---------------------------------------------------------------------------

/// GET /api/v1/workspaces/:id/sites
pub async fn list_workspace_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let exists =
        misc_queries::workspace_exists(&state.db, workspace_id, user.organization_id).await?;

    if !exists {
        return Err(ApiError::NotFound);
    }

    let sites = misc_queries::list_workspace_sites(&state.db, workspace_id).await?;

    let sites_json: Vec<Value> = sites
        .iter()
        .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
        .collect();

    Ok(Json(json!({ "sites": sites_json })))
}

/// POST /api/v1/workspaces/:id/sites
pub async fn add_workspace_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<AddWorkspaceSite>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    misc_queries::add_workspace_site(&state.db, workspace_id, body.site_id).await?;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/v1/workspaces/:id/sites/:site_id
pub async fn remove_workspace_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((workspace_id, site_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    misc_queries::remove_workspace_site(&state.db, workspace_id, site_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Workspace-Member bindings
// ---------------------------------------------------------------------------

/// GET /api/v1/workspaces/:id/members
pub async fn list_workspace_members(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let exists =
        misc_queries::workspace_exists(&state.db, workspace_id, user.organization_id).await?;

    if !exists {
        return Err(ApiError::NotFound);
    }

    let members = misc_queries::list_workspace_members(&state.db, workspace_id).await?;

    let members_json: Vec<Value> = members
        .iter()
        .map(|(id, user_id, team_id, role)| {
            json!({
                "id": id,
                "user_id": user_id,
                "team_id": team_id,
                "role": role,
            })
        })
        .collect();

    Ok(Json(json!({ "members": members_json })))
}

/// POST /api/v1/workspaces/:id/members
pub async fn add_workspace_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<AddWorkspaceMember>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if body.user_id.is_none() && body.team_id.is_none() {
        return Err(ApiError::Validation(
            "Either user_id or team_id must be provided".to_string(),
        ));
    }

    misc_queries::add_workspace_member(
        &state.db,
        workspace_id,
        body.user_id,
        body.team_id,
        &body.role,
    )
    .await?;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/v1/workspaces/:id/members/:member_id
pub async fn remove_workspace_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((workspace_id, member_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    misc_queries::remove_workspace_member(&state.db, member_id, workspace_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/workspaces/my-sites
pub async fn my_accessible_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if user.is_admin() {
        let sites = misc_queries::list_org_sites(&state.db, user.organization_id).await?;

        let sites_json: Vec<Value> = sites
            .iter()
            .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
            .collect();

        return Ok(Json(json!({ "sites": sites_json })));
    }

    // Check if workspace-site feature is configured
    let has_any =
        misc_queries::has_workspace_sites_configured(&state.db, user.organization_id).await?;

    if !has_any {
        let sites = misc_queries::list_org_sites(&state.db, user.organization_id).await?;

        let sites_json: Vec<Value> = sites
            .iter()
            .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
            .collect();

        return Ok(Json(json!({ "sites": sites_json })));
    }

    // Return only sites from user's workspaces
    let sites =
        misc_queries::list_user_accessible_sites(&state.db, user.organization_id, user.user_id)
            .await?;

    let sites_json: Vec<Value> = sites
        .iter()
        .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
        .collect();

    Ok(Json(json!({ "sites": sites_json })))
}
