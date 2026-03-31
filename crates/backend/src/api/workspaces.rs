//! Workspace management API endpoints.
//!
//! Workspaces control which sites/zones a user or team can access.
//! This prevents users from seeing or operating on machines outside their scope.

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbUuid;
use crate::error::{validate_length, validate_optional_length, ApiError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WorkspaceRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

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

/// GET /api/v1/workspaces — list all workspaces in the organization.
pub async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let workspaces = sqlx::query_as::<_, WorkspaceRow>(
        "SELECT id, organization_id, name, description, created_at
         FROM workspaces WHERE organization_id = $1 ORDER BY name",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "workspaces": workspaces })))
}

/// POST /api/v1/workspaces — create a workspace (requires org admin).
pub async fn create_workspace(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, organization_id, name, description) VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(id))
    .bind(user.organization_id)
    .bind(&body.name)
    .bind(&body.description)
    .execute(&state.db)
    .await?;

    // Audit log
    #[cfg(feature = "postgres")]
    let _ = sqlx::query(
        "INSERT INTO action_log (user_id, action, resource_type, resource_id, details)
         VALUES ($1, 'create_workspace', 'workspace', $2, $3)",
    )
    .bind(user.user_id)
    .bind(crate::db::bind_id(id))
    .bind(json!({"name": body.name}))
    .execute(&state.db)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let _ = sqlx::query(
        "INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details)
         VALUES ($1, $2, 'create_workspace', 'workspace', $3, $4)",
    )
    .bind(crate::db::bind_id(uuid::Uuid::new_v4()))
    .bind(user.user_id)
    .bind(crate::db::bind_id(id))
    .bind(json!({"name": body.name}).to_string())
    .execute(&state.db)
    .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": body.name,
            "description": body.description,
        })),
    ))
}

/// DELETE /api/v1/workspaces/:id — delete a workspace (requires org admin).
pub async fn delete_workspace(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let result = sqlx::query("DELETE FROM workspaces WHERE id = $1 AND organization_id = $2")
        .bind(crate::db::bind_id(id))
        .bind(user.organization_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Workspace-Site bindings
// ---------------------------------------------------------------------------

/// GET /api/v1/workspaces/:id/sites — list sites in this workspace.
pub async fn list_workspace_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Verify workspace belongs to user's org
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND organization_id = $2)",
    )
    .bind(crate::db::bind_id(workspace_id))
    .bind(user.organization_id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(ApiError::NotFound);
    }

    let sites = sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"
        SELECT s.id, s.name, s.code
        FROM sites s
        JOIN workspace_sites ws ON ws.site_id = s.id
        WHERE ws.workspace_id = $1
        ORDER BY s.name
        "#,
    )
    .bind(crate::db::bind_id(workspace_id))
    .fetch_all(&state.db)
    .await?;

    let sites_json: Vec<Value> = sites
        .iter()
        .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
        .collect();

    Ok(Json(json!({ "sites": sites_json })))
}

/// POST /api/v1/workspaces/:id/sites — add a site to the workspace (requires org admin).
pub async fn add_workspace_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<AddWorkspaceSite>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    sqlx::query(
        "INSERT INTO workspace_sites (workspace_id, site_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(crate::db::bind_id(workspace_id))
    .bind(body.site_id)
    .execute(&state.db)
    .await?;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/v1/workspaces/:id/sites/:site_id — remove a site from the workspace.
pub async fn remove_workspace_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((workspace_id, site_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    sqlx::query("DELETE FROM workspace_sites WHERE workspace_id = $1 AND site_id = $2")
        .bind(crate::db::bind_id(workspace_id))
        .bind(crate::db::bind_id(site_id))
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Workspace-Member bindings
// ---------------------------------------------------------------------------

/// GET /api/v1/workspaces/:id/members — list members of this workspace.
pub async fn list_workspace_members(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Verify workspace belongs to user's org
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND organization_id = $2)",
    )
    .bind(crate::db::bind_id(workspace_id))
    .bind(user.organization_id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(ApiError::NotFound);
    }

    let members = sqlx::query_as::<_, (DbUuid, Option<DbUuid>, Option<DbUuid>, String)>(
        r#"
        SELECT wm.id, wm.user_id, wm.team_id, wm.role
        FROM workspace_members wm
        WHERE wm.workspace_id = $1
        ORDER BY wm.created_at
        "#,
    )
    .bind(crate::db::bind_id(workspace_id))
    .fetch_all(&state.db)
    .await?;

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

/// POST /api/v1/workspaces/:id/members — add a member (user or team) to the workspace.
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

    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, team_id, role)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT DO NOTHING",
    )
    .bind(crate::db::bind_id(workspace_id))
    .bind(body.user_id)
    .bind(body.team_id)
    .bind(&body.role)
    .execute(&state.db)
    .await?;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/v1/workspaces/:id/members/:member_id — remove a member from the workspace.
pub async fn remove_workspace_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((workspace_id, member_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    sqlx::query("DELETE FROM workspace_members WHERE id = $1 AND workspace_id = $2")
        .bind(member_id)
        .bind(crate::db::bind_id(workspace_id))
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/workspaces/my-sites — list all sites the current user has access to.
pub async fn my_accessible_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if user.is_admin() {
        // Admin sees all sites
        let sites = sqlx::query_as::<_, (DbUuid, String, String)>(
            "SELECT id, name, code FROM sites WHERE organization_id = $1 ORDER BY name",
        )
        .bind(user.organization_id)
        .fetch_all(&state.db)
        .await?;

        let sites_json: Vec<Value> = sites
            .iter()
            .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
            .collect();

        return Ok(Json(json!({ "sites": sites_json })));
    }

    // Check if workspace-site feature is configured
    let has_any = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspace_sites ws JOIN workspaces w ON w.id = ws.workspace_id WHERE w.organization_id = $1)",
    )
    .bind(user.organization_id)
    .fetch_one(&state.db)
    .await?;

    if !has_any {
        // Feature not configured → return all sites
        let sites = sqlx::query_as::<_, (DbUuid, String, String)>(
            "SELECT id, name, code FROM sites WHERE organization_id = $1 ORDER BY name",
        )
        .bind(user.organization_id)
        .fetch_all(&state.db)
        .await?;

        let sites_json: Vec<Value> = sites
            .iter()
            .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
            .collect();

        return Ok(Json(json!({ "sites": sites_json })));
    }

    // Return only sites from user's workspaces
    let sites = sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"
        SELECT DISTINCT s.id, s.name, s.code
        FROM sites s
        JOIN workspace_sites ws ON ws.site_id = s.id
        JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
        WHERE s.organization_id = $1
          AND (
              wm.user_id = $2
              OR wm.team_id IN (SELECT team_id FROM team_members WHERE user_id = $2)
          )
        ORDER BY s.name
        "#,
    )
    .bind(user.organization_id)
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;

    let sites_json: Vec<Value> = sites
        .iter()
        .map(|(id, name, code)| json!({"id": id, "name": name, "code": code}))
        .collect();

    Ok(Json(json!({ "sites": sites_json })))
}
