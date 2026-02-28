pub mod api_key;
pub mod jwt;
pub mod oidc;
pub mod saml;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Authenticated user context extracted from JWT or API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub email: String,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Serialize)]
pub struct LoginError {
    pub message: String,
}

/// POST /api/v1/auth/login — Email + password login.
///
/// Verifies credentials against bcrypt hash in database.
/// Always available (local admin fallback even when SSO is configured).
pub async fn login(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::Json(body): axum::Json<LoginRequest>,
) -> Result<impl axum::response::IntoResponse, (axum::http::StatusCode, axum::Json<LoginError>)> {
    // Look up user by email with password hash
    let row: Option<(Uuid, Uuid, String, String, Option<String>)> = sqlx::query_as(
        r#"SELECT id, organization_id, email, role, password_hash
           FROM users WHERE email = $1 AND is_active = true"#,
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error during login: {}", e);
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(LoginError {
                message: "Internal error".to_string(),
            }),
        )
    })?;

    let (user_id, org_id, email, role, password_hash) = row.ok_or_else(|| {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(LoginError {
                message: "Invalid email or password".to_string(),
            }),
        )
    })?;

    // Verify password
    let hash = password_hash.ok_or_else(|| {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(LoginError {
                message: "Password login not configured for this user".to_string(),
            }),
        )
    })?;

    let password_valid = bcrypt::verify(&body.password, &hash).unwrap_or(false);
    if !password_valid {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(LoginError {
                message: "Invalid email or password".to_string(),
            }),
        ));
    }

    // Create JWT
    let jwt_token = jwt::create_token(
        user_id,
        org_id,
        &email,
        &role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|e| {
        tracing::error!("Failed to create JWT: {}", e);
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(LoginError {
                message: "Failed to create token".to_string(),
            }),
        )
    })?;

    let is_production = state.config.app_env == "production";
    let cookie = crate::middleware::auth::build_auth_cookie(&jwt_token, is_production);

    tracing::info!(email = %email, role = %role, "Login successful");

    Ok((
        [(axum::http::header::SET_COOKIE, cookie)],
        axum::Json(LoginResponse {
            token: jwt_token,
            user: AuthUser {
                user_id,
                organization_id: org_id,
                email,
                role,
            },
        }),
    ))
}

/// GET /api/v1/auth/info — Public endpoint returning auth configuration.
///
/// The frontend uses this to know which login methods are available.
///
/// Returns:
/// - `local`: true (always available for admin fallback)
/// - `oidc`: true if OIDC is configured
/// - `saml`: true if SAML is configured
pub async fn auth_info(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({
        "local": true,
        "oidc": state.config.oidc.is_some(),
        "saml": state.config.saml.is_some(),
    }))
}

/// Auth routes (no auth middleware — these ARE login/info endpoints).
pub fn auth_routes() -> axum::Router<std::sync::Arc<crate::AppState>> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/auth/login", post(login))
        .route("/auth/info", get(auth_info))
}
