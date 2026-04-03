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
use crate::core::permissions::{can_access_site, effective_permission};
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;
use axum::extract::Query;

#[derive(Debug, Deserialize)]
pub struct GrantUserPermissionRequest {
    pub user_id: Uuid,
    pub permission_level: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct GrantTeamPermissionRequest {
    pub team_id: Uuid,
    pub permission_level: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateShareLinkRequest {
    pub permission_level: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub max_uses: Option<i32>,
}

pub async fn list_user_permissions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let perms = state.permission_repo.list_user_permissions(app_id).await?;

    let permissions: Vec<Value> = perms
        .iter()
        .map(|p| json!({"id": p.id, "user_id": p.user_id, "permission_level": p.permission_level, "expires_at": p.expires_at}))
        .collect();

    Ok(Json(json!({ "permissions": permissions })))
}

/// Resolve the site_id and organization_id for an application.
/// Uses the permission_repo for database abstraction.
async fn app_site_info(
    permission_repo: &dyn crate::repository::permissions::PermissionRepository,
    app_id: Uuid,
) -> Option<(Uuid, Uuid)> {
    permission_repo.app_site_info(app_id).await.ok().flatten()
}

/// Validate that a target user has workspace access to the app's site.
/// Skipped when workspace feature is not configured (no workspace_sites rows).
async fn validate_workspace_access(
    pool: &crate::db::DbPool,
    permission_repo: &dyn crate::repository::permissions::PermissionRepository,
    target_user_id: Uuid,
    app_id: Uuid,
) -> Result<(), ApiError> {
    if let Some((site_id, org_id)) = app_site_info(permission_repo, app_id).await {
        // Check if target user can access the site — pass is_admin=false
        // because we want to verify actual workspace membership.
        if !can_access_site(pool, target_user_id, site_id, org_id, false).await {
            return Err(ApiError::Validation(
                "Target user does not have workspace access to this application's site".into(),
            ));
        }
    }
    Ok(())
}

pub async fn grant_user_permission(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<GrantUserPermissionRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Validate target user has workspace access to the app's site
    validate_workspace_access(
        &state.db,
        state.permission_repo.as_ref(),
        body.user_id,
        app_id,
    )
    .await?;

    log_action(
        &state.db,
        user.user_id,
        "grant_permission",
        "application",
        app_id,
        json!({"target_user": body.user_id, "level": body.permission_level}),
    )
    .await?;

    let id = state
        .permission_repo
        .grant_user_permission(
            app_id,
            body.user_id,
            &body.permission_level,
            *user.user_id,
            body.expires_at,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({"id": id, "status": "granted"})),
    ))
}

pub async fn list_team_permissions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let perms = state.permission_repo.list_team_permissions(app_id).await?;

    let permissions: Vec<Value> = perms
        .iter()
        .map(|p| json!({"id": p.id, "team_id": p.team_id, "permission_level": p.permission_level, "expires_at": p.expires_at}))
        .collect();

    Ok(Json(json!({ "permissions": permissions })))
}

pub async fn grant_team_permission(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<GrantTeamPermissionRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Validate that team members can access the site via workspace.
    // We check if at least the workspace feature is active and the team
    // has at least one member with access. If workspace is not configured,
    // can_access_site returns true (open access), so this is a no-op.
    if let Some((site_id, org_id)) = app_site_info(state.permission_repo.as_ref(), app_id).await {
        // Check if workspace feature is configured — if any workspace_sites exist
        let has_ws = crate::repository::permissions::has_workspace_sites(&state.db, org_id).await;

        if has_ws {
            // Verify team is in a workspace that includes this site
            let team_has_access = crate::repository::permissions::team_has_site_access(
                &state.db,
                site_id,
                body.team_id,
            )
            .await;

            if !team_has_access {
                return Err(ApiError::Validation(
                    "Team does not have workspace access to this application's site".into(),
                ));
            }
        }
    }

    log_action(
        &state.db,
        user.user_id,
        "grant_team_permission",
        "application",
        app_id,
        json!({"team_id": body.team_id, "level": body.permission_level}),
    )
    .await?;

    let id = state
        .permission_repo
        .grant_team_permission(
            app_id,
            body.team_id,
            &body.permission_level,
            *user.user_id,
            body.expires_at,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({"id": id, "status": "granted"})),
    ))
}

pub async fn list_share_links(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let links = crate::repository::permissions::list_active_share_links(&state.db, app_id).await?;

    let share_links: Vec<Value> = links
        .iter()
        .map(|(id, token, level, exp, max, count)| {
            json!({"id": id, "token": token, "permission_level": level, "expires_at": exp, "max_uses": max, "use_count": count})
        })
        .collect();

    Ok(Json(json!({ "share_links": share_links })))
}

pub async fn create_share_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateShareLinkRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let token = uuid::Uuid::new_v4().to_string().replace('-', "");

    log_action(
        &state.db,
        user.user_id,
        "create_share_link",
        "application",
        app_id,
        json!({"level": body.permission_level}),
    )
    .await?;

    let id = crate::repository::permissions::insert_share_link(
        &state.db,
        app_id,
        &token,
        &body.permission_level,
        *user.user_id,
        body.expires_at,
        body.max_uses,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({"id": id, "token": token}))))
}

pub async fn get_effective_permission(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    Ok(Json(
        json!({ "permission_level": format!("{:?}", perm).to_lowercase() }),
    ))
}

// ---------------------------------------------------------------------------
// Delete a specific permission entry (user or team)
// ---------------------------------------------------------------------------

pub async fn delete_permission(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, perm_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Try user permissions first, then team permissions
    let deleted =
        crate::repository::permissions::delete_user_permission(&state.db, perm_id, app_id).await?;

    if deleted == 0 {
        let deleted_team =
            crate::repository::permissions::delete_team_permission(&state.db, perm_id, app_id)
                .await?;

        if deleted_team == 0 {
            return Err(ApiError::NotFound);
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// User search / discovery — for the sharing user-picker
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UserSearchQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

pub async fn search_users(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(params): Query<UserSearchQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(20).min(50);
    let query = params.q.unwrap_or_default();

    let users = if query.is_empty() {
        crate::repository::permissions::list_org_users(&state.db, *user.organization_id, limit)
            .await?
    } else {
        let pattern = format!("%{}%", query);
        crate::repository::permissions::search_users_by_pattern(
            &state.db,
            *user.organization_id,
            &pattern,
            limit,
        )
        .await?
    };

    let data: Vec<Value> = users
        .iter()
        .map(|(id, email, name, role)| {
            json!({
                "id": id,
                "email": email,
                "display_name": name,
                "role": role,
            })
        })
        .collect();

    Ok(Json(json!({ "users": data })))
}

// ---------------------------------------------------------------------------
// Share link consumption — validate token and grant permission
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ConsumeShareLinkRequest {
    pub token: String,
}

pub async fn consume_share_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ConsumeShareLinkRequest>,
) -> Result<Json<Value>, ApiError> {
    // Look up the share link
    let link = crate::repository::permissions::get_share_link_for_consume(&state.db, &body.token)
        .await?
        .ok_or(ApiError::NotFound)?;

    let (link_id, raw_app_id, permission_level, expires_at, max_uses, use_count) = link;
    let app_id: Uuid = raw_app_id.into_inner();

    // Check expiration
    if let Some(exp) = expires_at {
        if exp < chrono::Utc::now() {
            return Err(ApiError::Validation("Share link has expired".into()));
        }
    }

    // Check max uses
    if let Some(max) = max_uses {
        if use_count >= max {
            return Err(ApiError::Validation(
                "Share link has reached maximum uses".into(),
            ));
        }
    }

    // Validate workspace access for the consuming user
    validate_workspace_access(
        &state.db,
        state.permission_repo.as_ref(),
        *user.user_id,
        app_id,
    )
    .await?;

    // Grant permission to the user
    crate::repository::permissions::grant_permission_via_share_link(
        &state.db,
        app_id,
        *user.user_id,
        &permission_level,
    )
    .await?;

    // Increment use count
    crate::repository::permissions::increment_share_link_use_count(&state.db, *link_id).await?;

    log_action(
        &state.db,
        user.user_id,
        "consume_share_link",
        "application",
        app_id,
        json!({"link_id": link_id, "level": permission_level}),
    )
    .await?;

    Ok(Json(json!({
        "app_id": app_id,
        "permission_level": permission_level,
        "status": "accepted",
    })))
}

// ---------------------------------------------------------------------------
// Revoke a share link
// ---------------------------------------------------------------------------

pub async fn revoke_share_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let rows =
        crate::repository::permissions::revoke_share_link_by_id(&state.db, link_id, app_id).await?;

    if rows == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Combined permissions list (users + teams in one call)
// ---------------------------------------------------------------------------

pub async fn list_all_permissions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    let user_perms =
        crate::repository::permissions::list_all_user_permissions(&state.db, app_id).await?;

    let team_perms =
        crate::repository::permissions::list_all_team_permissions(&state.db, app_id).await?;

    let mut permissions: Vec<Value> = Vec::new();

    for (id, uid, level, email, exp) in &user_perms {
        permissions.push(json!({
            "id": id,
            "user_id": uid,
            "user_email": email,
            "level": level,
            "expires_at": exp,
            "type": "user",
        }));
    }

    for (id, tid, level, name, exp) in &team_perms {
        permissions.push(json!({
            "id": id,
            "team_id": tid,
            "team_name": name,
            "level": level,
            "expires_at": exp,
            "type": "team",
        }));
    }

    Ok(Json(json!({ "permissions": permissions })))
}

// ---------------------------------------------------------------------------
// Public share link info (unauthenticated) — preview before login
// ---------------------------------------------------------------------------

pub async fn get_share_link_info(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let link = crate::repository::permissions::get_share_link_info(&state.db, &token)
        .await?
        .ok_or(ApiError::NotFound)?;

    let (app_id, permission_level, expires_at, max_uses, use_count, app_name) = link;

    // Check validity
    let expired = expires_at.is_some_and(|exp| exp < chrono::Utc::now());
    let exhausted = max_uses.is_some_and(|max| use_count >= max);

    Ok(Json(json!({
        "app_id": app_id,
        "app_name": app_name,
        "permission_level": permission_level,
        "expired": expired,
        "exhausted": exhausted,
        "valid": !expired && !exhausted,
    })))
}
