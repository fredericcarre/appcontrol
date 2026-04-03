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
#[allow(unused_imports)]
use crate::db::DbUuid;
use crate::error::{ApiError, OptionExt};
use crate::repository::gateway_queries as gw_repo;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateGatewayRequest {
    pub name: Option<String>,
    pub site_id: Option<Uuid>,
    pub is_active: Option<bool>,
    pub is_primary: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GatewayRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    /// DEPRECATED: Legacy zone field, now nullable. Use site_id instead.
    pub zone: Option<String>,
    pub hostname: Option<String>,
    pub port: Option<i32>,
    pub site_id: Option<DbUuid>,
    pub certificate_fingerprint: Option<String>,
    pub is_active: bool,
    pub is_primary: bool,
    pub priority: i32,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Response struct for gateway list with additional computed fields
#[derive(Debug, Serialize)]
pub struct GatewayListItem {
    pub id: Uuid,
    pub name: String,
    pub zone: String,
    pub status: String, // "active", "suspended"
    pub role: String,   // "primary", "standby", "failover_active"
    pub is_primary: bool,
    pub priority: i32,
    pub agent_count: i64,
    pub connected: bool,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    // Site information
    pub site_id: Option<Uuid>,
    pub site_name: Option<String>,
    pub site_code: Option<String>,
}

/// Site summary for grouping gateways
#[derive(Debug, Serialize)]
pub struct SiteSummary {
    pub site_id: Option<Uuid>,
    pub site_name: String,
    pub site_code: String,
    /// DEPRECATED: Legacy zone field for backward compatibility. Same as site_code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    pub gateway_count: i64,
    pub active_gateway_id: Option<Uuid>,
    pub failover_active: bool,
    pub gateways: Vec<GatewayListItem>,
}

pub async fn list_gateways(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    // Allow all authenticated users to view gateways (read-only)
    let gateways = state
        .gateway_repo
        .list_gateways(*user.organization_id)
        .await?;

    // Get the set of gateways actually connected via WebSocket
    let connected_ids: std::collections::HashSet<Uuid> =
        state.ws_hub.connected_gateway_ids().into_iter().collect();

    // Group by site_id and compute failover status
    // Key is (site_id, site_name, site_code)
    let mut sites_map: std::collections::HashMap<
        (Option<Uuid>, String, String),
        Vec<GatewayListItem>,
    > = std::collections::HashMap::new();

    for gw in gateways {
        let id = gw.id;
        let name = gw.name;
        let zone = gw.zone;
        let is_active = gw.is_active;
        let is_primary = gw.is_primary;
        let priority = gw.priority;
        let version = gw.version;
        let last_heartbeat = gw.last_heartbeat_at;
        let agent_count = gw.agent_count;
        let site_id = gw.site_id;
        let site_name = gw.site_name;
        let site_code = gw.site_code;

        // Connection status is determined by actual WebSocket connection in the hub
        let connected = is_active && connected_ids.contains(&id);

        let status = if !is_active {
            "suspended".to_string()
        } else {
            "active".to_string()
        };

        // Role is computed based on primary flag and connection status
        let role = if is_primary && connected {
            "primary".to_string()
        } else if is_primary && !connected {
            "primary_offline".to_string()
        } else if connected {
            "standby".to_string()
        } else {
            "standby_offline".to_string()
        };

        let item = GatewayListItem {
            id,
            name,
            zone: zone.clone().unwrap_or_default(),
            status,
            role,
            is_primary,
            priority,
            agent_count,
            connected,
            version,
            last_heartbeat_at: last_heartbeat,
            site_id,
            site_name: site_name.clone(),
            site_code: site_code.clone(),
        };

        // Group by site - use "Unassigned" for gateways without a site
        let key = (
            site_id,
            site_name.unwrap_or_else(|| "Unassigned".to_string()),
            site_code.unwrap_or_else(|| "N/A".to_string()),
        );
        sites_map.entry(key).or_default().push(item);
    }

    // Build site summaries
    let mut sites: Vec<SiteSummary> = sites_map
        .into_iter()
        .map(|((site_id, site_name, site_code), mut gateways)| {
            // Sort by priority within site
            gateways.sort_by_key(|g| g.priority);

            // Check if primary is offline → failover active
            let primary = gateways.iter().find(|g| g.is_primary);
            let primary_offline = primary.map(|p| !p.connected).unwrap_or(true);

            // Find active gateway (first connected in priority order)
            let active_gateway = gateways.iter().find(|g| g.connected);
            let failover_active = primary_offline && active_gateway.is_some();

            // Update role for the gateway handling traffic
            let active_id = active_gateway.map(|g| g.id);
            for gw in &mut gateways {
                if failover_active && Some(gw.id) == active_id && !gw.is_primary {
                    gw.role = "failover_active".to_string();
                }
            }

            SiteSummary {
                site_id,
                site_name: site_name.clone(),
                site_code: site_code.clone(),
                // Include zone for backward compat with old frontend
                zone: Some(site_code),
                gateway_count: gateways.len() as i64,
                active_gateway_id: active_id,
                failover_active,
                gateways,
            }
        })
        .collect();

    // Sort: assigned sites by name first, then unassigned
    sites.sort_by(|a, b| match (a.site_id.is_some(), b.site_id.is_some()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.site_name.cmp(&b.site_name),
    });

    // Return as "sites" - frontend should use this instead of legacy "zones"
    Ok(Json(json!({ "sites": sites })))
}

pub async fn get_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let gw = state
        .gateway_repo
        .get_gateway(id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!({
        "id": gw.id,
        "organization_id": gw.organization_id,
        "name": gw.name,
        "zone": gw.zone,
        "hostname": gw.hostname,
        "port": gw.port,
        "site_id": gw.site_id,
        "certificate_fingerprint": gw.certificate_fingerprint,
        "is_active": gw.is_active,
        "is_primary": gw.is_primary,
        "priority": gw.priority,
        "version": gw.version,
        "last_heartbeat_at": gw.last_heartbeat_at,
        "created_at": gw.created_at,
    })))
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
        if !gw_repo::site_exists_in_org(&state.db, site_id, *user.organization_id).await? {
            return Err(ApiError::Validation(
                "site_id does not exist in this organization".to_string(),
            ));
        }
    }

    // If setting as primary, first unset any existing primary in the same site
    if req.is_primary == Some(true) {
        let gw_info = gw_repo::get_gateway_site_and_zone(&state.db, id, *user.organization_id).await?;

        if let Some((site_id, zone)) = gw_info {
            if let Some(sid) = site_id {
                gw_repo::unset_primary_in_site(&state.db, *user.organization_id, sid, id).await?;
            } else {
                gw_repo::unset_primary_in_zone(&state.db, *user.organization_id, &zone, id).await?;
            }
        }
    }

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_gateway",
        "gateway",
        id,
        json!({
            "site_id": req.site_id,
            "is_active": req.is_active,
            "is_primary": req.is_primary,
            "priority": req.priority
        }),
    )
    .await
    .ok();

    let gw = gw_repo::update_gateway_returning(
        &state.db, id, *user.organization_id, &req.name, req.site_id, req.is_active, req.is_primary, req.priority,
    )
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(gw)))
}

/// POST /api/v1/gateways/:id/set-primary — Set this gateway as primary for its site
pub async fn set_gateway_primary(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get the gateway's site_id
    let (site_id, zone) = gw_repo::get_gateway_site_and_zone(&state.db, gateway_id, *user.organization_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "set_gateway_primary",
        "gateway",
        gateway_id,
        json!({ "site_id": site_id, "zone": &zone }),
    )
    .await
    .ok();

    let mut tx = state.db.begin().await?;

    // Unset existing primary in the same site (or same zone if no site assigned)
    if let Some(sid) = site_id {
        sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND site_id = $2 AND id != $3")
            .bind(crate::db::bind_id(user.organization_id))
            .bind(sid)
            .bind(gateway_id)
            .execute(&mut *tx)
            .await?;
    } else {
        // Fallback: use zone for gateways without site assignment
        sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND zone = $2 AND site_id IS NULL AND id != $3")
            .bind(crate::db::bind_id(user.organization_id))
            .bind(&zone)
            .bind(gateway_id)
            .execute(&mut *tx)
            .await?;
    }

    // Set this gateway as primary
    sqlx::query("UPDATE gateways SET is_primary = true WHERE id = $1")
        .bind(gateway_id)
        .execute(&mut *tx)
        .await?;

    // Log status event
    sqlx::query(
        r#"INSERT INTO gateway_status_events (organization_id, gateway_id, event_type, triggered_by)
           VALUES ($1, $2, 'promoted_to_primary', 'manual')"#,
    )
    .bind(crate::db::bind_id(user.organization_id))
    .bind(gateway_id)
    .execute(&mut *tx)
    .await
    .ok(); // Don't fail if table doesn't exist yet

    tx.commit().await?;

    Ok(Json(json!({
        "status": "ok",
        "gateway_id": gateway_id,
        "site_id": site_id,
    })))
}

/// GET /api/v1/gateways/:id/agents — List agents connected to a gateway
pub async fn list_gateway_agents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Verify gateway belongs to user's org
    if !gw_repo::gateway_exists_in_org(&state.db, gateway_id, *user.organization_id).await? {
        return Err(ApiError::NotFound);
    }

    let agents = gw_repo::list_gateway_agents(&state.db, gateway_id, *user.organization_id).await?;

    // Get live connection status from the WebSocket hub
    let connected_agents = state.ws_hub.connected_agent_ids();
    let connected_set: std::collections::HashSet<Uuid> = connected_agents.into_iter().collect();

    let agents_json: Vec<Value> = agents
        .into_iter()
        .map(|a| {
            let connected = connected_set.contains(&a.id);
            json!({
                "id": a.id,
                "hostname": a.hostname,
                "is_active": a.is_active,
                "last_heartbeat_at": a.last_heartbeat_at,
                "connected": connected,
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

    let gw = gw_repo::suspend_gateway(&state.db, gateway_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!({
        "id": gw.id, "organization_id": gw.organization_id, "name": gw.name,
        "zone": gw.zone, "hostname": gw.hostname, "port": gw.port,
        "site_id": gw.site_id, "certificate_fingerprint": gw.certificate_fingerprint,
        "is_active": gw.is_active, "is_primary": gw.is_primary, "priority": gw.priority,
        "version": gw.version, "last_heartbeat_at": gw.last_heartbeat_at,
        "created_at": gw.created_at,
    })))
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

    let gw = gw_repo::activate_gateway(&state.db, gateway_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    // Send ClearBlocklist to the gateway so all agents can reconnect
    // This fixes the bug where agents remain blocked after gateway activation
    let clear_msg = appcontrol_common::GatewayEnvelope::ClearBlocklist;
    if let Ok(json_str) = serde_json::to_string(&clear_msg) {
        state.ws_hub.send_to_gateway(gateway_id, &json_str);
        tracing::info!(
            gateway_id = %gateway_id,
            "Sent ClearBlocklist to gateway after activation"
        );
    }

    Ok(Json(json!({
        "id": gw.id, "organization_id": gw.organization_id, "name": gw.name,
        "zone": gw.zone, "hostname": gw.hostname, "port": gw.port,
        "site_id": gw.site_id, "certificate_fingerprint": gw.certificate_fingerprint,
        "is_active": gw.is_active, "is_primary": gw.is_primary, "priority": gw.priority,
        "version": gw.version, "last_heartbeat_at": gw.last_heartbeat_at,
        "created_at": gw.created_at,
    })))
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
    gw_repo::disconnect_agents_from_gateway(&state.db, gateway_id).await?;

    let rows = gw_repo::delete_gateway(&state.db, gateway_id, *user.organization_id).await?;

    if rows == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/gateways/:id/block — Block a gateway (security action)
///
/// This suspends the gateway, disconnects all agents, and prevents reconnection.
/// Use for compromised gateways that need to be isolated immediately.
pub async fn block_gateway(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(gateway_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get the gateway to verify it exists and get agent count
    let (name, zone) = gw_repo::get_gateway_name_and_zone(&state.db, gateway_id, *user.organization_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "block_gateway",
        "gateway",
        gateway_id,
        json!({ "name": &name, "zone": &zone }),
    )
    .await
    .ok();

    let mut tx = state.db.begin().await?;

    // 1. Suspend the gateway
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(gateway_id)
        .execute(&mut *tx)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE gateways SET is_active = 0 WHERE id = $1")
        .bind(DbUuid::from(gateway_id))
        .execute(&mut *tx)
        .await?;

    // 2. Get all agents connected to this gateway
    let agent_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM agents WHERE gateway_id = $1 AND organization_id = $2")
            .bind(gateway_id)
            .bind(crate::db::bind_id(user.organization_id))
            .fetch_all(&mut *tx)
            .await?;

    let agent_count = agent_ids.len();

    // 3. Disconnect all agents (set gateway_id = NULL)
    sqlx::query("UPDATE agents SET gateway_id = NULL WHERE gateway_id = $1")
        .bind(gateway_id)
        .execute(&mut *tx)
        .await?;

    // 4. Log gateway status event
    sqlx::query(
        r#"INSERT INTO gateway_status_events (organization_id, gateway_id, event_type, triggered_by)
           VALUES ($1, $2, 'blocked', 'manual')"#,
    )
    .bind(crate::db::bind_id(user.organization_id))
    .bind(gateway_id)
    .execute(&mut *tx)
    .await
    .ok(); // Don't fail if table doesn't exist yet

    // 5. Transition all components of affected agents to UNREACHABLE
    let mut components_affected = 0;
    for agent_id in &agent_ids {
        components_affected += transition_gateway_agent_components_to_unreachable(
            &state,
            DbUuid::from(*agent_id),
            DbUuid::from(gateway_id),
        )
        .await;
    }

    tx.commit().await?;

    // 6. Send block command to all agents via WebSocket hub
    for agent_id in &agent_ids {
        state
            .ws_hub
            .block_agent(*agent_id, "Gateway blocked by administrator");
    }

    // 7. Disconnect the gateway itself
    state.ws_hub.disconnect_gateway(gateway_id);

    Ok(Json(json!({
        "status": "blocked",
        "gateway_id": gateway_id,
        "gateway_name": name,
        "zone": zone,
        "agents_disconnected": agent_count,
        "components_affected": components_affected,
    })))
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
    let agent = gw_repo::get_agent_cert_info(&state.db, agent_id, *user.organization_id).await?;

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
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&fingerprint)
    .bind(&cn)
    .bind(crate::db::bind_id(agent_id))
    .bind(&req.reason)
    .bind(crate::db::bind_id(user.user_id))
    .execute(&mut *tx)
    .await?;

    // Log certificate event
    sqlx::query(
        r#"INSERT INTO certificate_events (agent_id, event_type, fingerprint, cn)
           VALUES ($1, 'revoked', $2, $3)"#,
    )
    .bind(crate::db::bind_id(agent_id))
    .bind(&fingerprint)
    .bind(&cn)
    .execute(&mut *tx)
    .await?;

    // Deactivate the agent (inside transaction — use raw SQL for tx support)
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE agents SET is_active = false, identity_verified = false WHERE id = $1")
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE agents SET is_active = 0, identity_verified = 0 WHERE id = $1")
        .bind(DbUuid::from(agent_id))
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

    let gw = gw_repo::get_gateway_cert_info(&state.db, gateway_id, *user.organization_id).await?;

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
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&fingerprint)
    .bind(&cn)
    .bind(gateway_id)
    .bind(&req.reason)
    .bind(crate::db::bind_id(user.user_id))
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

    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(gateway_id)
        .execute(&mut *tx)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE gateways SET is_active = 0 WHERE id = $1")
        .bind(DbUuid::from(gateway_id))
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

    let certs = gw_repo::list_revoked_certificates(&state.db, *user.organization_id).await?;

    let certs_json: Vec<Value> = certs
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id,
                "fingerprint": c.fingerprint,
                "cn": c.cn,
                "agent_id": c.agent_id,
                "gateway_id": c.gateway_id,
                "reason": c.reason,
                "revoked_at": c.revoked_at,
            })
        })
        .collect();

    Ok(Json(json!({ "revoked_certificates": certs_json })))
}

/// Check if a certificate fingerprint is revoked.
/// Used internally by the gateway mTLS verification.
pub async fn is_cert_revoked(db: &crate::db::DbPool, org_id: DbUuid, fingerprint: &str) -> bool {
    gw_repo::is_cert_revoked_in_org(db, *org_id, fingerprint).await
}

/// Verify an agent's certificate fingerprint matches what we issued (pinning).
/// Returns true if the fingerprint matches the stored one for this agent.
pub async fn verify_agent_cert_pinning(
    db: &crate::db::DbPool,
    agent_id: DbUuid,
    presented_fingerprint: &str,
) -> bool {
    gw_repo::verify_agent_cert_pinning(db, *agent_id, presented_fingerprint).await
}

/// Helper: Transition all components of an agent to UNREACHABLE when gateway is blocked.
/// Returns the number of components affected.
async fn transition_gateway_agent_components_to_unreachable(
    state: &AppState,
    agent_id: DbUuid,
    gateway_id: DbUuid,
) -> i32 {
    use appcontrol_common::ComponentState;

    let components = match gw_repo::get_agent_components_for_unreachable(&state.db, *agent_id).await {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let mut affected = 0;

    for comp in &components {
        let current_state = match crate::core::fsm::get_current_state(&state.db, crate::db::bind_id(comp.id)).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Skip if already UNREACHABLE, STOPPED, or STOPPING
        match current_state {
            ComponentState::Unreachable | ComponentState::Stopped | ComponentState::Stopping => {
                continue;
            }
            _ => {}
        }

        let details_json = serde_json::json!({
            "previous_state": current_state.to_string(),
            "agent_id": agent_id.to_string(),
            "gateway_id": gateway_id.to_string(),
        });

        let result = gw_repo::insert_gateway_blocked_transition(
            &state.db,
            comp.id,
            &current_state.to_string(),
            &details_json,
        )
        .await;

        if result.is_ok() {
            affected += 1;

            state.ws_hub.broadcast(
                comp.application_id,
                appcontrol_common::WsEvent::StateChange {
                    component_id: comp.id,
                    app_id: comp.application_id,
                    component_name: Some(comp.name.clone()),
                    app_name: Some(comp.app_name.clone()),
                    from: current_state,
                    to: ComponentState::Unreachable,
                    at: chrono::Utc::now(),
                },
            );

            tracing::info!(
                component_id = %comp.id,
                component_name = %comp.name,
                from = %current_state,
                agent_id = %agent_id,
                gateway_id = %gateway_id,
                "Component transitioned to UNREACHABLE (gateway blocked)"
            );
        }
    }

    affected
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
