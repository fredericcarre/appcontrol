use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::auth::{jwt, AuthUser};
use crate::AppState;

/// Cookie name for HttpOnly JWT token.
pub const AUTH_COOKIE_NAME: &str = "appcontrol_token";

/// Middleware that extracts the authenticated user from:
/// 1. HttpOnly cookie (preferred — secure, no XSS exposure)
/// 2. Authorization: Bearer header (API clients, CLI)
/// 3. Authorization: ApiKey header (scheduler integrations)
///
/// Checks token revocation against PostgreSQL (no Redis needed).
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Try to extract token from multiple sources (in priority order)
    let token_source = extract_token(&request);

    let auth_user = match token_source {
        TokenSource::Cookie(token) | TokenSource::Bearer(token) => {
            let claims =
                jwt::validate_token(&token, &state.config.jwt_secret, &state.config.jwt_issuer)
                    .map_err(|_| StatusCode::UNAUTHORIZED)?;

            // Check token revocation against PostgreSQL
            if is_token_revoked(&state.db, &token).await {
                tracing::warn!(email = %claims.email, "Revoked token used");
                return Err(StatusCode::UNAUTHORIZED);
            }

            let user_id: uuid::Uuid = claims.sub.parse().map_err(|_| StatusCode::UNAUTHORIZED)?;
            let org_id: uuid::Uuid = claims.org.parse().map_err(|_| StatusCode::UNAUTHORIZED)?;

            AuthUser {
                user_id: crate::db::DbUuid::from(user_id),
                organization_id: crate::db::DbUuid::from(org_id),
                email: claims.email,
                role: claims.role,
            }
        }
        TokenSource::ApiKey(key) => crate::auth::api_key::validate_api_key(&state.db, &key)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?,
        TokenSource::None => return Err(StatusCode::UNAUTHORIZED),
    };

    request.extensions_mut().insert(auth_user);
    Ok(next.run(request).await)
}

/// Token extraction sources.
enum TokenSource {
    Cookie(String),
    Bearer(String),
    ApiKey(String),
    None,
}

/// Extract authentication token from request (cookie, header, or API key).
fn extract_token(request: &Request) -> TokenSource {
    // 1. Try HttpOnly cookie first (most secure for browser clients)
    if let Some(cookie_header) = request.headers().get(header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix(&format!("{}=", AUTH_COOKIE_NAME)) {
                    return TokenSource::Cookie(token.to_string());
                }
            }
        }
    }

    // 2. Try Authorization header
    if let Some(auth_header) = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            return TokenSource::Bearer(token.to_string());
        }
        if let Some(key) = auth_header.strip_prefix("ApiKey ") {
            return TokenSource::ApiKey(key.to_string());
        }
    }

    TokenSource::None
}

/// Check if a token has been revoked (stored in PostgreSQL revoked_tokens table).
/// Public version for use by ws-token endpoint.
pub async fn is_token_revoked_public(pool: &crate::db::DbPool, token: &str) -> bool {
    is_token_revoked(pool, token).await
}

/// Check if a token has been revoked (stored in revoked_tokens table).
#[cfg(feature = "postgres")]
async fn is_token_revoked(pool: &crate::db::DbPool, token: &str) -> bool {
    let fingerprint = token_fingerprint(token);

    match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM revoked_tokens WHERE fingerprint = $1 AND expires_at > now())",
    )
    .bind(&fingerprint)
    .fetch_one(pool)
    .await
    {
        Ok(exists) => exists,
        Err(e) => {
            tracing::warn!("Token revocation check failed: {} — allowing token", e);
            false // Fail open: if DB check fails, allow the token
        }
    }
}

/// Check if a token has been revoked (SQLite version).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn is_token_revoked(pool: &crate::db::DbPool, token: &str) -> bool {
    let fingerprint = token_fingerprint(token);

    // SQLite uses datetime('now') instead of now()
    match sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM revoked_tokens WHERE fingerprint = $1 AND expires_at > datetime('now')",
    )
    .bind(&fingerprint)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count > 0,
        Err(e) => {
            tracing::warn!("Token revocation check failed: {} — allowing token", e);
            false // Fail open: if DB check fails, allow the token
        }
    }
}

/// Revoke a token by adding it to the revoked_tokens table.
/// The entry expires when the token would have expired (max 24h + 1h buffer).
#[cfg(feature = "postgres")]
pub async fn revoke_token(pool: &crate::db::DbPool, token: &str) -> Result<(), String> {
    let fingerprint = token_fingerprint(token);
    // Token expires in 24h max, so set expiry to 24h + buffer
    let ttl_secs: i64 = 86400 + 3600; // 25 hours

    sqlx::query(
        "INSERT INTO revoked_tokens (fingerprint, expires_at) VALUES ($1, now() + $2 * interval '1 second') ON CONFLICT (fingerprint) DO NOTHING",
    )
    .bind(&fingerprint)
    .bind(ttl_secs)
    .execute(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    tracing::info!("Token revoked (fingerprint={})", fingerprint);
    Ok(())
}

/// Revoke a token (SQLite version).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn revoke_token(pool: &crate::db::DbPool, token: &str) -> Result<(), String> {
    let fingerprint = token_fingerprint(token);
    // Token expires in 24h max, so set expiry to 24h + buffer
    let ttl_secs: i64 = 86400 + 3600; // 25 hours

    // SQLite uses datetime() with modifier instead of interval arithmetic
    sqlx::query(
        "INSERT OR IGNORE INTO revoked_tokens (fingerprint, expires_at) VALUES ($1, datetime('now', '+' || $2 || ' seconds'))",
    )
    .bind(&fingerprint)
    .bind(ttl_secs)
    .execute(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    tracing::info!("Token revoked (fingerprint={})", fingerprint);
    Ok(())
}

/// Cleanup expired revocation entries (called periodically from background task).
#[cfg(feature = "postgres")]
pub async fn cleanup_expired_revocations(pool: &crate::db::DbPool) {
    match sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < now()")
        .execute(pool)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => {
            tracing::debug!(
                cleaned = result.rows_affected(),
                "Cleaned expired token revocations"
            );
        }
        Err(e) => {
            tracing::warn!("Failed to cleanup expired revocations: {}", e);
        }
        _ => {}
    }
}

/// Cleanup expired revocation entries (SQLite version).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn cleanup_expired_revocations(pool: &crate::db::DbPool) {
    match sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < datetime('now')")
        .execute(pool)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => {
            tracing::debug!(
                cleaned = result.rows_affected(),
                "Cleaned expired token revocations"
            );
        }
        Err(e) => {
            tracing::warn!("Failed to cleanup expired revocations: {}", e);
        }
        _ => {}
    }
}

/// Compute a short fingerprint of the token for the revocation key.
/// We don't store the full token in the database for security.
fn token_fingerprint(token: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Build a Set-Cookie header value for the JWT token.
/// Uses HttpOnly, Secure, SameSite=Strict for maximum security.
pub fn build_auth_cookie(token: &str, is_production: bool) -> String {
    let secure = if is_production { "; Secure" } else { "" };
    format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400{}",
        AUTH_COOKIE_NAME, token, secure
    )
}

/// Build a Set-Cookie header to clear the auth cookie (for logout).
pub fn build_logout_cookie() -> String {
    format!(
        "{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        AUTH_COOKIE_NAME
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_fingerprint_deterministic() {
        let fp1 = token_fingerprint("test-token");
        let fp2 = token_fingerprint("test-token");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_token_fingerprint_different_for_different_tokens() {
        let fp1 = token_fingerprint("token-a");
        let fp2 = token_fingerprint("token-b");
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn test_build_auth_cookie_production() {
        let cookie = build_auth_cookie("jwt-token-here", true);
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("appcontrol_token=jwt-token-here"));
    }

    #[test]
    fn test_build_auth_cookie_development() {
        let cookie = build_auth_cookie("jwt-token-here", false);
        assert!(cookie.contains("HttpOnly"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn test_build_logout_cookie() {
        let cookie = build_logout_cookie();
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("appcontrol_token="));
    }
}
