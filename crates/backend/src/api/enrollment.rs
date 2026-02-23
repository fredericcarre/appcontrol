//! Enrollment token management + agent/gateway certificate enrollment.
//!
//! Tokens are created by admins (CLI, API, or UI) and used by agents to
//! obtain their mTLS certificates without manual PKI work.
//!
//! Flow:
//! 1. Admin creates token (UI/CLI/API) → stored as SHA-256 hash
//! 2. Agent sends token + hostname to `/api/v1/enroll` (unauthenticated)
//! 3. Backend validates token, generates cert signed by org CA, returns cert+key+CA
//! 4. Agent writes certs to disk, connects with mTLS

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{validate_length, ApiError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Token management (authenticated — admin endpoints)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    /// Max number of uses (null = unlimited)
    pub max_uses: Option<i32>,
    /// Validity in hours (default 24)
    pub valid_hours: Option<i64>,
    /// Scope: "agent" or "gateway" (default "agent")
    pub scope: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub id: Uuid,
    pub token: String,
    pub name: String,
    pub max_uses: Option<i32>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scope: String,
}

pub async fn create_enrollment_token(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_length("name", &req.name, 1, 200)?;

    let scope = req.scope.as_deref().unwrap_or("agent");
    if scope != "agent" && scope != "gateway" {
        return Err(ApiError::Validation(
            "scope must be 'agent' or 'gateway'".to_string(),
        ));
    }

    let valid_hours = req.valid_hours.unwrap_or(24);
    if !(1..=8760).contains(&valid_hours) {
        return Err(ApiError::Validation(
            "valid_hours must be between 1 and 8760 (1 year)".to_string(),
        ));
    }

    // Generate token and compute hash (we store hash, return plaintext once)
    let token = appcontrol_common::generate_enrollment_token();
    let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));
    let token_prefix = &token[..std::cmp::min(token.len(), 18)];
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(valid_hours);

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_enrollment_token",
        "enrollment_token",
        Uuid::nil(),
        json!({ "name": &req.name, "scope": scope, "max_uses": req.max_uses, "valid_hours": valid_hours }),
    )
    .await
    .ok();

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO enrollment_tokens
           (organization_id, token_hash, token_prefix, name, max_uses, expires_at, scope, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id"#,
    )
    .bind(user.organization_id)
    .bind(&token_hash)
    .bind(token_prefix)
    .bind(&req.name)
    .bind(req.max_uses)
    .bind(expires_at)
    .bind(scope)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({
        "id": id,
        "token": token,
        "name": req.name,
        "max_uses": req.max_uses,
        "expires_at": expires_at,
        "scope": scope,
    })))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EnrollmentTokenRow {
    pub id: Uuid,
    pub token_prefix: String,
    pub name: String,
    pub max_uses: Option<i32>,
    pub current_uses: i32,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scope: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_enrollment_tokens(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let tokens = sqlx::query_as::<_, EnrollmentTokenRow>(
        r#"SELECT id, token_prefix, name, max_uses, current_uses, expires_at, scope, created_at, revoked_at
           FROM enrollment_tokens
           WHERE organization_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "tokens": tokens })))
}

pub async fn revoke_enrollment_token(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(token_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "revoke_enrollment_token",
        "enrollment_token",
        token_id,
        json!({}),
    )
    .await
    .ok();

    let result = sqlx::query(
        r#"UPDATE enrollment_tokens
           SET revoked_at = now(), revoked_by = $3
           WHERE id = $1 AND organization_id = $2 AND revoked_at IS NULL"#,
    )
    .bind(token_id)
    .bind(user.organization_id)
    .bind(user.user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(Json(json!({ "status": "revoked" })))
}

// ---------------------------------------------------------------------------
// PKI init — store organization CA
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InitPkiRequest {
    /// Organization name for the CA (e.g., "Acme Corp")
    pub org_name: String,
    /// CA validity in days (default 3650 = 10 years)
    pub validity_days: Option<u32>,
}

pub async fn init_pki(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<InitPkiRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_length("org_name", &req.org_name, 1, 200)?;

    // Check if CA already exists
    let existing: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT ca_cert_pem FROM organizations WHERE id = $1",
    )
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    if let Some((Some(ref _cert),)) = existing {
        return Err(ApiError::Conflict(
            "CA already initialized. Use force=true to regenerate (will invalidate all existing certs).".to_string(),
        ));
    }

    let validity_days = req.validity_days.unwrap_or(3650);
    let ca = appcontrol_common::generate_ca(&req.org_name, validity_days)
        .map_err(|e| ApiError::Internal(format!("CA generation failed: {}", e)))?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "init_pki",
        "organization",
        user.organization_id,
        json!({ "org_name": &req.org_name, "validity_days": validity_days }),
    )
    .await
    .ok();

    sqlx::query(
        "UPDATE organizations SET ca_cert_pem = $2, ca_key_pem = $3 WHERE id = $1",
    )
    .bind(user.organization_id)
    .bind(&ca.cert_pem)
    .bind(&ca.key_pem)
    .execute(&state.db)
    .await?;

    let fingerprint = appcontrol_common::fingerprint_pem(&ca.cert_pem).unwrap_or_default();

    Ok(Json(json!({
        "status": "initialized",
        "ca_fingerprint": fingerprint,
        "ca_cert_pem": ca.cert_pem,
        "validity_days": validity_days,
    })))
}

pub async fn get_ca_cert(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT ca_cert_pem FROM organizations WHERE id = $1",
    )
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((Some(cert_pem),)) => {
            let fingerprint = appcontrol_common::fingerprint_pem(&cert_pem).unwrap_or_default();
            Ok(Json(json!({
                "ca_cert_pem": cert_pem,
                "fingerprint": fingerprint,
            })))
        }
        _ => Err(ApiError::NotFound),
    }
}

// ---------------------------------------------------------------------------
// Agent/Gateway enrollment (UNAUTHENTICATED — token-based)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    /// The enrollment token (plaintext)
    pub token: String,
    /// Hostname of the agent/gateway being enrolled
    pub hostname: String,
    /// Additional DNS SANs for gateway certs (optional)
    #[serde(default)]
    pub san_dns: Vec<String>,
    /// Additional IP SANs for gateway certs (optional)
    #[serde(default)]
    pub san_ips: Vec<String>,
    /// Certificate validity in days (default 365)
    pub validity_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct EnrollResponse {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
    pub agent_id: Uuid,
    pub fingerprint: String,
    pub expires_in_days: u32,
}

pub async fn enroll(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<EnrollRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_length("hostname", &req.hostname, 1, 300)?;

    // Extract client IP from X-Forwarded-For or X-Real-IP headers
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    if !req.token.starts_with("ac_enroll_") {
        return Err(ApiError::Validation("Invalid token format".to_string()));
    }

    // Hash the token and look it up
    let token_hash = hex::encode(sha2::Sha256::digest(req.token.as_bytes()));

    let token_row = sqlx::query_as::<_, (Uuid, Uuid, String, Option<i32>, i32, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, organization_id, scope, max_uses, current_uses, expires_at
           FROM enrollment_tokens
           WHERE token_hash = $1
           AND revoked_at IS NULL"#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?;

    let (token_id, org_id, scope, max_uses, current_uses, expires_at) = match token_row {
        Some(row) => row,
        None => {
            log_enrollment_event(&state.db, Uuid::nil(), None, "invalid_token", &req.hostname, &client_ip).await;
            return Err(ApiError::Unauthorized);
        }
    };

    // Check expiry
    if chrono::Utc::now() > expires_at {
        log_enrollment_event(&state.db, org_id, Some(token_id), "token_expired", &req.hostname, &client_ip).await;
        return Err(ApiError::Validation("Token has expired".to_string()));
    }

    // Check usage limit
    if let Some(max) = max_uses {
        if current_uses >= max {
            log_enrollment_event(&state.db, org_id, Some(token_id), "token_exhausted", &req.hostname, &client_ip).await;
            return Err(ApiError::Validation("Token has reached max uses".to_string()));
        }
    }

    // Load organization CA
    let ca_row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ca_cert_pem, ca_key_pem FROM organizations WHERE id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await?;

    let (ca_cert_pem, ca_key_pem) = match ca_row {
        Some((Some(cert), Some(key))) => (cert, key),
        _ => {
            return Err(ApiError::Internal(
                "Organization CA not initialized. Run `appctl pki init` first.".to_string(),
            ));
        }
    };

    let validity_days = req.validity_days.unwrap_or(365);

    // Issue certificate based on scope
    let issued = match scope.as_str() {
        "gateway" => {
            appcontrol_common::issue_gateway_cert(
                &ca_cert_pem,
                &ca_key_pem,
                &req.hostname,
                &req.san_dns,
                &req.san_ips,
                validity_days,
            )
            .map_err(|e| ApiError::Internal(format!("Cert generation failed: {}", e)))?
        }
        _ => {
            appcontrol_common::issue_agent_cert(
                &ca_cert_pem,
                &ca_key_pem,
                &req.hostname,
                validity_days,
            )
            .map_err(|e| ApiError::Internal(format!("Cert generation failed: {}", e)))?
        }
    };

    let fingerprint = appcontrol_common::fingerprint_pem(&issued.cert_pem).unwrap_or_default();

    // Generate agent_id (deterministic from hostname, same as agent does locally)
    let agent_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, req.hostname.as_bytes());

    // Increment token usage
    sqlx::query("UPDATE enrollment_tokens SET current_uses = current_uses + 1 WHERE id = $1")
        .bind(token_id)
        .execute(&state.db)
        .await?;

    // Upsert agent record
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, hostname, is_active, certificate_fingerprint, certificate_cn, identity_verified)
           VALUES ($1, $2, $3, true, $4, $5, true)
           ON CONFLICT (id) DO UPDATE SET
               hostname = EXCLUDED.hostname,
               certificate_fingerprint = EXCLUDED.certificate_fingerprint,
               certificate_cn = EXCLUDED.certificate_cn,
               identity_verified = true,
               is_active = true"#,
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(&req.hostname)
    .bind(&fingerprint)
    .bind(&req.hostname)
    .execute(&state.db)
    .await?;

    // Log enrollment event (APPEND-ONLY)
    sqlx::query(
        r#"INSERT INTO enrollment_events
           (organization_id, token_id, event_type, hostname, ip_address, agent_id, cert_fingerprint, cert_cn)
           VALUES ($1, $2, 'success', $3, $4, $5, $6, $7)"#,
    )
    .bind(org_id)
    .bind(token_id)
    .bind(&req.hostname)
    .bind(client_ip.clone())
    .bind(agent_id)
    .bind(&fingerprint)
    .bind(&req.hostname)
    .execute(&state.db)
    .await
    .ok();

    // Log certificate event
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn, issued_at, expires_at)
           VALUES ($1, 'issued', $2, $3, now(), now() + $4 * interval '1 day')"#,
    )
    .bind(agent_id)
    .bind(&fingerprint)
    .bind(&req.hostname)
    .bind(validity_days as i32)
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(json!({
        "cert_pem": issued.cert_pem,
        "key_pem": issued.key_pem,
        "ca_pem": issued.ca_pem,
        "agent_id": agent_id,
        "fingerprint": fingerprint,
        "expires_in_days": validity_days,
    })))
}

/// Log a failed enrollment attempt (APPEND-ONLY).
async fn log_enrollment_event(
    db: &sqlx::PgPool,
    org_id: Uuid,
    token_id: Option<Uuid>,
    event_type: &str,
    hostname: &str,
    ip_address: &str,
) {
    sqlx::query(
        r#"INSERT INTO enrollment_events (organization_id, token_id, event_type, hostname, ip_address)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(org_id)
    .bind(token_id)
    .bind(event_type)
    .bind(hostname)
    .bind(ip_address)
    .execute(db)
    .await
    .ok();
}

// ---------------------------------------------------------------------------
// Enrollment events (audit trail)
// ---------------------------------------------------------------------------

pub async fn list_enrollment_events(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let events = sqlx::query_as::<_, (Uuid, Option<Uuid>, String, Option<String>, Option<String>, Option<Uuid>, Option<String>, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, token_id, event_type, hostname, ip_address, agent_id, cert_fingerprint, created_at
           FROM enrollment_events
           WHERE organization_id = $1
           ORDER BY created_at DESC
           LIMIT 100"#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    let events_json: Vec<Value> = events
        .into_iter()
        .map(|(id, token_id, event_type, hostname, ip_address, agent_id, cert_fp, created_at)| {
            json!({
                "id": id,
                "token_id": token_id,
                "event_type": event_type,
                "hostname": hostname,
                "ip_address": ip_address,
                "agent_id": agent_id,
                "cert_fingerprint": cert_fp,
                "created_at": created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "events": events_json })))
}
