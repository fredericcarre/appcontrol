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

/// Well-known UUIDs for development (deterministic for easier testing).
const DEV_ORG_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEV_ADMIN_ID: &str = "00000000-0000-0000-0000-000000000002";
const DEV_OPERATOR_ID: &str = "00000000-0000-0000-0000-000000000003";
const DEV_VIEWER_ID: &str = "00000000-0000-0000-0000-000000000004";

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
/// Creates the dev organization and users on first call, then returns a JWT.
pub async fn dev_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DevLoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Refuse in production
    if state.config.app_env == "production" {
        tracing::warn!("Dev login attempted in production — rejected");
        return Err(StatusCode::FORBIDDEN);
    }

    let org_id: Uuid = DEV_ORG_ID.parse().unwrap();
    let (user_id, role) = match req.username.as_str() {
        "admin" => (DEV_ADMIN_ID.parse::<Uuid>().unwrap(), "admin"),
        "operator" => (DEV_OPERATOR_ID.parse::<Uuid>().unwrap(), "operator"),
        "viewer" => (DEV_VIEWER_ID.parse::<Uuid>().unwrap(), "viewer"),
        _ => {
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Ensure dev org exists
    let _ = sqlx::query(
        r#"
        INSERT INTO organizations (id, name, slug)
        VALUES ($1, 'Development Org', 'dev')
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(org_id)
    .execute(&state.db)
    .await;

    // Ensure dev users exist
    for (uid, uname, urole) in [
        (DEV_ADMIN_ID, "admin", "admin"),
        (DEV_OPERATOR_ID, "operator", "operator"),
        (DEV_VIEWER_ID, "viewer", "viewer"),
    ] {
        let uid: Uuid = uid.parse().unwrap();
        let _ = sqlx::query(
            r#"
            INSERT INTO users (id, organization_id, external_id, email, display_name, role)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(uid)
        .bind(org_id)
        .bind(uname)
        .bind(format!("{}@dev.local", uname))
        .bind(uname)
        .bind(urole)
        .execute(&state.db)
        .await;
    }

    // Generate JWT
    let token = jwt::create_token(
        user_id,
        org_id,
        &format!("{}@dev.local", req.username),
        role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!(
        username = %req.username,
        role = %role,
        "Dev login successful"
    );

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
/// Accepts email like "admin@dev.local" and ignores password.
/// Returns token + user info in the format expected by the frontend.
pub async fn email_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmailLoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Refuse in production
    if state.config.app_env == "production" {
        tracing::warn!("Dev email login attempted in production — rejected");
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"message": "Dev login disabled in production"})),
        ));
    }

    // Extract username from email (admin@dev.local → admin)
    let username = req.email.split('@').next().unwrap_or("");

    let org_id: Uuid = DEV_ORG_ID.parse().unwrap();
    let (user_id, role, display_name) = match username {
        "admin" => (DEV_ADMIN_ID.parse::<Uuid>().unwrap(), "admin", "Admin"),
        "operator" => (DEV_OPERATOR_ID.parse::<Uuid>().unwrap(), "operator", "Operator"),
        "viewer" => (DEV_VIEWER_ID.parse::<Uuid>().unwrap(), "viewer", "Viewer"),
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"message": "Invalid credentials. Use admin@dev.local, operator@dev.local, or viewer@dev.local"})),
            ));
        }
    };

    // Ensure dev org exists
    let _ = sqlx::query(
        r#"
        INSERT INTO organizations (id, name, slug)
        VALUES ($1, 'Development Org', 'dev')
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(org_id)
    .execute(&state.db)
    .await;

    // Ensure dev users exist
    for (uid, uname, urole) in [
        (DEV_ADMIN_ID, "admin", "admin"),
        (DEV_OPERATOR_ID, "operator", "operator"),
        (DEV_VIEWER_ID, "viewer", "viewer"),
    ] {
        let uid: Uuid = uid.parse().unwrap();
        let _ = sqlx::query(
            r#"
            INSERT INTO users (id, organization_id, external_id, email, display_name, role)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(uid)
        .bind(org_id)
        .bind(uname)
        .bind(format!("{}@dev.local", uname))
        .bind(uname)
        .bind(urole)
        .execute(&state.db)
        .await;
    }

    // Generate JWT
    let token = jwt::create_token(
        user_id,
        org_id,
        &req.email,
        role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"message": "Failed to create token"})),
        )
    })?;

    tracing::info!(
        email = %req.email,
        role = %role,
        "Dev email login successful"
    );

    Ok(Json(EmailLoginResponse {
        token,
        user: EmailLoginUser {
            id: user_id.to_string(),
            email: req.email,
            name: display_name.to_string(),
            role: role.to_string(),
            org_id: org_id.to_string(),
            org_name: "Development Org".to_string(),
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
