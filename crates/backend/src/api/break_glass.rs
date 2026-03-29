use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbUuid;
use crate::error::{validate_length, ApiError};
use crate::middleware::audit::log_action;
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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BreakGlassSessionRow {
    pub id: DbUuid,
    pub account_id: DbUuid,
    pub organization_id: DbUuid,
    pub activated_by_ip: String,
    pub reason: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub actions_taken: i32,
}

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

    sqlx::query(
        "INSERT INTO break_glass_accounts (id, organization_id, username, password_hash) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(user.organization_id)
    .bind(&body.username)
    .bind(&password_hash)
    .execute(&state.db)
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

    let accounts = sqlx::query_as::<_, (DbUuid, String, bool, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, username, is_active, last_rotated_at FROM break_glass_accounts \
         WHERE organization_id = $1 ORDER BY username",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

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
    #[cfg(feature = "postgres")]
    let account = sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT id, organization_id FROM break_glass_accounts \
         WHERE username = $1 AND password_hash = $2 AND is_active = true",
    )
    .bind(&body.username)
    .bind(&password_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::Unauthorized)?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let account = sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT id, organization_id FROM break_glass_accounts \
         WHERE username = $1 AND password_hash = $2 AND is_active = 1",
    )
    .bind(&body.username)
    .bind(&password_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::Unauthorized)?;

    let (account_id, organization_id) = account;

    // Duration: 60 min default, 120 min max
    let duration_minutes = body.duration_minutes.unwrap_or(60).clamp(5, 120);

    // Create session (APPEND-ONLY)
    let session_id = Uuid::new_v4();
    let session = sqlx::query_as::<_, BreakGlassSessionRow>(&format!(
        "INSERT INTO break_glass_sessions (
                id, account_id, organization_id, activated_by_ip, reason, expires_at
            ) VALUES ($1, $2, $3, $4, $5, {} + make_interval(mins => $6))
            RETURNING id, account_id, organization_id, activated_by_ip, reason,
                      started_at, expires_at, ended_at, actions_taken",
        crate::db::sql::now()
    ))
    .bind(session_id)
    .bind(account_id)
    .bind(organization_id)
    .bind("0.0.0.0") // In production, extract from X-Forwarded-For
    .bind(&body.reason)
    .bind(duration_minutes)
    .fetch_one(&state.db)
    .await?;

    // Log the break-glass activation as a CRITICAL security event
    let _ = sqlx::query(
        &format!(
            "INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details, created_at)
             VALUES ($1, $2, 'break_glass_activated', 'organization', $3, $4, {})",
            crate::db::sql::now()
        ),
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(organization_id)
    .bind(json!({
        "session_id": session_id,
        "username": body.username,
        "reason": body.reason,
        "duration_minutes": duration_minutes,
        "break_glass": true,
    }))
    .execute(&state.db)
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

    let sessions = sqlx::query_as::<_, BreakGlassSessionRow>(
        "SELECT id, account_id, organization_id, activated_by_ip, reason, \
         started_at, expires_at, ended_at, actions_taken \
         FROM break_glass_sessions WHERE organization_id = $1 \
         ORDER BY started_at DESC LIMIT 50",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

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

    let result = sqlx::query(&format!(
        "UPDATE break_glass_sessions SET ended_at = {} \
         WHERE id = $1 AND organization_id = $2 AND ended_at IS NULL",
        crate::db::sql::now()
    ))
    .bind(session_id)
    .bind(user.organization_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
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
