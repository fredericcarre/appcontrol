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

// ── Dev login (development mode only) ──

/// POST /api/v1/auth/dev-login — Simple email-based login for local development.
///
/// Only available when `APP_ENV=development` (the default). Looks up the user
/// by email and returns a JWT token. No password required — this is strictly
/// for quickstart / local dev convenience.
pub async fn dev_login(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    axum::Json(body): axum::Json<DevLoginRequest>,
) -> Result<impl axum::response::IntoResponse, axum::http::StatusCode> {
    if state.config.app_env != "development" {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }

    let row: (Uuid, Uuid, String, String) = sqlx::query_as(
        "SELECT id, organization_id, email, role FROM users WHERE email = $1 AND is_active = true",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let (user_id, org_id, email, role) = row;

    let jwt_token = jwt::create_token(
        user_id,
        org_id,
        &email,
        &role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let is_production = false; // dev-login only works in development mode
    let cookie = crate::middleware::auth::build_auth_cookie(&jwt_token, is_production);

    Ok((
        [(axum::http::header::SET_COOKIE, cookie)],
        axum::Json(DevLoginResponse {
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

#[derive(Debug, Deserialize)]
pub struct DevLoginRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct DevLoginResponse {
    pub token: String,
    pub user: AuthUser,
}

/// Dev login route (no auth middleware — this IS a login endpoint).
pub fn dev_login_routes() -> axum::Router<std::sync::Arc<crate::AppState>> {
    use axum::routing::post;
    axum::Router::new().route("/auth/dev-login", post(dev_login))
}
