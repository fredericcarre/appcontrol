//! Local authentication module.
//!
//! Supports two modes:
//! - **demo**: Predefined users (admin/operator/viewer), no password verification
//! - **local**: Users in database with bcrypt password hashes
//!
//! ## Usage
//!
//! ```bash
//! # Demo mode (default in development)
//! curl -X POST http://localhost:3000/api/v1/auth/login \
//!   -H "Content-Type: application/json" \
//!   -d '{"email": "admin@local", "password": "anything"}'
//!
//! # Local mode (default in production)
//! curl -X POST http://localhost:3000/api/v1/auth/login \
//!   -H "Content-Type: application/json" \
//!   -d '{"email": "admin@local", "password": "admin"}'
//! ```

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{auth::jwt, config::AuthMode, AppState};

// All seed data comes from SEED_* environment variables via config::SeedConfig.
// No hardcoded users, organizations, or credentials in this module.

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: LoginUser,
}

#[derive(Debug, Serialize)]
pub struct LoginUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub org_id: String,
    pub org_name: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub message: String,
}

/// POST /api/v1/auth/login — Email/password login.
///
/// Behavior depends on AUTH_MODE:
/// - demo: accepts predefined users, ignores password
/// - local: verifies password against bcrypt hash in database
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.config.auth_mode {
        AuthMode::Demo => demo_login(&state, &req).await,
        AuthMode::Local => local_login(&state, &req).await,
        AuthMode::Oidc | AuthMode::Saml => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "Password login disabled. Use SSO.".to_string(),
            }),
        )),
    }
}

/// Demo mode login - looks up user by email, no password check.
/// Users must be seeded at startup via SEED_* config.
async fn demo_login(
    state: &AppState,
    req: &LoginRequest,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    #[cfg(feature = "postgres")]
    let is_active_check: &str = "u.is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let is_active_check: &str = "u.is_active = 1";

    let user: Option<(Uuid, Uuid, String, String, String)> = sqlx::query_as(
        &format!(
            "SELECT u.id, u.organization_id, u.display_name, u.role, o.name \
             FROM users u JOIN organizations o ON o.id = u.organization_id \
             WHERE u.email = $1 AND {is_active_check}"
        ),
    )
    .bind(&req.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error during demo login: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { message: "Internal error".to_string() }))
    })?;

    let (user_id, org_id, display_name, role, org_name) = match user {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    message: "Invalid credentials. Check SEED_ADMIN_EMAIL in your configuration.".to_string(),
                }),
            ));
        }
    };

    create_login_response(state, user_id, org_id, &org_name, &req.email, &display_name, &role)
}

/// Local mode login - verify password against bcrypt hash
async fn local_login(
    state: &AppState,
    req: &LoginRequest,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Look up user by email with organization name
    let user: Option<(Uuid, Uuid, String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT u.id, u.organization_id, u.display_name, u.role, o.name as org_name, u.password_hash
        FROM users u
        JOIN organizations o ON o.id = u.organization_id
        WHERE u.email = $1 AND u.auth_provider = 'local'
        "#,
    )
    .bind(&req.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error during login: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                message: "Internal error".to_string(),
            }),
        )
    })?;

    let (user_id, org_id, display_name, role, org_name, password_hash) = match user {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    message: "Invalid email or password".to_string(),
                }),
            ));
        }
    };

    // Verify password
    let hash = password_hash.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                message: "Password login not configured for this user".to_string(),
            }),
        )
    })?;

    let password_valid = bcrypt::verify(&req.password, &hash).unwrap_or(false);
    if !password_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                message: "Invalid email or password".to_string(),
            }),
        ));
    }

    create_login_response(state, user_id, org_id, &org_name, &req.email, &display_name, &role)
}

/// Create JWT and login response
fn create_login_response(
    state: &AppState,
    user_id: Uuid,
    org_id: Uuid,
    org_name: &str,
    email: &str,
    display_name: &str,
    role: &str,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    let token = jwt::create_token(
        user_id,
        org_id,
        email,
        role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                message: "Failed to create token".to_string(),
            }),
        )
    })?;

    tracing::info!(email = %email, role = %role, "Login successful");

    Ok(Json(LoginResponse {
        token,
        user: LoginUser {
            id: user_id.to_string(),
            email: email.to_string(),
            name: display_name.to_string(),
            role: role.to_string(),
            org_id: org_id.to_string(),
            org_name: org_name.to_string(),
        },
    }))
}

/// GET /api/v1/auth/mode — Return current auth mode
pub async fn auth_mode(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "mode": format!("{:?}", state.config.auth_mode).to_lowercase(),
        "sso_enabled": matches!(state.config.auth_mode, AuthMode::Oidc | AuthMode::Saml),
        "local_login_enabled": matches!(state.config.auth_mode, AuthMode::Demo | AuthMode::Local),
    }))
}

/// GET /api/v1/auth/users — List available users (demo mode only)
pub async fn demo_users(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, StatusCode> {
    if state.config.auth_mode != AuthMode::Demo {
        return Err(StatusCode::NOT_FOUND);
    }

    // Return users from DB instead of hardcoded list
    #[cfg(feature = "postgres")]
    let q = "SELECT email, role FROM users WHERE is_active = true ORDER BY role";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let q = "SELECT email, role FROM users WHERE is_active = 1 ORDER BY role";

    let users: Vec<(String, String)> = sqlx::query_as(q)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user_list: Vec<serde_json::Value> = users.into_iter().map(|(email, role)| {
        serde_json::json!({"email": email, "role": role})
    }).collect();

    Ok(Json(serde_json::json!({ "users": user_list })))
}

/// Local auth routes
pub fn local_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/mode", get(auth_mode))
        .route("/auth/users", get(demo_users))
}
