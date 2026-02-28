//! Development-only authentication module.
//!
//! Provides a simple login endpoint for local development and testing.
//! **DISABLED** when APP_ENV=production.
//!
//! ## Usage
//!
//! ```bash
//! # Login and get a token
//! TOKEN=$(curl -s -X POST http://localhost:3000/api/v1/auth/dev/login \
//!   -H "Content-Type: application/json" \
//!   -d '{"username": "admin"}' | jq -r '.token')
//!
//! # Use the token
//! curl -H "Authorization: Bearer $TOKEN" http://localhost:3000/api/v1/apps
//! ```

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{auth::jwt, AppState};

// All seed data comes from SEED_* environment variables via config::SeedConfig.
// No hardcoded users, organizations, or credentials in this module.

#[derive(Debug, Deserialize)]
pub struct DevLoginRequest {
    /// Username: "admin", "operator", or "viewer"
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct DevLoginResponse {
    pub token: String,
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub role: String,
    pub expires_in: u64,
}

/// POST /api/v1/auth/dev/login — Development-only login.
///
/// Looks up user by username (role) from the database. Users must be seeded
/// at startup via SEED_* config — this endpoint does NOT create users.
pub async fn dev_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DevLoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if state.config.app_env == "production" {
        tracing::warn!("Dev login attempted in production — rejected");
        return Err(StatusCode::FORBIDDEN);
    }

    // Look up user by role name (admin, operator, viewer)
    let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.organization_id, u.email, u.role FROM users u WHERE u.role = $1 AND u.is_active = true LIMIT 1",
    )
    .bind(&req.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (user_id, org_id, email, role) = row.ok_or(StatusCode::UNAUTHORIZED)?;

    let token = jwt::create_token(
        user_id,
        org_id,
        &email,
        &role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!(username = %req.username, role = %role, "Dev login successful");

    Ok(Json(DevLoginResponse {
        token,
        user_id,
        organization_id: org_id,
        role: role.to_string(),
        expires_in: 86400,
    }))
}

/// GET /api/v1/auth/dev/users — List available dev users.
pub async fn dev_users(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, StatusCode> {
    if state.config.app_env == "production" {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(serde_json::json!({
        "users": [
            {"username": "admin", "role": "admin", "description": "Full access to all features"},
            {"username": "operator", "role": "operator", "description": "Can operate apps (start/stop) but not edit"},
            {"username": "viewer", "role": "viewer", "description": "Read-only access"}
        ],
        "usage": "POST /api/v1/auth/dev/login with {\"username\": \"admin\"}"
    })))
}

/// Request for email/password login (used by frontend).
#[derive(Debug, Deserialize)]
pub struct EmailLoginRequest {
    pub email: String,
    #[allow(dead_code)]
    pub password: String, // Ignored in dev mode
}

/// Response for email login (matches frontend expectations).
#[derive(Debug, Serialize)]
pub struct EmailLoginResponse {
    pub token: String,
    pub user: EmailLoginUser,
}

#[derive(Debug, Serialize)]
pub struct EmailLoginUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub org_id: String,
    pub org_name: String,
}

/// POST /api/v1/auth/login — Email/password login (dev mode only).
///
/// Looks up user by email in the database. No hardcoded users — all come from SEED_* config.
pub async fn email_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmailLoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.config.app_env == "production" {
        tracing::warn!("Dev email login attempted in production — rejected");
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"message": "Dev login disabled in production"})),
        ));
    }

    // Look up user by email
    let row: Option<(Uuid, Uuid, String, String, String)> = sqlx::query_as(
        r#"SELECT u.id, u.organization_id, u.display_name, u.role, o.name
           FROM users u JOIN organizations o ON o.id = u.organization_id
           WHERE u.email = $1 AND u.is_active = true"#,
    )
    .bind(&req.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error during login: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"message": "Internal error"})))
    })?;

    let (user_id, org_id, display_name, role, org_name) = row.ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"message": "Invalid credentials"})))
    })?;

    let token = jwt::create_token(
        user_id,
        org_id,
        &req.email,
        &role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"message": "Failed to create token"})))
    })?;

    tracing::info!(email = %req.email, role = %role, "Dev email login successful");

    Ok(Json(EmailLoginResponse {
        token,
        user: EmailLoginUser {
            id: user_id.to_string(),
            email: req.email,
            name: display_name,
            role,
            org_id: org_id.to_string(),
            org_name,
        },
    }))
}

/// Development auth routes (only active when APP_ENV != production).
pub fn dev_routes() -> Router<Arc<AppState>> {
    use axum::routing::get;
    Router::new()
        .route("/auth/dev/login", post(dev_login))
        .route("/auth/dev/users", get(dev_users))
        .route("/auth/login", post(email_login))
}
