use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{validate_length, ApiError};
use crate::middleware::audit::log_action;
use crate::repository::misc_queries;
use crate::AppState;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateBreakGlassAccountRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ActivateBreakGlassRequest {
    pub username: String,
    pub password: String,
    pub reason: String,
    /// Session duration in minutes (default: 60, max: 120)
    pub duration_minutes: Option<i32>,
}

// Re-export for backward compatibility
pub use misc_queries::BreakGlassSessionRow;

/// Simple password hash using base64 of XOR folding.
/// In production, replace with argon2 or bcrypt.
fn hash_password(password: &str) -> String {
    use base64::Engine;
    let bytes = password.as_bytes();
    let mut hash = [0u8; 32];
    for (i, &b) in bytes.iter().enumerate() {
        hash[i % 32] ^= b;
        hash[(i + 7) % 32] = hash[(i + 7) % 32].wrapping_add(b);
        hash[(i + 13) % 32] = hash[(i + 13) % 32].wrapping_mul(b | 1);
    }
    base64::engine::general_purpose::STANDARD.encode(hash)
}

// ============================================================================
// Admin: Manage break-glass accounts
// ============================================================================

/// Create a break-glass account (org admin only).
pub async fn create_break_glass_account(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateBreakGlassAccountRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_length("username", &body.username, 1, 200)?;

    let id = Uuid::new_v4();
    let password_hash = hash_password(&body.password);

    log_action(
        &state.db,
        user.user_id,
        "create_break_glass_account",
        "organization",
        user.organization_id,
        json!({ "username": body.username }),
    )
    .await?;

    misc_queries::create_break_glass_account(
        &state.db,
        id,
        user.organization_id,
        &body.username,
        &password_hash,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "username": body.username,
            "message": "Break-glass account created. Store the password in a secure vault."
        })),
    ))
}

/// List break-glass accounts (org admin only).
pub async fn list_break_glass_accounts(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let accounts = misc_queries::list_break_glass_accounts(&state.db, user.organization_id).await?;

    let result: Vec<Value> = accounts
        .into_iter()
        .map(|(id, username, is_active, last_rotated)| {
            json!({
                "id": id,
                "username": username,
                "is_active": is_active,
                "last_rotated_at": last_rotated,
            })
        })
        .collect();

    Ok(Json(json!({ "accounts": result })))
}

// ============================================================================
// Break-glass activation (no auth required — this IS the emergency access)
// ============================================================================

/// Activate a break-glass session. Returns a temporary JWT token.
/// This endpoint does NOT require standard authentication.
pub async fn activate_break_glass(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActivateBreakGlassRequest>,
) -> Result<Json<Value>, ApiError> {
    let password_hash = hash_password(&body.password);

    // Validate credentials
    let account = misc_queries::find_break_glass_account(&state.db, &body.username, &password_hash)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    let (account_id, organization_id) = account;

    // Duration: 60 min default, 120 min max
    let duration_minutes = body.duration_minutes.unwrap_or(60).clamp(5, 120);

    // Create session (APPEND-ONLY)
    let session_id = Uuid::new_v4();
    let session = misc_queries::create_break_glass_session(
        &state.db,
        session_id,
        account_id,
        organization_id,
        "0.0.0.0", // In production, extract from X-Forwarded-For
        &body.reason,
        duration_minutes,
    )
    .await?;

    // Log the break-glass activation as a CRITICAL security event
    let _ = misc_queries::log_break_glass_activation(
        &state.db,
        account_id,
        organization_id,
        json!({
            "session_id": session_id,
            "username": body.username,
            "reason": body.reason,
            "duration_minutes": duration_minutes,
            "break_glass": true,
        }),
    )
    .await;

    // Broadcast security alert
    tracing::warn!(
        session_id = %session_id,
        username = %body.username,
        reason = %body.reason,
        "BREAK-GLASS SESSION ACTIVATED"
    );

    Ok(Json(json!({
        "session": session,
        "message": "Break-glass session activated. All actions will be audited.",
        "warning": "Emergency use only. Post-incident review is mandatory.",
    })))
}

/// List break-glass sessions (org admin only, for audit).
pub async fn list_break_glass_sessions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let sessions =
        misc_queries::list_break_glass_sessions(&state.db, user.organization_id).await?;

    Ok(Json(json!({ "sessions": sessions })))
}

/// End a break-glass session early (org admin only).
pub async fn end_break_glass_session(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows =
        misc_queries::end_break_glass_session(&state.db, session_id, user.organization_id).await?;

    if rows == 0 {
        return Err(ApiError::NotFound);
    }

    log_action(
        &state.db,
        user.user_id,
        "end_break_glass_session",
        "break_glass_session",
        session_id,
        json!({ "ended_by": user.user_id }),
    )
    .await?;

    Ok(Json(json!({
        "session_id": session_id,
        "status": "ended",
    })))
}
