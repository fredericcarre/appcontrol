use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::auth::{jwt, AuthUser};
use crate::AppState;

/// Middleware that extracts the authenticated user from JWT or API key.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let auth_user = match auth_header {
        Some(ref header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            let claims = jwt::validate_token(token, &state.config.jwt_secret, &state.config.jwt_issuer)
                .map_err(|_| StatusCode::UNAUTHORIZED)?;

            let user_id: uuid::Uuid = claims.sub.parse().map_err(|_| StatusCode::UNAUTHORIZED)?;
            let org_id: uuid::Uuid = claims.org.parse().map_err(|_| StatusCode::UNAUTHORIZED)?;

            AuthUser {
                user_id,
                organization_id: org_id,
                email: claims.email,
                role: claims.role,
            }
        }
        Some(ref header) if header.starts_with("ApiKey ") => {
            let key = &header[7..];
            crate::auth::api_key::validate_api_key(&state.db, key)
                .await
                .map_err(|_| StatusCode::UNAUTHORIZED)?
        }
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    request.extensions_mut().insert(auth_user);
    Ok(next.run(request).await)
}
