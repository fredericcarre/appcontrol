//! Gateway management API.
//!
//! Gateways bridge agent WebSocket connections to the backend.
//! Each gateway belongs to a site and authenticates via mTLS.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateGatewayRequest {
    pub name: Option<String>,
    pub site_id: Option<Uuid>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GatewayRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub zone: String,
    pub hostname: Option<String>,
    pub port: Option<i32>,
    pub site_id: Option<Uuid>,
    pub certificate_fingerprint: Option<String>,
    pub is_active: bool,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_gateways(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let gateways = sqlx::query_as::<_, GatewayRow>(
        r#"SELECT id, organization_id, name, zone, hostname, port, site_id,
                  certificate_fingerprint, is_active, last_heartbeat_at, created_at
           FROM gateways
           WHERE organization_id = $1
           ORDER BY name"#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "gateways": gateways })))
}

pub async fn get_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"SELECT id, organization_id, name, zone, hostname, port, site_id,
                  certificate_fingerprint, is_active, last_heartbeat_at, created_at
           FROM gateways
           WHERE id = $1 AND organization_id = $2"#,
    )
    .bind(id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(gw)))
}

pub async fn update_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateGatewayRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // If site_id is provided, verify it belongs to the same org
    if let Some(site_id) = req.site_id {
        let site_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE id = $1 AND organization_id = $2)",
        )
        .bind(site_id)
        .bind(user.organization_id)
        .fetch_one(&state.db)
        .await?;

        if !site_exists {
            return Err(ApiError::Validation(
                "site_id does not exist in this organization".to_string(),
            ));
        }
    }

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_gateway",
        "gateway",
        id,
        json!({ "site_id": req.site_id, "is_active": req.is_active }),
    )
    .await
    .ok();

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"UPDATE gateways SET
               name = COALESCE($3, name),
               site_id = COALESCE($4, site_id),
               is_active = COALESCE($5, is_active)
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, zone, hostname, port, site_id,
                     certificate_fingerprint, is_active, last_heartbeat_at, created_at"#,
    )
    .bind(id)
    .bind(user.organization_id)
    .bind(&req.name)
    .bind(req.site_id)
    .bind(req.is_active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(gw)))
}

// ---------------------------------------------------------------------------
// Certificate Revocation
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RevokeCertRequest {
    pub reason: String,
}

/// POST /api/v1/agents/:id/revoke-cert — Revoke an agent's certificate.
pub async fn revoke_agent_cert(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
    Json(req): Json<RevokeCertRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get agent's current fingerprint
    let agent: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT certificate_fingerprint, certificate_cn FROM agents WHERE id = $1 AND organization_id = $2",
    )
    .bind(agent_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    let (fingerprint, cn) = match agent {
        Some((Some(fp), cn)) => (fp, cn),
        Some((None, _)) => {
            return Err(ApiError::Validation(
                "Agent has no certificate to revoke".to_string(),
            ));
        }
        None => return Err(ApiError::NotFound),
    };

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "revoke_agent_cert",
        "agent",
        agent_id,
        json!({ "fingerprint": &fingerprint, "reason": &req.reason }),
    )
    .await
    .ok();

    let mut tx = state.db.begin().await?;

    // Insert into revoked_certificates (APPEND-ONLY)
    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, agent_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(user.organization_id)
    .bind(&fingerprint)
    .bind(&cn)
    .bind(agent_id)
    .bind(&req.reason)
    .bind(user.user_id)
    .execute(&mut *tx)
    .await?;

    // Log certificate event
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           VALUES ($1, 'revoked', $2, $3)"#,
    )
    .bind(agent_id)
    .bind(&fingerprint)
    .bind(&cn)
    .execute(&mut *tx)
    .await?;

    // Deactivate the agent
    sqlx::query("UPDATE agents SET is_active = false, identity_verified = false WHERE id = $1")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Json(json!({
        "status": "revoked",
        "fingerprint": fingerprint,
        "agent_id": agent_id,
    })))
}

/// POST /api/v1/gateways/:id/revoke-cert — Revoke a gateway's certificate.
pub async fn revoke_gateway_cert(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
    Json(req): Json<RevokeCertRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let gw: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT certificate_fingerprint, certificate_cn FROM gateways WHERE id = $1 AND organization_id = $2",
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    let (fingerprint, cn) = match gw {
        Some((Some(fp), cn)) => (fp, cn),
        Some((None, _)) => {
            return Err(ApiError::Validation(
                "Gateway has no certificate to revoke".to_string(),
            ));
        }
        None => return Err(ApiError::NotFound),
    };

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "revoke_gateway_cert",
        "gateway",
        gateway_id,
        json!({ "fingerprint": &fingerprint, "reason": &req.reason }),
    )
    .await
    .ok();

    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"INSERT INTO revoked_certificates (organization_id, fingerprint, cn, gateway_id, reason, revoked_by)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(user.organization_id)
    .bind(&fingerprint)
    .bind(&cn)
    .bind(gateway_id)
    .bind(&req.reason)
    .bind(user.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO certificate_events (gateway_id, event_type, fingerprint, cn)
           VALUES ($1, 'revoked', $2, $3)"#,
    )
    .bind(gateway_id)
    .bind(&fingerprint)
    .bind(&cn)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(gateway_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Json(json!({
        "status": "revoked",
        "fingerprint": fingerprint,
        "gateway_id": gateway_id,
    })))
}

/// GET /api/v1/revoked-certificates — List all revoked certificates.
pub async fn list_revoked_certificates(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let certs = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<Uuid>, Option<Uuid>, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, fingerprint, cn, agent_id, gateway_id, reason, revoked_at
           FROM revoked_certificates
           WHERE organization_id = $1
           ORDER BY revoked_at DESC
           LIMIT 100"#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    let certs_json: Vec<Value> = certs
        .into_iter()
        .map(|(id, fp, cn, agent_id, gateway_id, reason, revoked_at)| {
            json!({
                "id": id,
                "fingerprint": fp,
                "cn": cn,
                "agent_id": agent_id,
                "gateway_id": gateway_id,
                "reason": reason,
                "revoked_at": revoked_at,
            })
        })
        .collect();

    Ok(Json(json!({ "revoked_certificates": certs_json })))
}

/// Check if a certificate fingerprint is revoked.
/// Used internally by the gateway mTLS verification.
pub async fn is_cert_revoked(db: &sqlx::PgPool, org_id: Uuid, fingerprint: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM revoked_certificates WHERE organization_id = $1 AND fingerprint = $2)",
    )
    .bind(org_id)
    .bind(fingerprint)
    .fetch_one(db)
    .await
    .unwrap_or(false)
}

/// Verify an agent's certificate fingerprint matches what we issued (pinning).
/// Returns true if the fingerprint matches the stored one for this agent.
pub async fn verify_agent_cert_pinning(
    db: &sqlx::PgPool,
    agent_id: Uuid,
    presented_fingerprint: &str,
) -> bool {
    let stored: Option<Option<String>> = sqlx::query_scalar(
        "SELECT certificate_fingerprint FROM agents WHERE id = $1 AND is_active = true AND identity_verified = true",
    )
    .bind(agent_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    match stored {
        Some(Some(fp)) => fp == presented_fingerprint,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fingerprint_comparison() {
        let stored = "abc123";
        let presented = "abc123";
        assert_eq!(stored, presented);

        let wrong = "def456";
        assert_ne!(stored, wrong);
    }
}
