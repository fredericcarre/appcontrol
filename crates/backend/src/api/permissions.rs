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
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

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
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(StatusCode::FORBIDDEN);
    }

    let perms = sqlx::query_as::<_, (Uuid, Uuid, String, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"
        SELECT apu.id, apu.user_id, apu.permission_level, apu.expires_at
        FROM app_permissions_users apu
        WHERE apu.application_id = $1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let permissions: Vec<Value> = perms
        .iter()
        .map(|(id, uid, level, exp)| json!({"id": id, "user_id": uid, "permission_level": level, "expires_at": exp}))
        .collect();

    Ok(Json(json!({ "permissions": permissions })))
}

pub async fn grant_user_permission(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<GrantUserPermissionRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "grant_permission", "application", app_id,
        json!({"target_user": body.user_id, "level": body.permission_level}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (application_id, user_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = now()
        RETURNING id
        "#,
    )
    .bind(app_id)
    .bind(body.user_id)
    .bind(&body.permission_level)
    .bind(user.user_id)
    .bind(body.expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!({"id": id, "status": "granted"}))))
}

pub async fn list_team_permissions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(StatusCode::FORBIDDEN);
    }

    let perms = sqlx::query_as::<_, (Uuid, Uuid, String, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"
        SELECT apt.id, apt.team_id, apt.permission_level, apt.expires_at
        FROM app_permissions_teams apt
        WHERE apt.application_id = $1
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "grant_team_permission", "application", app_id,
        json!({"team_id": body.team_id, "level": body.permission_level}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO app_permissions_teams (application_id, team_id, permission_level, granted_by, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (application_id, team_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = now()
        RETURNING id
        "#,
    )
    .bind(app_id)
    .bind(body.team_id)
    .bind(&body.permission_level)
    .bind(user.user_id)
    .bind(body.expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!({"id": id, "status": "granted"}))))
}

pub async fn list_share_links(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(StatusCode::FORBIDDEN);
    }

    let links = sqlx::query_as::<_, (Uuid, String, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>(
        r#"
        SELECT id, token, permission_level, expires_at, max_uses, use_count
        FROM app_share_links
        WHERE application_id = $1 AND is_active = true
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(StatusCode::FORBIDDEN);
    }

    let token = uuid::Uuid::new_v4().to_string().replace('-', "");

    log_action(&state.db, user.user_id, "create_share_link", "application", app_id, json!({"level": body.permission_level}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = sqlx::query_scalar::<_, Uuid>(
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
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!({"id": id, "token": token}))))
}

pub async fn get_effective_permission(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    Ok(Json(json!({ "permission_level": format!("{:?}", perm).to_lowercase() })))
}
