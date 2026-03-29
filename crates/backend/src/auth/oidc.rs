//! OIDC (OpenID Connect) authentication module.
//!
//! Implements the Authorization Code Flow for OIDC providers (Keycloak, Azure AD, Okta, etc.).
//! The flow:
//! 1. Frontend redirects to `/api/v1/auth/oidc/login` → redirect to provider
//! 2. Provider authenticates user → redirects back to `/api/v1/auth/oidc/callback`
//! 3. Backend exchanges auth code for tokens, extracts user info, creates/updates user, returns JWT

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::AuthUser;
use crate::db::DbUuid;
use crate::AppState;

/// OIDC provider configuration.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// OIDC discovery URL (e.g., https://keycloak.example.com/realms/appcontrol/.well-known/openid-configuration)
    pub discovery_url: String,
    /// Client ID registered with the OIDC provider
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Redirect URI (must match provider config)
    pub redirect_uri: String,
    /// Scopes to request
    pub scopes: Vec<String>,
}

impl OidcConfig {
    /// Load OIDC configuration from environment variables.
    pub fn from_env() -> Option<Self> {
        let discovery_url = std::env::var("OIDC_DISCOVERY_URL").ok()?;
        let client_id = std::env::var("OIDC_CLIENT_ID").ok()?;
        let client_secret = std::env::var("OIDC_CLIENT_SECRET").ok()?;
        let redirect_uri = std::env::var("OIDC_REDIRECT_URI")
            .unwrap_or_else(|_| "/api/v1/auth/oidc/callback".to_string());
        let scopes = std::env::var("OIDC_SCOPES")
            .unwrap_or_else(|_| "openid,profile,email".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        Some(Self {
            discovery_url,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
        })
    }
}

/// OIDC discovery document (subset of fields we need).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    issuer: String,
}

/// Token response from OIDC provider.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    id_token: Option<String>,
    refresh_token: Option<String>,
}

/// User info from OIDC provider.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct UserInfo {
    sub: String,
    email: Option<String>,
    name: Option<String>,
    preferred_username: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OidcCallbackQuery {
    pub code: String,
    pub state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OidcLoginResponse {
    pub token: String,
    pub user: AuthUser,
}

/// GET /api/v1/auth/oidc/login — Redirect to OIDC provider.
pub async fn oidc_login(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    let oidc = state
        .config
        .oidc
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    let scopes = oidc.scopes.join(" ");
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
        discover_authorization_endpoint(&oidc.discovery_url)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?,
        urlencoding::encode(&oidc.client_id),
        urlencoding::encode(&oidc.redirect_uri),
        urlencoding::encode(&scopes),
        Uuid::new_v4(), // CSRF state parameter
    );

    Ok(Redirect::temporary(&auth_url))
}

/// GET /api/v1/auth/oidc/callback — Exchange authorization code for tokens.
/// Sets an HttpOnly cookie with the JWT token for browser security.
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OidcCallbackQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let oidc = state
        .config
        .oidc
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Discover token endpoint
    let discovery = discover(&oidc.discovery_url)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let token_resp = client
        .post(&discovery.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &query.code),
            ("redirect_uri", &oidc.redirect_uri),
            ("client_id", &oidc.client_id),
            ("client_secret", &oidc.client_secret),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json::<TokenResponse>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    // Fetch user info
    let user_info = client
        .get(&discovery.userinfo_endpoint)
        .bearer_auth(&token_resp.access_token)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json::<UserInfo>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let email = user_info.email.unwrap_or_else(|| user_info.sub.clone());
    let name = user_info
        .name
        .or(user_info.preferred_username)
        .unwrap_or_else(|| email.clone());

    // Find or create user in our database
    let auth_user = find_or_create_oidc_user(&state.db, &email, &name, &user_info.sub)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Generate our own JWT
    let jwt_token = super::jwt::create_token(
        auth_user.user_id,
        auth_user.organization_id,
        &auth_user.email,
        &auth_user.role,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Set HttpOnly cookie for browser security (no localStorage exposure)
    let is_production = state.config.app_env == "production";
    let cookie = crate::middleware::auth::build_auth_cookie(&jwt_token, is_production);

    let response = (
        [(axum::http::header::SET_COOKIE, cookie)],
        Json(OidcLoginResponse {
            token: jwt_token,
            user: auth_user,
        }),
    );

    Ok(response)
}

/// Discover OIDC endpoints from the discovery URL.
async fn discover(discovery_url: &str) -> Result<OidcDiscovery, anyhow::Error> {
    let client = reqwest::Client::new();
    let discovery: OidcDiscovery = client.get(discovery_url).send().await?.json().await?;
    Ok(discovery)
}

/// Extract just the authorization endpoint from discovery.
async fn discover_authorization_endpoint(discovery_url: &str) -> Result<String, anyhow::Error> {
    let d = discover(discovery_url).await?;
    Ok(d.authorization_endpoint)
}

/// Find existing user by OIDC subject or create a new one.
async fn find_or_create_oidc_user(
    pool: &crate::db::DbPool,
    email: &str,
    display_name: &str,
    oidc_sub: &str,
) -> Result<AuthUser, sqlx::Error> {
    // Try to find by email first
    let existing = sqlx::query_as::<_, (DbUuid, DbUuid, String, String)>(
        "SELECT id, organization_id, email, role FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    if let Some((user_id, org_id, email, role)) = existing {
        // Update OIDC subject if not set
        let _ = sqlx::query("UPDATE users SET oidc_sub = $1 WHERE id = $2 AND oidc_sub IS NULL")
            .bind(oidc_sub)
            .bind(user_id)
            .execute(pool)
            .await;

        return Ok(AuthUser {
            user_id: *user_id,
            organization_id: *org_id,
            email,
            role,
        });
    }

    // Auto-create user in default organization
    let org_id = sqlx::query_scalar::<_, DbUuid>("SELECT id FROM organizations LIMIT 1")
        .fetch_one(pool)
        .await?;

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, oidc_sub)
         VALUES ($1, $2, $3, $4, $5, 'viewer', $6)",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("oidc:{oidc_sub}"))
    .bind(email)
    .bind(display_name)
    .bind(oidc_sub)
    .execute(pool)
    .await?;

    Ok(AuthUser {
        user_id,
        organization_id: *org_id,
        email: email.to_string(),
        role: "viewer".to_string(),
    })
}

/// OIDC routes for the router.
pub fn oidc_routes() -> axum::Router<Arc<AppState>> {
    use axum::routing::get;
    axum::Router::new()
        .route("/auth/oidc/login", get(oidc_login))
        .route("/auth/oidc/callback", get(oidc_callback))
}
