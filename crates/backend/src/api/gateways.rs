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
use crate::db::DbUuid;
use crate::error::{ApiError, OptionExt};
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
    pub id: DbUuid,
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
    pub site_id: Option<DbUuid>,
    pub site_name: Option<String>,
    pub site_code: Option<String>,
}

/// Site summary for grouping gateways
#[derive(Debug, Serialize)]
pub struct SiteSummary {
    pub site_id: Option<DbUuid>,
    pub site_name: String,
    pub site_code: String,
    /// DEPRECATED: Legacy zone field for backward compatibility. Same as site_code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    pub gateway_count: i64,
    pub active_gateway_id: Option<DbUuid>,
    pub failover_active: bool,
    pub gateways: Vec<GatewayListItem>,
}

pub async fn list_gateways(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    // Allow all authenticated users to view gateways (read-only)
    let gateways = sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            Option<String>,
            bool,
            bool,
            i32,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            i64,
            Option<DbUuid>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"SELECT
               g.id,
               g.name,
               g.zone,
               g.is_active,
               COALESCE(g.is_primary, false) as is_primary,
               COALESCE(g.priority, 0) as priority,
               g.version,
               g.last_heartbeat_at,
               COALESCE((SELECT COUNT(*) FROM agents a WHERE a.gateway_id = g.id), 0) as agent_count,
               g.site_id,
               s.name as site_name,
               s.code as site_code
           FROM gateways g
           LEFT JOIN sites s ON s.id = g.site_id
           WHERE g.organization_id = $1
           ORDER BY COALESCE(s.name, 'zzz'), g.priority, g.name"#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    // Get the set of gateways actually connected via WebSocket
    let connected_ids: std::collections::HashSet<Uuid> =
        state.ws_hub.connected_gateway_ids().into_iter().collect();

    // Group by site_id and compute failover status
    // Key is (site_id, site_name, site_code)
    let mut sites_map: std::collections::HashMap<
        (Option<DbUuid>, String, String),
        Vec<GatewayListItem>,
    > = std::collections::HashMap::new();

    for (
        id,
        name,
        zone,
        is_active,
        is_primary,
        priority,
        version,
        last_heartbeat,
        agent_count,
        site_id,
        site_name,
        site_code,
    ) in gateways
    {
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
            id: DbUuid::from(id),
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
            site_id: site_id.map(DbUuid::from),
            site_name: site_name.clone(),
            site_code: site_code.clone(),
        };

        // Group by site - use "Unassigned" for gateways without a site
        let key = (
            site_id.map(DbUuid::from),
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

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"SELECT id, organization_id, name, zone, hostname, port, site_id,
                  certificate_fingerprint, is_active,
                  COALESCE(is_primary, false) as is_primary,
                  COALESCE(priority, 0) as priority,
                  version, last_heartbeat_at, created_at
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

    // If setting as primary, first unset any existing primary in the same site
    if req.is_primary == Some(true) {
        let gw_info: Option<(Option<DbUuid>, String)> = sqlx::query_as(
            "SELECT site_id, zone FROM gateways WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(user.organization_id)
        .fetch_optional(&state.db)
        .await?;

        if let Some((site_id, zone)) = gw_info {
            if let Some(sid) = site_id {
                // Unset primary within same site
                sqlx::query(
                    "UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND site_id = $2 AND id != $3",
                )
                .bind(user.organization_id)
                .bind(sid)
                .bind(id)
                .execute(&state.db)
                .await?;
            } else {
                // Fallback: use zone for gateways without site assignment
                sqlx::query(
                    "UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND zone = $2 AND site_id IS NULL AND id != $3",
                )
                .bind(user.organization_id)
                .bind(&zone)
                .bind(id)
                .execute(&state.db)
                .await?;
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

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"UPDATE gateways SET
               name = COALESCE($3, name),
               site_id = COALESCE($4, site_id),
               is_active = COALESCE($5, is_active),
               is_primary = COALESCE($6, is_primary),
               priority = COALESCE($7, priority)
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, zone, hostname, port, site_id,
                     certificate_fingerprint, is_active,
                     COALESCE(is_primary, false) as is_primary,
                     COALESCE(priority, 0) as priority,
                     version, last_heartbeat_at, created_at"#,
    )
    .bind(id)
    .bind(user.organization_id)
    .bind(&req.name)
    .bind(req.site_id)
    .bind(req.is_active)
    .bind(req.is_primary)
    .bind(req.priority)
    .fetch_optional(&state.db)
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
    let gw: Option<(Option<Uuid>, String)> =
        sqlx::query_as("SELECT site_id, zone FROM gateways WHERE id = $1 AND organization_id = $2")
            .bind(gateway_id)
            .bind(user.organization_id)
            .fetch_optional(&state.db)
            .await?;

    let (site_id, zone) = match gw {
        Some((s, z)) => (s, z),
        None => return Err(ApiError::NotFound),
    };

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
            .bind(user.organization_id)
            .bind(sid)
            .bind(gateway_id)
            .execute(&mut *tx)
            .await?;
    } else {
        // Fallback: use zone for gateways without site assignment
        sqlx::query("UPDATE gateways SET is_primary = false WHERE organization_id = $1 AND zone = $2 AND site_id IS NULL AND id != $3")
            .bind(user.organization_id)
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
    .bind(user.organization_id)
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

    let agents = sqlx::query_as::<_, (DbUuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT id, hostname, is_active, last_heartbeat_at
           FROM agents
           WHERE gateway_id = $1 AND organization_id = $2
           ORDER BY hostname"#,
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    // Get live connection status from the WebSocket hub
    let connected_agents = state.ws_hub.connected_agent_ids();
    let connected_set: std::collections::HashSet<Uuid> = connected_agents.into_iter().collect();

    let agents_json: Vec<Value> = agents
        .into_iter()
        .map(|(id, hostname, is_active, last_heartbeat_at)| {
            let connected = connected_set.contains(&id);
            json!({
                "id": id,
                "hostname": hostname,
                "is_active": is_active,
                "last_heartbeat_at": last_heartbeat_at,
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

    let gw = sqlx::query_as::<_, GatewayRow>(
        r#"UPDATE gateways SET is_active = false
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, zone, hostname, port, site_id,
                     certificate_fingerprint, is_active,
                     COALESCE(is_primary, false) as is_primary,
                     COALESCE(priority, 0) as priority,
                     version, last_heartbeat_at, created_at"#,
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
                     certificate_fingerprint, is_active,
                     COALESCE(is_primary, false) as is_primary,
                     COALESCE(priority, 0) as priority,
                     version, last_heartbeat_at, created_at"#,
    )
    .bind(gateway_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    // Send ClearBlocklist to the gateway so all agents can reconnect
    // This fixes the bug where agents remain blocked after gateway activation
    let clear_msg = appcontrol_common::GatewayEnvelope::ClearBlocklist;
    if let Ok(json) = serde_json::to_string(&clear_msg) {
        state.ws_hub.send_to_gateway(gateway_id, &json);
        tracing::info!(
            gateway_id = %gateway_id,
            "Sent ClearBlocklist to gateway after activation"
        );
    }

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
    let gw: Option<(String, String)> =
        sqlx::query_as("SELECT name, zone FROM gateways WHERE id = $1 AND organization_id = $2")
            .bind(gateway_id)
            .bind(user.organization_id)
            .fetch_optional(&state.db)
            .await?;

    let (name, zone) = match gw {
        Some(g) => g,
        None => return Err(ApiError::NotFound),
    };

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
    sqlx::query("UPDATE gateways SET is_active = false WHERE id = $1")
        .bind(gateway_id)
        .execute(&mut *tx)
        .await?;

    // 2. Get all agents connected to this gateway
    let agent_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM agents WHERE gateway_id = $1 AND organization_id = $2")
            .bind(gateway_id)
            .bind(user.organization_id)
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
    .bind(user.organization_id)
    .bind(gateway_id)
    .execute(&mut *tx)
    .await
    .ok(); // Don't fail if table doesn't exist yet

    // 5. Transition all components of affected agents to UNREACHABLE
    let mut components_affected = 0;
    for agent_id in &agent_ids {
        components_affected +=
            transition_gateway_agent_components_to_unreachable(&state, DbUuid::from(*agent_id), DbUuid::from(gateway_id)).await;
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
pub async fn is_cert_revoked(db: &crate::db::DbPool, org_id: DbUuid, fingerprint: &str) -> bool {
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
    db: &crate::db::DbPool,
    agent_id: DbUuid,
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

/// Helper: Transition all components of an agent to UNREACHABLE when gateway is blocked.
/// Returns the number of components affected.
async fn transition_gateway_agent_components_to_unreachable(
    state: &AppState,
    agent_id: DbUuid,
    gateway_id: DbUuid,
) -> i32 {
    use appcontrol_common::ComponentState;

    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        id: DbUuid,
        name: String,
        application_id: DbUuid,
        app_name: String,
    }

    let components: Vec<ComponentInfo> = sqlx::query_as(
        r#"SELECT c.id, c.name, c.application_id, a.name AS app_name
           FROM components c
           JOIN applications a ON c.application_id = a.id
           WHERE c.agent_id = $1"#,
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut affected = 0;

    for comp in &components {
        let current_state = match crate::core::fsm::get_current_state(&state.db, comp.id).await {
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

        let result = sqlx::query(
            r#"
            INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
            VALUES ($1, $2, 'UNREACHABLE', 'gateway_blocked',
                    jsonb_build_object('previous_state', $2, 'agent_id', $3::text, 'gateway_id', $4::text))
            "#,
        )
        .bind(comp.id)
        .bind(current_state.to_string())
        .bind(agent_id.to_string())
        .bind(gateway_id.to_string())
        .execute(&state.db)
        .await;

        if result.is_ok() {
            affected += 1;

            state.ws_hub.broadcast(
                comp.application_id,
                appcontrol_common::WsEvent::StateChange {
                    component_id: *comp.id,
                    app_id: *comp.application_id,
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
