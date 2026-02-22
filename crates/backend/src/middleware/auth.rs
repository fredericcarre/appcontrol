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
/// Also checks token revocation against Redis if available.
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

            // Check token revocation via Redis (if available)
            if is_token_revoked(&state, &token).await {
                tracing::warn!(email = %claims.email, "Revoked token used");
                return Err(StatusCode::UNAUTHORIZED);
            }

            let user_id: uuid::Uuid = claims.sub.parse().map_err(|_| StatusCode::UNAUTHORIZED)?;
            let org_id: uuid::Uuid = claims.org.parse().map_err(|_| StatusCode::UNAUTHORIZED)?;

            AuthUser {
                user_id,
                organization_id: org_id,
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

/// Check if a token has been revoked (stored in Redis blacklist).
async fn is_token_revoked(state: &AppState, token: &str) -> bool {
    let mut conn = match &state.redis {
        Some(r) => r.clone(),
        None => return false, // No Redis = no revocation (acceptable in dev)
    };

    let key = format!("revoked:{}", token_fingerprint(token));
    match redis::cmd("EXISTS")
        .arg(&key)
        .query_async::<_, i32>(&mut conn)
        .await
    {
        Ok(exists) => exists > 0,
        Err(e) => {
            tracing::warn!("Redis revocation check failed: {} — allowing token", e);
            false // Fail open: if Redis is down, allow the token
        }
    }
}

/// Revoke a token by adding it to the Redis blacklist.
/// The entry auto-expires when the token would have expired (max 24h).
pub async fn revoke_token(state: &AppState, token: &str) -> Result<(), String> {
    let mut conn = state
        .redis
        .clone()
        .ok_or_else(|| "Redis not configured — token revocation unavailable".to_string())?;

    let key = format!("revoked:{}", token_fingerprint(token));
    // Token expires in 24h max, so set TTL to 24h + buffer
    let ttl_secs = 86400 + 3600; // 25 hours

    redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("EX")
        .arg(ttl_secs)
        .query_async::<_, String>(&mut conn)
        .await
        .map_err(|e| format!("Redis error: {}", e))?;

    tracing::info!("Token revoked (fingerprint={})", &key);
    Ok(())
}

/// Compute a short fingerprint of the token for Redis key.
/// We don't store the full token in Redis for security.
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
