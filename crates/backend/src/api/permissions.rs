use crate::db::DbUuid;
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

    let perms = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT apu.id, apu.user_id, apu.permission_level, apu.expires_at
        FROM app_permissions_users apu
        WHERE apu.application_id = $1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let permissions: Vec<Value> = perms
        .iter()
        .map(|(id, uid, level, exp)| json!({"id": id, "user_id": uid, "permission_level": level, "expires_at": exp}))
        .collect();

    Ok(Json(json!({ "permissions": permissions })))
}

/// Resolve the site_id and organization_id for an application.
async fn app_site_info(pool: &crate::db::DbPool, app_id: Uuid) -> Option<(Uuid, Uuid)> {
    sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT site_id, organization_id FROM applications WHERE id = $1",
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|(s, o)| (s.into_inner(), o.into_inner()))
}

/// Validate that a target user has workspace access to the app's site.
/// Skipped when workspace feature is not configured (no workspace_sites rows).
async fn validate_workspace_access(
    pool: &crate::db::DbPool,
    target_user_id: Uuid,
    app_id: Uuid,
) -> Result<(), ApiError> {
    if let Some((site_id, org_id)) = app_site_info(pool, app_id).await {
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
    validate_workspace_access(&state.db, body.user_id, app_id).await?;

    log_action(
        &state.db,
        user.user_id,
        "grant_permission",
        "application",
        app_id,
        json!({"target_user": body.user_id, "level": body.permission_level}),
    )
    .await?;

    let id = sqlx::query_scalar::<_, DbUuid>(
        &format!(
            "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by, expires_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (application_id, user_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = {}
             RETURNING id",
            crate::db::sql::now()
        ),
    )
    .bind(app_id)
    .bind(body.user_id)
    .bind(&body.permission_level)
    .bind(user.user_id)
    .bind(body.expires_at)
    .fetch_one(&state.db)
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

    let perms = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT apt.id, apt.team_id, apt.permission_level, apt.expires_at
        FROM app_permissions_teams apt
        WHERE apt.application_id = $1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let permissions: Vec<Value> = perms
        .iter()
        .map(|(id, tid, level, exp)| json!({"id": id, "team_id": tid, "permission_level": level, "expires_at": exp}))
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
    if let Some((site_id, org_id)) = app_site_info(&state.db, app_id).await {
        // Check if workspace feature is configured — if any workspace_sites exist
        let has_ws = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM workspace_sites ws JOIN workspaces w ON w.id = ws.workspace_id WHERE w.organization_id = $1)",
        )
        .bind(org_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

        if has_ws {
            // Verify team is in a workspace that includes this site
            let team_has_access = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM workspace_sites ws
                    JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
                    WHERE ws.site_id = $1 AND wm.team_id = $2
                )
                "#,
            )
            .bind(site_id)
            .bind(body.team_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false);

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

    let id = sqlx::query_scalar::<_, DbUuid>(
        &format!(
            "INSERT INTO app_permissions_teams (application_id, team_id, permission_level, granted_by, expires_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (application_id, team_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = {}
             RETURNING id",
            crate::db::sql::now()
        ),
    )
    .bind(app_id)
    .bind(body.team_id)
    .bind(&body.permission_level)
    .bind(user.user_id)
    .bind(body.expires_at)
    .fetch_one(&state.db)
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

    #[cfg(feature = "postgres")]
    let links = sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<i32>,
            i32,
        ),
    >(
        r#"
        SELECT id, token, permission_level, expires_at, max_uses, use_count
        FROM app_share_links
        WHERE application_id = $1 AND is_active = true
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let links = sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<i32>,
            i32,
        ),
    >(
        r#"
        SELECT id, token, permission_level, expires_at, max_uses, use_count
        FROM app_share_links
        WHERE application_id = $1 AND is_active = 1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

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

    let id = sqlx::query_scalar::<_, DbUuid>(
        r#"
        INSERT INTO app_share_links (application_id, token, permission_level, created_by, expires_at, max_uses)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(app_id)
    .bind(&token)
    .bind(&body.permission_level)
    .bind(user.user_id)
    .bind(body.expires_at)
    .bind(body.max_uses)
    .fetch_one(&state.db)
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
        sqlx::query("DELETE FROM app_permissions_users WHERE id = $1 AND application_id = $2")
            .bind(perm_id)
            .bind(app_id)
            .execute(&state.db)
            .await?;

    if deleted.rows_affected() == 0 {
        let deleted_team =
            sqlx::query("DELETE FROM app_permissions_teams WHERE id = $1 AND application_id = $2")
                .bind(perm_id)
                .bind(app_id)
                .execute(&state.db)
                .await?;

        if deleted_team.rows_affected() == 0 {
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
        // Return users in the same organization
        #[cfg(feature = "postgres")]
        let result = sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
            r#"
            SELECT id, email, display_name, role
            FROM users
            WHERE organization_id = $1 AND is_active = true
            ORDER BY display_name, email
            LIMIT $2
            "#,
        )
        .bind(user.organization_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await?;
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let result = sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
            r#"
            SELECT id, email, display_name, role
            FROM users
            WHERE organization_id = $1 AND is_active = 1
            ORDER BY display_name, email
            LIMIT $2
            "#,
        )
        .bind(user.organization_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await?;
        result
    } else {
        // Search by email or display name (case-insensitive)
        let pattern = format!("%{}%", query);
        search_users_by_pattern(&state.db, user.organization_id, &pattern, limit).await?
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
    #[cfg(feature = "postgres")]
    let link = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<i32>,
            i32,
        ),
    >(
        r#"
        SELECT id, application_id, permission_level, expires_at, max_uses, use_count
        FROM app_share_links
        WHERE token = $1 AND is_active = true
        "#,
    )
    .bind(&body.token)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let link = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<i32>,
            i32,
        ),
    >(
        r#"
        SELECT id, application_id, permission_level, expires_at, max_uses, use_count
        FROM app_share_links
        WHERE token = $1 AND is_active = 1
        "#,
    )
    .bind(&body.token)
    .fetch_optional(&state.db)
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
    validate_workspace_access(&state.db, user.user_id, app_id).await?;

    // Grant permission to the user
    sqlx::query(
        &format!(
            "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by, expires_at)
             VALUES ($1, $2, $3, $4, NULL)
             ON CONFLICT (application_id, user_id) DO UPDATE SET
                 permission_level = CASE
                     WHEN EXCLUDED.permission_level > app_permissions_users.permission_level
                     THEN EXCLUDED.permission_level
                     ELSE app_permissions_users.permission_level
                 END,
                 updated_at = {}",
            crate::db::sql::now()
        ),
    )
    .bind(app_id)
    .bind(user.user_id)
    .bind(&permission_level)
    .bind(user.user_id)
    .execute(&state.db)
    .await?;

    // Increment use count
    sqlx::query("UPDATE app_share_links SET use_count = use_count + 1 WHERE id = $1")
        .bind(link_id)
        .execute(&state.db)
        .await?;

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

    #[cfg(feature = "postgres")]
    let result = sqlx::query(
        "UPDATE app_share_links SET is_active = false WHERE id = $1 AND application_id = $2",
    )
    .bind(link_id)
    .bind(app_id)
    .execute(&state.db)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query(
        "UPDATE app_share_links SET is_active = 0 WHERE id = $1 AND application_id = $2",
    )
    .bind(DbUuid::from(link_id))
    .bind(DbUuid::from(app_id))
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
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

    let user_perms = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT apu.id, apu.user_id, apu.permission_level, u.email, apu.expires_at
        FROM app_permissions_users apu
        LEFT JOIN users u ON u.id = apu.user_id
        WHERE apu.application_id = $1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let team_perms = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT apt.id, apt.team_id, apt.permission_level, t.name, apt.expires_at
        FROM app_permissions_teams apt
        LEFT JOIN teams t ON t.id = apt.team_id
        WHERE apt.application_id = $1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

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
    #[cfg(feature = "postgres")]
    let link = sqlx::query_as::<_, (DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32, String)>(
        r#"
        SELECT sl.application_id, sl.permission_level, sl.expires_at, sl.max_uses, sl.use_count, a.name
        FROM app_share_links sl
        JOIN applications a ON a.id = sl.application_id
        WHERE sl.token = $1 AND sl.is_active = true
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let link = sqlx::query_as::<_, (DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32, String)>(
        r#"
        SELECT sl.application_id, sl.permission_level, sl.expires_at, sl.max_uses, sl.use_count, a.name
        FROM app_share_links sl
        JOIN applications a ON a.id = sl.application_id
        WHERE sl.token = $1 AND sl.is_active = 1
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
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

// ============================================================================
// Database-specific helper functions
// ============================================================================

#[cfg(feature = "postgres")]
async fn search_users_by_pattern(
    db: &crate::db::DbPool,
    org_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<(DbUuid, String, Option<String>, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
        r#"
        SELECT id, email, display_name, role
        FROM users
        WHERE organization_id = $1 AND is_active = true
          AND (email ILIKE $2 OR display_name ILIKE $2)
        ORDER BY display_name, email
        LIMIT $3
        "#,
    )
    .bind(org_id)
    .bind(pattern)
    .bind(limit)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn search_users_by_pattern(
    db: &crate::db::DbPool,
    org_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<(DbUuid, String, Option<String>, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
        r#"
        SELECT id, email, display_name, role
        FROM users
        WHERE organization_id = $1 AND is_active = 1
          AND (email LIKE $2 OR display_name LIKE $2)
        ORDER BY display_name, email
        LIMIT $3
        "#,
    )
    .bind(org_id)
    .bind(pattern)
    .bind(limit)
    .fetch_all(db)
    .await
}
