//! Gateway management API.
//!
//! Gateways bridge agent WebSocket connections to the backend.
//! Each gateway belongs to a site and authenticates via mTLS.

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

/// Response struct for gateway list with additional computed fields
#[derive(Debug, Serialize)]
pub struct GatewayListItem {
    pub id: Uuid,
    pub name: String,
    pub zone: String,
    pub status: String,
    pub agent_count: i64,
    pub connected: bool,
}

pub async fn list_gateways(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    // Allow all authenticated users to view gateways (read-only)
    let gateways = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            bool,
            Option<chrono::DateTime<chrono::Utc>>,
            i64,
        ),
    >(
        r#"SELECT
               g.id,
               g.name,
               g.zone,
               g.is_active,
               g.last_heartbeat_at,
               COALESCE((SELECT COUNT(*) FROM agents a WHERE a.gateway_id = g.id), 0) as agent_count
           FROM gateways g
           WHERE g.organization_id = $1
           ORDER BY g.name"#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    // Determine connected status: active + heartbeat within last 60 seconds
    let now = chrono::Utc::now();
    let gateway_list: Vec<GatewayListItem> = gateways
        .into_iter()
        .map(|(id, name, zone, is_active, last_heartbeat, agent_count)| {
            let status = if !is_active {
                "suspended".to_string()
            } else {
                "active".to_string()
            };
            let connected = is_active
                && last_heartbeat
                    .map(|hb| (now - hb).num_seconds() < 60)
                    .unwrap_or(false);
            GatewayListItem {
                id,
                name,
                zone,
                status,
                agent_count,
                connected,
            }
        })
        .collect();

    Ok(Json(json!({ "gateways": gateway_list })))
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

/// GET /api/v1/gateways/:id/agents — List agents connected to a gateway
pub async fn list_gateway_agents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Verify gateway belongs to user's org
    let gateway_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM gateways WHERE id = $1 AND organization_id = $2)",
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_one(&state.db)
    .await?;

    if !gateway_exists {
        return Err(ApiError::NotFound);
    }

    let agents = sqlx::query_as::<_, (Uuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT id, hostname, is_active, last_heartbeat_at
           FROM agents
           WHERE gateway_id = $1 AND organization_id = $2
           ORDER BY hostname"#,
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    let agents_json: Vec<Value> = agents
        .into_iter()
        .map(|(id, hostname, is_active, last_heartbeat_at)| {
            json!({
                "id": id,
                "hostname": hostname,
                "is_active": is_active,
                "last_heartbeat_at": last_heartbeat_at,
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agents_json })))
}

/// POST /api/v1/gateways/:id/suspend — Suspend a gateway
pub async fn suspend_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "suspend_gateway",
        "gateway",
        gateway_id,
        json!({}),
    )
    .await
    .ok();

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"UPDATE gateways SET is_active = false
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, zone, hostname, port, site_id,
                     certificate_fingerprint, is_active, last_heartbeat_at, created_at"#,
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(gw)))
}

/// POST /api/v1/gateways/:id/activate — Activate a suspended gateway
pub async fn activate_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "activate_gateway",
        "gateway",
        gateway_id,
        json!({}),
    )
    .await
    .ok();

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"UPDATE gateways SET is_active = true
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, zone, hostname, port, site_id,
                     certificate_fingerprint, is_active, last_heartbeat_at, created_at"#,
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(gw)))
}

/// DELETE /api/v1/gateways/:id — Delete a gateway
pub async fn delete_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "delete_gateway",
        "gateway",
        gateway_id,
        json!({}),
    )
    .await
    .ok();

    // First disconnect all agents from this gateway
    sqlx::query("UPDATE agents SET gateway_id = NULL WHERE gateway_id = $1")
        .bind(gateway_id)
        .execute(&state.db)
        .await?;

    let result = sqlx::query("DELETE FROM gateways WHERE id = $1 AND organization_id = $2")
        .bind(gateway_id)
        .bind(user.organization_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
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

    let certs = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<Uuid>,
            Option<Uuid>,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
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
