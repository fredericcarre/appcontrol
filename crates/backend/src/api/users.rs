//! User management API.
//!
//! Org admins can create local users, list users, update roles, and deactivate users.
//! Users created via OIDC/SAML are auto-provisioned on first login and appear here too.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: String,
    /// Role: admin, operator, editor, viewer
    pub role: Option<String>,
    /// Password for local auth (will be hashed with bcrypt)
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub is_active: Option<bool>,
    /// New password (will be hashed with bcrypt)
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub role: Option<String>,
    pub is_active: Option<bool>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub auth_provider: String,
    pub is_active: bool,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const VALID_ROLES: [&str; 4] = ["admin", "operator", "editor", "viewer"];

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let users = sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users
           WHERE organization_id = $1
             AND ($2::text IS NULL OR role = $2)
             AND ($3::bool IS NULL OR is_active = $3)
             AND ($4::text IS NULL OR email ILIKE '%' || $4 || '%' OR display_name ILIKE '%' || $4 || '%')
           ORDER BY display_name"#,
    )
    .bind(user.organization_id)
    .bind(&query.role)
    .bind(query.is_active)
    .bind(&query.search)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "users": users })))
}

pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() && user.user_id != id {
        return Err(ApiError::Forbidden);
    }

    let target = sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users
           WHERE id = $1 AND organization_id = $2"#,
    )
    .bind(id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(target)))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_length("email", &req.email, 3, 300)?;
    validate_length("display_name", &req.display_name, 1, 200)?;

    let role = req.role.as_deref().unwrap_or("viewer");
    if !VALID_ROLES.contains(&role) {
        return Err(ApiError::Validation(
            "role must be one of: admin, operator, editor, viewer".to_string(),
        ));
    }

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_user",
        "user",
        Uuid::nil(),
        json!({ "email": &req.email, "role": role }),
    )
    .await
    .ok();

    let external_id = format!("local-{}", req.email);

    // Hash password if provided
    let password_hash = if let Some(ref password) = req.password {
        if password.len() < 4 {
            return Err(ApiError::Validation(
                "Password must be at least 4 characters".to_string(),
            ));
        }
        Some(
            bcrypt::hash(password, bcrypt::DEFAULT_COST)
                .map_err(|_| ApiError::Internal("Failed to hash password".to_string()))?,
        )
    } else {
        None
    };

    let new_user = sqlx::query_as::<_, UserRow>(
        r#"INSERT INTO users (organization_id, external_id, email, display_name, role, auth_provider, password_hash)
           VALUES ($1, $2, $3, $4, $5, 'local', $6)
           RETURNING id, organization_id, email, display_name, role, auth_provider,
                     is_active, last_login_at, created_at"#,
    )
    .bind(user.organization_id)
    .bind(&external_id)
    .bind(&req.email)
    .bind(&req.display_name)
    .bind(role)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!(new_user)))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_optional_length("display_name", &req.display_name, 200)?;

    if let Some(ref role) = req.role {
        if !VALID_ROLES.contains(&role.as_str()) {
            return Err(ApiError::Validation(
                "role must be one of: admin, operator, editor, viewer".to_string(),
            ));
        }
    }

    // Hash password if provided
    let password_hash = if let Some(ref password) = req.password {
        if password.len() < 4 {
            return Err(ApiError::Validation(
                "Password must be at least 4 characters".to_string(),
            ));
        }
        Some(
            bcrypt::hash(password, bcrypt::DEFAULT_COST)
                .map_err(|_| ApiError::Internal("Failed to hash password".to_string()))?,
        )
    } else {
        None
    };

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_user",
        "user",
        id,
        json!({ "role": &req.role, "is_active": req.is_active, "password_changed": req.password.is_some() }),
    )
    .await
    .ok();

    let updated = sqlx::query_as::<_, UserRow>(
        r#"UPDATE users SET
               display_name = COALESCE($3, display_name),
               role = COALESCE($4, role),
               is_active = COALESCE($5, is_active),
               password_hash = COALESCE($6, password_hash)
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, email, display_name, role, auth_provider,
                     is_active, last_login_at, created_at"#,
    )
    .bind(id)
    .bind(user.organization_id)
    .bind(&req.display_name)
    .bind(&req.role)
    .bind(req.is_active)
    .bind(&password_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(updated)))
}

/// GET /api/v1/users/me — Get current user info
pub async fn get_me(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let me = sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users WHERE id = $1"#,
    )
    .bind(user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    // Also fetch platform_role
    let platform_role: Option<String> =
        sqlx::query_scalar("SELECT platform_role FROM users WHERE id = $1")
            .bind(user.user_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();

    Ok(Json(json!({
        "user": me,
        "platform_role": platform_role,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_roles() {
        assert!(VALID_ROLES.contains(&"admin"));
        assert!(VALID_ROLES.contains(&"viewer"));
        assert!(!VALID_ROLES.contains(&"superadmin"));
    }
}
