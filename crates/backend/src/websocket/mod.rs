pub mod hub;
pub mod log_subscriptions;
pub mod pending_requests;

pub use hub::Hub;
pub use log_subscriptions::LogSubscriptionManager;
pub use pending_requests::PendingLogRequests;

use axum::{
    extract::{ws, Query, State},
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::auth::jwt;
use crate::db::DbUuid;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
}

/// Frontend client WebSocket endpoint (requires JWT).
pub async fn ws_handler(
    ws: ws::WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
    // Validate JWT
    let claims = match jwt::validate_token(
        &query.token,
        &state.config.jwt_secret,
        &state.config.jwt_issuer,
    ) {
        Ok(c) => c,
        Err(_) => {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let user_id: uuid::Uuid = match claims.sub.parse() {
        Ok(id) => id,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };

    let is_admin = claims.role == "admin";
    let org_id: uuid::Uuid = claims.org.parse().unwrap_or_default();

    ws.on_upgrade(move |socket| handle_client_socket(socket, state, user_id, is_admin, org_id))
        .into_response()
}

/// Gateway WebSocket endpoint (internal, no JWT).
/// The gateway connects here to relay agent messages and receive commands.
pub async fn gateway_ws_handler(
    ws: ws::WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_gateway_socket(socket, state))
}

async fn handle_client_socket(
    socket: ws::WebSocket,
    state: Arc<AppState>,
    user_id: uuid::Uuid,
    is_admin: bool,
    _org_id: uuid::Uuid,
) {
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();

    // Register this connection
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let conn_id = uuid::Uuid::new_v4();
    state.ws_hub.add_connection(conn_id, user_id, tx);

    // Forward messages from hub to client
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process frontend subscription messages with permission checking
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                if let Ok(client_msg) =
                    serde_json::from_str::<appcontrol_common::WsClientMessage>(&text)
                {
                    match client_msg {
                        appcontrol_common::WsClientMessage::Subscribe { app_id } => {
                            // Permission check: user must have at least View permission
                            let perm = crate::core::permissions::effective_permission(
                                &state_clone.db,
                                user_id,
                                app_id,
                                is_admin,
                            )
                            .await;
                            if perm >= appcontrol_common::PermissionLevel::View {
                                state_clone.ws_hub.subscribe(conn_id, app_id);
                                tracing::debug!(
                                    user_id = %user_id,
                                    app_id = %app_id,
                                    "WebSocket subscription approved (perm={:?})",
                                    perm
                                );
                            } else {
                                tracing::warn!(
                                    user_id = %user_id,
                                    app_id = %app_id,
                                    "WebSocket subscription DENIED — insufficient permission"
                                );
                                // Send a denial event to the client
                                let deny_event = serde_json::json!({
                                    "type": "SubscriptionDenied",
                                    "payload": {
                                        "app_id": app_id,
                                        "reason": "insufficient_permission"
                                    }
                                });
                                if let Some(conn_user_id) =
                                    state_clone.ws_hub.get_connection_user_id(conn_id)
                                {
                                    // Log the attempt in action_log
                                    let _ = crate::middleware::audit::log_action(
                                        &state_clone.db,
                                        conn_user_id,
                                        "ws_subscribe_denied",
                                        "application",
                                        app_id,
                                        deny_event,
                                    )
                                    .await;
                                }
                            }
                        }
                        appcontrol_common::WsClientMessage::Unsubscribe { app_id } => {
                            state_clone.ws_hub.unsubscribe(conn_id, app_id);
                        }
                        appcontrol_common::WsClientMessage::TerminalStart {
                            agent_id,
                            shell,
                            cols,
                            rows,
                        } => {
                            // Terminal access is admin-only
                            if !is_admin {
                                tracing::warn!(
                                    user_id = %user_id,
                                    agent_id = %agent_id,
                                    "Terminal access DENIED — admin only"
                                );
                                // Send error back to client
                                let error_event = appcontrol_common::WsEvent::TerminalError {
                                    session_id: uuid::Uuid::nil(),
                                    error: "Terminal access requires administrator privileges"
                                        .to_string(),
                                };
                                if let Ok(json) = serde_json::to_string(&error_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                                continue;
                            }

                            // Check if agent is connected
                            if !state_clone.ws_hub.is_agent_connected(agent_id) {
                                tracing::warn!(
                                    user_id = %user_id,
                                    agent_id = %agent_id,
                                    "Terminal start failed — agent not connected"
                                );
                                let error_event = appcontrol_common::WsEvent::TerminalError {
                                    session_id: uuid::Uuid::nil(),
                                    error: "Agent is not connected".to_string(),
                                };
                                if let Ok(json) = serde_json::to_string(&error_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                                continue;
                            }

                            // Create session
                            let (session_id, request_id) = state_clone
                                .terminal_sessions
                                .create_session(agent_id, conn_id, user_id);

                            // Log the action
                            let _ = crate::middleware::audit::log_action(
                                &state_clone.db,
                                user_id,
                                "terminal_start",
                                "agent",
                                agent_id,
                                serde_json::json!({
                                    "session_id": session_id,
                                    "shell": shell,
                                }),
                            )
                            .await;

                            // Send StartTerminal to agent
                            let start_msg = appcontrol_common::BackendMessage::StartTerminal {
                                request_id,
                                shell,
                                cols,
                                rows,
                                env: std::collections::HashMap::new(),
                            };

                            if state_clone.ws_hub.send_to_agent(agent_id, start_msg) {
                                // Send TerminalStarted to frontend
                                let started_event = appcontrol_common::WsEvent::TerminalStarted {
                                    session_id,
                                    agent_id,
                                };
                                if let Ok(json) = serde_json::to_string(&started_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                                tracing::info!(
                                    session_id = %session_id,
                                    agent_id = %agent_id,
                                    user_id = %user_id,
                                    "Terminal session started"
                                );
                            } else {
                                // Failed to send to agent
                                state_clone.terminal_sessions.remove_session(session_id);
                                let error_event = appcontrol_common::WsEvent::TerminalError {
                                    session_id,
                                    error: "Failed to send command to agent".to_string(),
                                };
                                if let Ok(json) = serde_json::to_string(&error_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                            }
                        }
                        appcontrol_common::WsClientMessage::TerminalInput { session_id, data } => {
                            // Decode base64 input
                            let bytes = match base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                &data,
                            ) {
                                Ok(b) => b,
                                Err(_) => {
                                    tracing::debug!(session_id = %session_id, "Invalid base64 terminal input");
                                    continue;
                                }
                            };

                            // Look up session - extract fields and drop lock to avoid deadlock
                            let session_info = state_clone
                                .terminal_sessions
                                .get_session(session_id)
                                .map(|s| (s.conn_id, s.request_id, s.agent_id));

                            if let Some((session_conn_id, request_id, agent_id)) = session_info {
                                // Verify this connection owns the session
                                if session_conn_id != conn_id {
                                    tracing::warn!(
                                        session_id = %session_id,
                                        "Terminal input from wrong connection"
                                    );
                                    continue;
                                }

                                state_clone.terminal_sessions.touch_session(session_id);

                                // Forward to agent
                                let input_msg = appcontrol_common::BackendMessage::TerminalInput {
                                    request_id,
                                    data: bytes,
                                };
                                state_clone.ws_hub.send_to_agent(agent_id, input_msg);
                            }
                        }
                        appcontrol_common::WsClientMessage::TerminalResize {
                            session_id,
                            cols,
                            rows,
                        } => {
                            // Extract fields and drop lock to avoid deadlock
                            let session_info = state_clone
                                .terminal_sessions
                                .get_session(session_id)
                                .map(|s| (s.conn_id, s.request_id, s.agent_id));

                            if let Some((session_conn_id, request_id, agent_id)) = session_info {
                                if session_conn_id != conn_id {
                                    continue;
                                }

                                state_clone.terminal_sessions.touch_session(session_id);

                                let resize_msg =
                                    appcontrol_common::BackendMessage::TerminalResize {
                                        request_id,
                                        cols,
                                        rows,
                                    };
                                state_clone.ws_hub.send_to_agent(agent_id, resize_msg);
                            }
                        }
                        appcontrol_common::WsClientMessage::TerminalClose { session_id } => {
                            if let Some(session) =
                                state_clone.terminal_sessions.remove_session(session_id)
                            {
                                if session.conn_id != conn_id {
                                    // Put it back - wrong connection
                                    continue;
                                }

                                // Log the action
                                let _ = crate::middleware::audit::log_action(
                                    &state_clone.db,
                                    user_id,
                                    "terminal_close",
                                    "agent",
                                    session.agent_id,
                                    serde_json::json!({
                                        "session_id": session_id,
                                    }),
                                )
                                .await;

                                // Send close to agent
                                let close_msg = appcontrol_common::BackendMessage::TerminalClose {
                                    request_id: session.request_id,
                                };
                                state_clone
                                    .ws_hub
                                    .send_to_agent(session.agent_id, close_msg);

                                tracing::info!(
                                    session_id = %session_id,
                                    "Terminal session closed by user"
                                );
                            }
                        }
                        appcontrol_common::WsClientMessage::LogSubscribe {
                            agent_id,
                            gateway_id,
                            min_level,
                        } => {
                            // Log viewing is admin-only
                            if !is_admin {
                                tracing::warn!(
                                    user_id = %user_id,
                                    "Log subscription DENIED — admin only"
                                );
                                // Send error back to client
                                let error_event = serde_json::json!({
                                    "type": "LogSubscriptionDenied",
                                    "payload": {
                                        "agent_id": agent_id,
                                        "gateway_id": gateway_id,
                                        "reason": "Log viewing requires administrator privileges"
                                    }
                                });
                                if let Ok(json) = serde_json::to_string(&error_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                                continue;
                            }

                            if let Some(aid) = agent_id {
                                state_clone.log_subscriptions.subscribe_agent(
                                    conn_id,
                                    aid,
                                    min_level.clone(),
                                );
                                // Check if agent is connected and send confirmation
                                let agent_connected = state_clone.ws_hub.is_agent_connected(aid);
                                let confirm_event = serde_json::json!({
                                    "type": "LogSubscriptionConfirmed",
                                    "payload": {
                                        "agent_id": aid,
                                        "connected": agent_connected,
                                        "min_level": min_level
                                    }
                                });
                                if let Ok(json) = serde_json::to_string(&confirm_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                                tracing::debug!(
                                    user_id = %user_id,
                                    agent_id = %aid,
                                    min_level = %min_level,
                                    agent_connected = agent_connected,
                                    "Log subscription added for agent"
                                );
                            }
                            if let Some(gid) = gateway_id {
                                state_clone.log_subscriptions.subscribe_gateway(
                                    conn_id,
                                    gid,
                                    min_level.clone(),
                                );
                                // Send confirmation for gateway subscription
                                let confirm_event = serde_json::json!({
                                    "type": "LogSubscriptionConfirmed",
                                    "payload": {
                                        "gateway_id": gid,
                                        "connected": true,  // Gateway is always connected if reachable
                                        "min_level": min_level
                                    }
                                });
                                if let Ok(json) = serde_json::to_string(&confirm_event) {
                                    state_clone.ws_hub.send_to_connection(conn_id, json);
                                }
                                tracing::debug!(
                                    user_id = %user_id,
                                    gateway_id = %gid,
                                    min_level = %min_level,
                                    "Log subscription added for gateway"
                                );
                            }
                        }
                        appcontrol_common::WsClientMessage::LogUnsubscribe {
                            agent_id,
                            gateway_id,
                        } => {
                            if let Some(aid) = agent_id {
                                state_clone
                                    .log_subscriptions
                                    .unsubscribe_agent(conn_id, aid);
                                tracing::debug!(
                                    user_id = %user_id,
                                    agent_id = %aid,
                                    "Log subscription removed for agent"
                                );
                            }
                            if let Some(gid) = gateway_id {
                                state_clone
                                    .log_subscriptions
                                    .unsubscribe_gateway(conn_id, gid);
                                tracing::debug!(
                                    user_id = %user_id,
                                    gateway_id = %gid,
                                    "Log subscription removed for gateway"
                                );
                            }
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.ws_hub.remove_connection(conn_id);
    state.log_subscriptions.remove_connection(conn_id);
}

/// Handle a gateway WebSocket connection.
/// The gateway identifies itself with a Register message, then relays agent messages.
async fn handle_gateway_socket(socket: ws::WebSocket, state: Arc<AppState>) {
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();

    // Channel for sending commands from Hub to this gateway
    let (gw_tx, mut gw_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // The gateway_id will be set when the gateway sends its Register message
    let gateway_id: Arc<std::sync::Mutex<Option<uuid::Uuid>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Forward commands from hub to gateway
    let send_task = tokio::spawn(async move {
        while let Some(msg) = gw_rx.recv().await {
            if sender.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process messages from the gateway
    let state_clone = state.clone();
    let gw_id_clone = gateway_id.clone();
    let gw_tx_clone = gw_tx;
    let recv_task = tokio::spawn(async move {
        tracing::debug!("Gateway recv_task started");
        loop {
            tracing::debug!("Waiting for next gateway message...");
            match receiver.next().await {
                Some(Ok(msg)) => {
                    if let ws::Message::Text(text) = msg {
                        tracing::debug!(
                            bytes = text.len(),
                            "Received WebSocket message from gateway"
                        );
                        match serde_json::from_str::<appcontrol_common::GatewayMessage>(&text) {
                            Ok(gw_msg) => {
                                process_gateway_message(
                                    &state_clone,
                                    &gw_id_clone,
                                    &gw_tx_clone,
                                    gw_msg,
                                )
                                .await;
                                tracing::debug!("Finished processing gateway message");
                            }
                            Err(_) => {
                                // Backwards compatibility: try parsing as raw AgentMessage
                                if let Ok(agent_msg) =
                                    serde_json::from_str::<appcontrol_common::AgentMessage>(&text)
                                {
                                    // No gateway_id available in backwards compat mode
                                    process_agent_message(&state_clone, agent_msg, None).await;
                                } else {
                                    tracing::warn!("Unknown message from gateway");
                                }
                            }
                        }
                    } else {
                        tracing::debug!(msg_type = ?msg, "Non-text message from gateway");
                    }
                }
                Some(Err(e)) => {
                    tracing::error!(error = %e, "Error receiving from gateway");
                    break;
                }
                None => {
                    tracing::debug!("Gateway WebSocket stream ended");
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    // Cleanup: unregister gateway and all its agent routes
    let gw_id = gateway_id.lock().unwrap().take();
    if let Some(id) = gw_id {
        state.ws_hub.unregister_gateway(id);
        tracing::info!(gateway_id = %id, "Gateway disconnected from backend");
    } else {
        tracing::info!("Unknown gateway disconnected (never registered)");
    }
}

/// Process a typed GatewayMessage from a registered gateway.
async fn process_gateway_message(
    state: &Arc<AppState>,
    gateway_id_cell: &Arc<std::sync::Mutex<Option<uuid::Uuid>>>,
    gw_tx: &tokio::sync::mpsc::UnboundedSender<String>,
    msg: appcontrol_common::GatewayMessage,
) {
    match msg {
        appcontrol_common::GatewayMessage::Register {
            gateway_id,
            name,
            zone,
            site_id,
            version,
            enrollment_token,
        } => {
            // ── Enrollment token validation ──
            // In production, the gateway should present an enrollment token.
            // The token determines which organization the gateway belongs to.
            let org_id: Option<uuid::Uuid> = if let Some(ref token) = enrollment_token {
                // Validate the enrollment token and extract org_id
                match validate_gateway_enrollment_token(&state.db, token).await {
                    Ok(org) => Some(org),
                    Err(reason) => {
                        tracing::warn!(
                            gateway_id = %gateway_id,
                            reason = %reason,
                            "REJECTED: invalid gateway enrollment token"
                        );
                        let disconnect = appcontrol_common::GatewayEnvelope::DisconnectGateway {
                            reason: reason.to_string(),
                        };
                        if let Ok(json) = serde_json::to_string(&disconnect) {
                            let _ = gw_tx.send(json);
                        }
                        return;
                    }
                }
            } else {
                // No token provided — check if dev mode (single org) or reject
                // In dev mode with a single org, we allow unauthenticated gateway registration
                #[cfg(feature = "postgres")]
                let single_org: Option<uuid::Uuid> =
                    sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
                        .fetch_optional(&state.db)
                        .await
                        .ok()
                        .flatten();
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let single_org: Option<uuid::Uuid> = {
                    let row: Option<DbUuid> =
                        sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
                            .fetch_optional(&state.db)
                            .await
                            .ok()
                            .flatten();
                    row.map(|u| u.into_inner())
                };

                if single_org.is_some() {
                    tracing::warn!(
                        gateway_id = %gateway_id,
                        "Gateway registered without enrollment token (dev mode - using single org)"
                    );
                }
                single_org
            };

            let org_id = match org_id {
                Some(id) => id,
                None => {
                    tracing::warn!(
                        gateway_id = %gateway_id,
                        "REJECTED: no organization found and no enrollment token provided"
                    );
                    let disconnect = appcontrol_common::GatewayEnvelope::DisconnectGateway {
                        reason: "Enrollment token required (no default organization)".to_string(),
                    };
                    if let Ok(json) = serde_json::to_string(&disconnect) {
                        let _ = gw_tx.send(json);
                    }
                    return;
                }
            };

            // ── Gateway active check ──
            // If the gateway is blocked (is_active = false), reject the connection.
            #[cfg(feature = "postgres")]
            let is_active: bool =
                sqlx::query_scalar("SELECT COALESCE(is_active, true) FROM gateways WHERE id = $1")
                    .bind(gateway_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(true); // Default to active if gateway doesn't exist (first-time registration)
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let is_active: bool = {
                let val: Option<i32> =
                    sqlx::query_scalar("SELECT COALESCE(is_active, 1) FROM gateways WHERE id = $1")
                        .bind(DbUuid::from(gateway_id))
                        .fetch_optional(&state.db)
                        .await
                        .ok()
                        .flatten();
                val.unwrap_or(1) != 0
            };

            if !is_active {
                tracing::warn!(
                    gateway_id = %gateway_id,
                    "REJECTED: gateway is blocked (is_active = false)"
                );
                // Send disconnect message to the gateway
                let disconnect = appcontrol_common::GatewayEnvelope::DisconnectGateway {
                    reason: "Gateway has been blocked by administrator".to_string(),
                };
                if let Ok(json) = serde_json::to_string(&disconnect) {
                    let _ = gw_tx.send(json);
                }
                return;
            }

            tracing::info!(
                gateway_id = %gateway_id,
                name = ?name,
                zone = ?zone,
                site_id = ?site_id,
                version = %version,
                org_id = %org_id,
                has_token = enrollment_token.is_some(),
                "Gateway registered"
            );
            // Store the gateway_id for this connection
            *gateway_id_cell.lock().unwrap() = Some(gateway_id);
            // Register in the hub with the sender channel
            // Use zone for display purposes (legacy), but site_id for grouping
            let display_zone = zone.clone().unwrap_or_else(|| "default".to_string());
            state
                .ws_hub
                .register_gateway(gateway_id, display_zone, gw_tx.clone());

            // Use the provided name, or generate one from the gateway_id
            let display_name =
                name.unwrap_or_else(|| format!("Gateway-{}", &gateway_id.to_string()[..8]));

            // Resolve site_id: prefer explicit site_id, fallback to zone lookup
            let resolved_site_id: Option<uuid::Uuid> = if site_id.is_some() {
                site_id
            } else if let Some(ref z) = zone {
                // Backward compat: look up site by code matching zone
                #[cfg(feature = "postgres")]
                let result: Option<uuid::Uuid> =
                    sqlx::query_scalar("SELECT id FROM sites WHERE organization_id = $1 AND code = $2")
                        .bind(org_id)
                        .bind(z)
                        .fetch_optional(&state.db)
                        .await
                        .ok()
                        .flatten();
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let result: Option<uuid::Uuid> = {
                    let row: Option<DbUuid> =
                        sqlx::query_scalar("SELECT id FROM sites WHERE organization_id = $1 AND code = $2")
                            .bind(DbUuid::from(org_id))
                            .bind(z)
                            .fetch_optional(&state.db)
                            .await
                            .ok()
                            .flatten();
                    row.map(|u| u.into_inner())
                };
                result
            } else {
                None
            };

            // Auto-register/update gateway in database (upsert)
            // This ensures the gateway appears in the UI even if not pre-created
            // Note: We only update last_heartbeat_at, NOT is_active (admin controls that)
            //
            // For new gateways: auto-assign is_primary and priority based on site_id:
            // - First gateway in a site becomes primary (is_primary=true, priority=0)
            // - Subsequent gateways are standby (is_primary=false, priority=N)
            // - Gateways without site_id are assigned priority 0 and not primary
            #[cfg(feature = "postgres")]
            if let Err(e) = sqlx::query(
                r#"
                WITH site_info AS (
                    SELECT
                        COALESCE(MAX(priority), -1) + 1 AS next_priority,
                        COUNT(*) = 0 AS is_first_in_site
                    FROM gateways
                    WHERE organization_id = $2
                      AND (($5::uuid IS NOT NULL AND site_id = $5) OR ($5::uuid IS NULL AND site_id IS NULL))
                      AND id != $1
                )
                INSERT INTO gateways (id, organization_id, name, zone, site_id, is_active, is_primary, priority, last_heartbeat_at)
                SELECT $1, $2, $3, $4, $5, true,
                       CASE WHEN $5::uuid IS NOT NULL THEN si.is_first_in_site ELSE false END,
                       si.next_priority,
                       now()
                FROM site_info si
                ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    zone = COALESCE(EXCLUDED.zone, gateways.zone),
                    site_id = COALESCE(EXCLUDED.site_id, gateways.site_id),
                    last_heartbeat_at = now()
                "#,
            )
            .bind(gateway_id)
            .bind(org_id)
            .bind(&display_name)
            .bind(&zone)
            .bind(resolved_site_id)
            .execute(&state.db)
            .await
            {
                tracing::warn!(gateway_id = %gateway_id, "Failed to upsert gateway record: {}", e);
            }

            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            if let Err(e) = sqlx::query(
                "INSERT INTO gateways (id, organization_id, name, zone, site_id, is_active, last_heartbeat_at) \
                 VALUES ($1, $2, $3, $4, $5, 1, datetime('now')) \
                 ON CONFLICT (id) DO UPDATE SET \
                     name = EXCLUDED.name, \
                     zone = COALESCE(EXCLUDED.zone, gateways.zone), \
                     site_id = COALESCE(EXCLUDED.site_id, gateways.site_id), \
                     last_heartbeat_at = datetime('now')",
            )
            .bind(DbUuid::from(gateway_id))
            .bind(DbUuid::from(org_id))
            .bind(&display_name)
            .bind(&zone)
            .bind(resolved_site_id.map(DbUuid::from))
            .execute(&state.db)
            .await
            {
                tracing::warn!(gateway_id = %gateway_id, "Failed to upsert gateway record: {}", e);
            }
        }
        appcontrol_common::GatewayMessage::AgentMessage(agent_msg) => {
            // Get the gateway_id from the cell for auto-registration
            let gw_id = { *gateway_id_cell.lock().unwrap() };
            process_agent_message(state, agent_msg, gw_id).await;
        }
        appcontrol_common::GatewayMessage::AgentConnected {
            agent_id,
            hostname,
            version,
            cert_fingerprint,
            cert_cn,
        } => {
            // Copy the value out and drop the MutexGuard before any .await
            let gw_id = { *gateway_id_cell.lock().unwrap() };
            if let Some(gw_id) = gw_id {
                // ── Agent active check ──
                // If the agent is blocked (is_active = false), reject the connection.
                #[cfg(feature = "postgres")]
                let is_active: bool = sqlx::query_scalar(
                    "SELECT COALESCE(is_active, true) FROM agents WHERE id = $1",
                )
                .bind(agent_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .unwrap_or(true); // Default to active if agent doesn't exist (first-time registration)
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let is_active: bool = {
                    let val: Option<i32> = sqlx::query_scalar(
                        "SELECT COALESCE(is_active, 1) FROM agents WHERE id = $1",
                    )
                    .bind(DbUuid::from(agent_id))
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();
                    val.unwrap_or(1) != 0
                };

                if !is_active {
                    tracing::warn!(
                        agent_id = %agent_id,
                        "REJECTED: agent is blocked (is_active = false)"
                    );
                    // Register route temporarily so we can send the disconnect message
                    state.ws_hub.register_agent_route(agent_id, gw_id);
                    let disconnect = appcontrol_common::BackendMessage::DisconnectAgent {
                        agent_id,
                        reason: "Agent has been blocked by administrator".to_string(),
                    };
                    state.ws_hub.send_to_agent(agent_id, disconnect);
                    state.ws_hub.unregister_agent_route(agent_id);
                    return;
                }

                // ── Certificate revocation check ──
                // If the agent presents a cert fingerprint, check if it's been revoked.
                if let Some(ref fp) = cert_fingerprint {
                    #[cfg(feature = "postgres")]
                    let is_revoked: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM revoked_certificates WHERE fingerprint = $1)",
                    )
                    .bind(fp)
                    .fetch_one(&state.db)
                    .await
                    .unwrap_or(false);
                    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                    let is_revoked: bool = {
                        let count: i32 = sqlx::query_scalar(
                            "SELECT COUNT(*) FROM revoked_certificates WHERE fingerprint = $1",
                        )
                        .bind(fp)
                        .fetch_one(&state.db)
                        .await
                        .unwrap_or(0);
                        count > 0
                    };

                    if is_revoked {
                        tracing::warn!(
                            agent_id = %agent_id,
                            fingerprint = %fp,
                            "REJECTED: agent presented a revoked certificate"
                        );
                        // Register route temporarily so we can send the disconnect message
                        state.ws_hub.register_agent_route(agent_id, gw_id);
                        let disconnect = appcontrol_common::BackendMessage::DisconnectAgent {
                            agent_id,
                            reason: "Certificate has been revoked".to_string(),
                        };
                        state.ws_hub.send_to_agent(agent_id, disconnect);
                        state.ws_hub.unregister_agent_route(agent_id);
                        return;
                    }
                }

                // ── Certificate pinning check ──
                // If the agent already has a stored fingerprint, verify it matches.
                if let Some(ref fp) = cert_fingerprint {
                    #[cfg(feature = "postgres")]
                    let stored_fp: Option<Option<String>> = sqlx::query_scalar(
                        "SELECT certificate_fingerprint FROM agents WHERE id = $1",
                    )
                    .bind(agent_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();
                    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                    let stored_fp: Option<Option<String>> = sqlx::query_scalar(
                        "SELECT certificate_fingerprint FROM agents WHERE id = $1",
                    )
                    .bind(DbUuid::from(agent_id))
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();

                    if let Some(Some(ref stored)) = stored_fp {
                        if !stored.is_empty() && stored != fp {
                            tracing::warn!(
                                agent_id = %agent_id,
                                stored = %stored,
                                presented = %fp,
                                "REJECTED: agent cert fingerprint mismatch (possible impersonation)"
                            );
                            // Register route temporarily so we can send the disconnect message
                            state.ws_hub.register_agent_route(agent_id, gw_id);
                            let disconnect = appcontrol_common::BackendMessage::DisconnectAgent {
                                agent_id,
                                reason: "Certificate fingerprint does not match enrolled identity"
                                    .to_string(),
                            };
                            state.ws_hub.send_to_agent(agent_id, disconnect);
                            state.ws_hub.unregister_agent_route(agent_id);
                            return;
                        }
                    }
                }

                tracing::info!(
                    agent_id = %agent_id,
                    hostname = %hostname,
                    gateway_id = %gw_id,
                    identity_verified = cert_fingerprint.is_some(),
                    "Agent connected via gateway"
                );
                state.ws_hub.register_agent_route(agent_id, gw_id);

                // Push component config to the agent so its scheduler starts health checks.
                send_config_to_agent(state, agent_id).await;

                // Send RunChecksNow to trigger immediate health checks.
                // This ensures components transition from UNREACHABLE to their actual state
                // without waiting for the next scheduled check interval.
                send_run_checks_now(state, agent_id);

                // Update agent record: gateway_id, certificate fingerprint, version
                #[cfg(feature = "postgres")]
                if let Err(e) = sqlx::query(
                    "UPDATE agents SET gateway_id = $2, \
                     certificate_fingerprint = COALESCE($3, certificate_fingerprint), \
                     certificate_cn = COALESCE($4, certificate_cn), \
                     version = COALESCE($5, version), \
                     identity_verified = ($3 IS NOT NULL) \
                     WHERE id = $1",
                )
                .bind(agent_id)
                .bind(gw_id)
                .bind(&cert_fingerprint)
                .bind(&cert_cn)
                .bind(&version)
                .execute(&state.db)
                .await
                {
                    tracing::warn!(
                        agent_id = %agent_id,
                        "Failed to update agent record: {}", e
                    );
                }
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                if let Err(e) = sqlx::query(
                    "UPDATE agents SET gateway_id = $2, \
                     certificate_fingerprint = COALESCE($3, certificate_fingerprint), \
                     certificate_cn = COALESCE($4, certificate_cn), \
                     version = COALESCE($5, version), \
                     identity_verified = ($3 IS NOT NULL) \
                     WHERE id = $1",
                )
                .bind(DbUuid::from(agent_id))
                .bind(DbUuid::from(gw_id))
                .bind(&cert_fingerprint)
                .bind(&cert_cn)
                .bind(&version)
                .execute(&state.db)
                .await
                {
                    tracing::warn!(
                        agent_id = %agent_id,
                        "Failed to update agent record: {}", e
                    );
                }
            } else {
                tracing::warn!(
                    agent_id = %agent_id,
                    "AgentConnected received before gateway registration"
                );
            }
        }
        appcontrol_common::GatewayMessage::AgentDisconnected { agent_id, hostname } => {
            tracing::info!(
                agent_id = %agent_id,
                hostname = %hostname,
                "Agent disconnected from gateway"
            );
            state.ws_hub.unregister_agent_route(agent_id);

            // Immediately transition all non-stopped components to UNREACHABLE
            // This provides instant feedback to the UI when an agent disconnects
            mark_agent_components_unreachable(state, agent_id, "agent_disconnect").await;

            // NOTE: We intentionally do NOT clear gateway_id here.
            // The agent keeps its gateway association even when temporarily disconnected.
            // gateway_id is only cleared when:
            // - Agent is explicitly blocked (POST /agents/:id/block)
            // - Gateway is deleted (DELETE /gateways/:id)
            // The "connected" status is determined by the hub's routing table.
        }
        appcontrol_common::GatewayMessage::LogEntries {
            gateway_id,
            entries,
        } => {
            // Route gateway log entries to subscribed frontend connections
            let subscribers = state.log_subscriptions.get_gateway_subscribers(gateway_id);
            if subscribers.is_empty() {
                return; // No subscribers, skip processing
            }

            // Look up gateway name for display
            #[cfg(feature = "postgres")]
            let gateway_name: String =
                sqlx::query_scalar("SELECT name FROM gateways WHERE id = $1")
                    .bind(gateway_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| gateway_id.to_string());
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let gateway_name: String =
                sqlx::query_scalar("SELECT name FROM gateways WHERE id = $1")
                    .bind(DbUuid::from(gateway_id))
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| gateway_id.to_string());

            for entry in entries {
                // Create WsEvent for each log entry
                let log_event = appcontrol_common::WsEvent::LogEntry {
                    source_type: "gateway".to_string(),
                    source_id: gateway_id,
                    source_name: gateway_name.clone(),
                    level: entry.level.clone(),
                    target: entry.target,
                    message: entry.message,
                    timestamp: entry.timestamp.to_rfc3339(),
                };

                if let Ok(json) = serde_json::to_string(&log_event) {
                    for conn_id in &subscribers {
                        // Check level filter for this connection
                        if state
                            .log_subscriptions
                            .level_passes_filter(*conn_id, &entry.level)
                        {
                            state.ws_hub.send_to_connection(*conn_id, json.clone());
                        }
                    }
                }
            }
        }
        appcontrol_common::GatewayMessage::Heartbeat {
            gateway_id,
            connected_agents,
            buffer_messages,
            buffer_bytes,
        } => {
            tracing::debug!(
                gateway_id = %gateway_id,
                connected_agents = connected_agents,
                buffer_messages = buffer_messages,
                buffer_bytes = buffer_bytes,
                "Gateway heartbeat received"
            );

            // Update gateway's last heartbeat timestamp
            #[cfg(feature = "postgres")]
            let gw_hb_result = sqlx::query(&format!(
                "UPDATE gateways SET last_heartbeat_at = {} WHERE id = $1",
                crate::db::sql::now()
            ))
            .bind(gateway_id)
            .execute(&state.db)
            .await;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let gw_hb_result = sqlx::query(&format!(
                "UPDATE gateways SET last_heartbeat_at = {} WHERE id = $1",
                crate::db::sql::now()
            ))
            .bind(DbUuid::from(gateway_id))
            .execute(&state.db)
            .await;
            if let Err(e) = gw_hb_result
            {
                tracing::warn!(
                    gateway_id = %gateway_id,
                    "Failed to update gateway heartbeat: {}", e
                );
            }

            // Log warning if gateway has buffered messages (backend was unreachable)
            if buffer_messages > 0 {
                tracing::warn!(
                    gateway_id = %gateway_id,
                    buffer_messages = buffer_messages,
                    buffer_bytes = buffer_bytes,
                    "Gateway reports buffered messages (possible connectivity issues)"
                );
            }
        }
    }
}

/// Process an incoming agent message: update FSM, record events, broadcast.
/// The optional gateway_id is used for auto-registration when receiving heartbeats
/// from agents that aren't in the routing table (e.g., after gateway reconnect).
async fn process_agent_message(
    state: &Arc<AppState>,
    msg: appcontrol_common::AgentMessage,
    source_gateway_id: Option<uuid::Uuid>,
) {
    match msg {
        appcontrol_common::AgentMessage::CheckResult(cr) => {
            tracing::debug!(
                component_id = %cr.component_id,
                exit_code = cr.exit_code,
                has_metrics = cr.metrics.is_some(),
                "Processing check result"
            );

            // Store check event with metrics for audit trail
            if let Err(e) = crate::core::fsm::store_check_event(&state.db, &cr).await {
                tracing::warn!(
                    component_id = %cr.component_id,
                    "Failed to store check event: {}", e
                );
            }

            // Process FSM transition based on exit code
            if let Err(e) =
                crate::core::fsm::process_check_result(state, cr.component_id, cr.exit_code).await
            {
                tracing::warn!(
                    component_id = %cr.component_id,
                    exit_code = cr.exit_code,
                    "Failed to process check result: {}", e
                );
            }
        }
        appcontrol_common::AgentMessage::CommandResult {
            request_id,
            exit_code,
            stdout,
            stderr,
            sequence_id,
            ..
        } => {
            tracing::info!(
                request_id = %request_id,
                exit_code = exit_code,
                sequence_id = ?sequence_id,
                "Command result received"
            );
            if !stdout.is_empty() || !stderr.is_empty() {
                tracing::debug!(stdout = %stdout, stderr = %stderr, "Command output");
            }

            // Record result in command_executions for audit trail
            crate::core::sequencer::record_command_result(
                &state.db, request_id, exit_code, &stdout, &stderr,
            )
            .await;

            // Broadcast CommandResultEvent to subscribed frontend clients
            if let Ok(Some((comp_id, comp_name, app_id))) =
                sqlx::query_as::<_, (DbUuid, String, DbUuid)>(
                    r#"SELECT c.id, c.name, c.application_id
                   FROM command_executions ce
                   JOIN components c ON ce.component_id = c.id
                   WHERE ce.request_id = $1"#,
                )
                .bind(DbUuid::from(request_id))
                .fetch_optional(&state.db)
                .await
            {
                state.ws_hub.broadcast(
                    app_id,
                    appcontrol_common::WsEvent::CommandResultEvent {
                        request_id,
                        component_id: *comp_id,
                        component_name: Some(comp_name),
                        exit_code,
                        stdout: stdout.clone(),
                        stderr: stderr.clone(),
                    },
                );
            }

            // Send Ack back to agent if sequence_id was provided.
            // Uses the request_id → agent_id mapping recorded when ExecuteCommand was dispatched.
            if let Some(seq) = sequence_id {
                let ack = appcontrol_common::BackendMessage::Ack {
                    request_id,
                    sequence_id: Some(seq),
                };
                if let Some(agent_id) = state.ws_hub.resolve_request_agent(request_id) {
                    if state.ws_hub.send_to_agent(agent_id, ack) {
                        tracing::debug!(
                            request_id = %request_id,
                            agent_id = %agent_id,
                            sequence_id = seq,
                            "Ack routed to agent"
                        );
                    } else {
                        tracing::warn!(
                            request_id = %request_id,
                            agent_id = %agent_id,
                            "Failed to route Ack — agent not reachable"
                        );
                    }
                } else {
                    tracing::debug!(
                        request_id = %request_id,
                        sequence_id = seq,
                        "No agent mapping for request — Ack not routed"
                    );
                }
            }
        }
        appcontrol_common::AgentMessage::CommandOutputChunk {
            request_id,
            stdout,
            stderr,
        } => {
            tracing::trace!(
                request_id = %request_id,
                "Command output chunk received (stdout={} bytes, stderr={} bytes)",
                stdout.len(),
                stderr.len()
            );

            // Resolve component_id from command_executions to broadcast to subscribed clients
            let component_id = sqlx::query_scalar::<_, DbUuid>(
                "SELECT component_id FROM command_executions WHERE request_id = $1",
            )
            .bind(DbUuid::from(request_id))
            .fetch_optional(&state.db)
            .await;

            if let Ok(Some(comp_id)) = component_id {
                // Resolve app_id for broadcast routing
                if let Ok(Some(app_id)) = sqlx::query_scalar::<_, DbUuid>(
                    "SELECT application_id FROM components WHERE id = $1",
                )
                .bind(comp_id)
                .fetch_optional(&state.db)
                .await
                {
                    state.ws_hub.broadcast(
                        app_id,
                        appcontrol_common::WsEvent::CommandOutputChunkEvent {
                            request_id,
                            component_id: *comp_id,
                            stdout,
                            stderr,
                        },
                    );
                }
            }
        }
        appcontrol_common::AgentMessage::Heartbeat {
            agent_id,
            cpu,
            memory,
            disk,
            ..
        } => {
            tracing::trace!(agent_id = %agent_id, cpu = %cpu, memory = %memory, disk = ?disk, "Agent heartbeat");

            // Auto-register agent route if we receive a heartbeat but agent is not in routing table.
            // This handles the case where the gateway reconnected but didn't send AgentConnected.
            if let Some(gw_id) = source_gateway_id {
                if !state.ws_hub.is_agent_connected(agent_id) {
                    tracing::info!(
                        agent_id = %agent_id,
                        gateway_id = %gw_id,
                        "Auto-registering agent route from heartbeat (missing AgentConnected)"
                    );
                    state.ws_hub.register_agent_route(agent_id, gw_id);

                    // Send component config to agent first — without this, agent's scheduler
                    // doesn't know what components to health-check. This fixes drift issues
                    // where agent reconnects but doesn't receive its config.
                    let state_clone = state.clone();
                    tokio::spawn(async move {
                        send_config_to_agent(&state_clone, agent_id).await;
                    });

                    // Then trigger immediate health checks to restore component states
                    send_run_checks_now(state, agent_id);
                }

                // Also restore gateway_id in database if it was cleared (e.g., after gateway block/unblock).
                // This fixes the case where an agent's gateway_id is NULL but it's actively sending heartbeats.
                let db = state.db.clone();
                tokio::spawn(async move {
                    #[cfg(feature = "postgres")]
                    let result = sqlx::query(
                        "UPDATE agents SET gateway_id = $2 WHERE id = $1 AND gateway_id IS NULL",
                    )
                    .bind(agent_id)
                    .bind(gw_id)
                    .execute(&db)
                    .await;
                    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                    let result = sqlx::query(
                        "UPDATE agents SET gateway_id = $2 WHERE id = $1 AND gateway_id IS NULL",
                    )
                    .bind(DbUuid::from(agent_id))
                    .bind(DbUuid::from(gw_id))
                    .execute(&db)
                    .await;

                    if let Ok(res) = result {
                        if res.rows_affected() > 0 {
                            tracing::info!(
                                agent_id = %agent_id,
                                gateway_id = %gw_id,
                                "Restored agent gateway_id from heartbeat (was NULL)"
                            );
                        }
                    }
                });
            }

            // Batch heartbeat update — flushed every 5s instead of 1 SQL per heartbeat.
            // At 2500 agents this reduces PostgreSQL writes from 2500/min to ~12/min.
            state.heartbeat_batcher.record(agent_id).await;

            // Store metrics for time-series graphing (sample every heartbeat)
            #[cfg(feature = "postgres")]
            let metrics_result = sqlx::query(
                "INSERT INTO agent_metrics (agent_id, cpu_pct, memory_pct, disk_used_pct) VALUES ($1, $2, $3, $4)"
            )
            .bind(agent_id)
            .bind(cpu)
            .bind(memory)
            .bind(disk)
            .execute(&state.db)
            .await;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let metrics_result = sqlx::query(
                "INSERT INTO agent_metrics (agent_id, cpu_pct, memory_pct, disk_used_pct) VALUES ($1, $2, $3, $4)"
            )
            .bind(DbUuid::from(agent_id))
            .bind(cpu)
            .bind(memory)
            .bind(disk)
            .execute(&state.db)
            .await;
            if let Err(e) = metrics_result
            {
                // Don't fail on metrics insert - just log warning
                tracing::warn!(agent_id = %agent_id, "Failed to insert agent metrics: {}", e);
            }
        }
        appcontrol_common::AgentMessage::Register {
            agent_id,
            hostname,
            ip_addresses,
            version,
            os_name,
            os_version,
            cpu_arch,
            cpu_cores,
            total_memory_mb,
            disk_total_gb,
            cert_fingerprint,
            ..
        } => {
            tracing::info!(
                agent_id = %agent_id,
                hostname = %hostname,
                ip_count = ip_addresses.len(),
                has_cert = cert_fingerprint.is_some(),
                os = ?os_name,
                "Agent registered via gateway"
            );
            // Check if agent is blocked before processing registration
            #[cfg(feature = "postgres")]
            let is_blocked: bool = sqlx::query_scalar(
                "SELECT NOT COALESCE(is_active, true) FROM agents WHERE id = $1",
            )
            .bind(agent_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or(false);
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let is_blocked: bool = {
                let val: Option<i32> = sqlx::query_scalar(
                    "SELECT CASE WHEN COALESCE(is_active, 1) = 0 THEN 1 ELSE 0 END FROM agents WHERE id = $1",
                )
                .bind(DbUuid::from(agent_id))
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
                val.unwrap_or(0) != 0
            };

            if is_blocked {
                tracing::warn!(
                    agent_id = %agent_id,
                    hostname = %hostname,
                    "REJECTED: registration from blocked agent"
                );
                // Don't process registration for blocked agents
                return;
            }

            // Update agent record with hostname, IPs, version, system info, and heartbeat
            // NOTE: We do NOT set is_active = true here to respect blocked status
            #[cfg(feature = "postgres")]
            if let Err(e) = sqlx::query(&format!(
                "UPDATE agents SET hostname = $2, ip_addresses = $3, last_heartbeat_at = {}, \
                 version = COALESCE($4, version), \
                 os_name = COALESCE($5, os_name), \
                 os_version = COALESCE($6, os_version), \
                 cpu_arch = COALESCE($7, cpu_arch), \
                 cpu_cores = COALESCE($8, cpu_cores), \
                 total_memory_mb = COALESCE($9, total_memory_mb), \
                 disk_total_gb = COALESCE($10, disk_total_gb), \
                 certificate_fingerprint = COALESCE($11, certificate_fingerprint), \
                 identity_verified = ($11 IS NOT NULL) \
                 WHERE id = $1 AND is_active = true",
                crate::db::sql::now()
            ))
            .bind(agent_id)
            .bind(&hostname)
            .bind(serde_json::json!(&ip_addresses))
            .bind(&version)
            .bind(&os_name)
            .bind(&os_version)
            .bind(&cpu_arch)
            .bind(cpu_cores.map(|c| c as i32))
            .bind(total_memory_mb.map(|m| m as i64))
            .bind(disk_total_gb.map(|d| d as i64))
            .bind(&cert_fingerprint)
            .execute(&state.db)
            .await
            {
                tracing::warn!(agent_id = %agent_id, "Failed to update agent registration: {}", e);
            }
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            if let Err(e) = sqlx::query(&format!(
                "UPDATE agents SET hostname = $2, ip_addresses = $3, last_heartbeat_at = {}, \
                 version = COALESCE($4, version), \
                 os_name = COALESCE($5, os_name), \
                 os_version = COALESCE($6, os_version), \
                 cpu_arch = COALESCE($7, cpu_arch), \
                 cpu_cores = COALESCE($8, cpu_cores), \
                 total_memory_mb = COALESCE($9, total_memory_mb), \
                 disk_total_gb = COALESCE($10, disk_total_gb), \
                 certificate_fingerprint = COALESCE($11, certificate_fingerprint), \
                 identity_verified = ($11 IS NOT NULL) \
                 WHERE id = $1 AND is_active = 1",
                crate::db::sql::now()
            ))
            .bind(DbUuid::from(agent_id))
            .bind(&hostname)
            .bind(serde_json::to_string(&ip_addresses).unwrap_or_default())
            .bind(&version)
            .bind(&os_name)
            .bind(&os_version)
            .bind(&cpu_arch)
            .bind(cpu_cores.map(|c| c as i32))
            .bind(total_memory_mb.map(|m| m as i64))
            .bind(disk_total_gb.map(|d| d as i64))
            .bind(&cert_fingerprint)
            .execute(&state.db)
            .await
            {
                tracing::warn!(agent_id = %agent_id, "Failed to update agent registration: {}", e);
            }

            // Resolve components that reference this host but have no agent_id yet
            // (late binding: user created component before agent was online)
            crate::api::components::resolve_components_for_agent(
                &state.db,
                agent_id,
                &hostname,
                &ip_addresses,
            )
            .await;

            // Send updated component config after resolution (may have bound new components)
            send_config_to_agent(state, agent_id).await;
        }
        appcontrol_common::AgentMessage::CertificateRenewal { agent_id, csr_pem } => {
            tracing::info!(
                agent_id = %agent_id,
                csr_len = csr_pem.len(),
                "Agent certificate renewal request received"
            );
            // TODO: Forward CSR to CA, get signed cert, send CertificateResponse back
        }
        appcontrol_common::AgentMessage::DiscoveryReport {
            agent_id,
            hostname,
            ref processes,
            ref listeners,
            ref connections,
            ref services,
            ref scheduled_jobs,
            ref firewall_rules,
            scanned_at,
        } => {
            tracing::info!(
                agent_id = %agent_id,
                hostname = %hostname,
                processes = processes.len(),
                listeners = listeners.len(),
                connections = connections.len(),
                services = services.len(),
                scheduled_jobs = scheduled_jobs.len(),
                firewall_rules = firewall_rules.len(),
                "Discovery report received"
            );
            // Store the full report as JSONB
            let report_json = serde_json::to_value(serde_json::json!({
                "processes": processes,
                "listeners": listeners,
                "connections": connections,
                "services": services,
                "scheduled_jobs": scheduled_jobs,
                "firewall_rules": firewall_rules,
            }))
            .unwrap_or_default();

            #[cfg(feature = "postgres")]
            let disc_result = sqlx::query(
                "INSERT INTO discovery_reports (agent_id, hostname, report, scanned_at)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(agent_id)
            .bind(&hostname)
            .bind(&report_json)
            .bind(scanned_at)
            .execute(&state.db)
            .await;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let disc_result = sqlx::query(
                "INSERT INTO discovery_reports (agent_id, hostname, report, scanned_at)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(DbUuid::from(agent_id))
            .bind(&hostname)
            .bind(serde_json::to_string(&report_json).unwrap_or_default())
            .bind(scanned_at.to_rfc3339())
            .execute(&state.db)
            .await;
            if let Err(e) = disc_result
            {
                tracing::warn!(
                    agent_id = %agent_id,
                    "Failed to store discovery report: {}", e
                );
            }
        }
        appcontrol_common::AgentMessage::UpdateProgress {
            update_id,
            agent_id,
            chunks_received,
            status,
            ref error,
        } => {
            let status_str = match status {
                appcontrol_common::UpdateStatus::Downloading => "in_progress",
                appcontrol_common::UpdateStatus::Verifying => "verifying",
                appcontrol_common::UpdateStatus::Applying => "applying",
                appcontrol_common::UpdateStatus::Complete => "complete",
                appcontrol_common::UpdateStatus::Failed => "failed",
            };
            tracing::info!(
                update_id = %update_id,
                agent_id = %agent_id,
                chunks = chunks_received,
                status = status_str,
                "Agent update progress"
            );
            #[cfg(feature = "postgres")]
            let _ = sqlx::query(&format!(
                "UPDATE agent_update_tasks \
                 SET status = $2, error = $3, \
                     completed_at = CASE WHEN $2 IN ('complete', 'failed') THEN {} ELSE completed_at END \
                 WHERE id = $1",
                crate::db::sql::now()
            ))
            .bind(update_id)
            .bind(status_str)
            .bind(error.as_deref())
            .execute(&state.db)
            .await;
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let _ = sqlx::query(&format!(
                "UPDATE agent_update_tasks \
                 SET status = $2, error = $3, \
                     completed_at = CASE WHEN $2 IN ('complete', 'failed') THEN {} ELSE completed_at END \
                 WHERE id = $1",
                crate::db::sql::now()
            ))
            .bind(DbUuid::from(update_id))
            .bind(status_str)
            .bind(error.as_deref())
            .execute(&state.db)
            .await;
        }
        appcontrol_common::AgentMessage::TerminalOutput { request_id, data } => {
            tracing::debug!(
                request_id = %request_id,
                data_len = data.len(),
                "Received TerminalOutput from agent"
            );
            // Look up session by request_id and forward to frontend connection
            if let Some(session_id) = state
                .terminal_sessions
                .get_session_id_by_request(request_id)
            {
                tracing::debug!(
                    request_id = %request_id,
                    session_id = %session_id,
                    "Found session, forwarding to frontend"
                );
                // Get conn_id and drop the read lock before calling touch_session
                // (to avoid deadlock: get_session holds read lock, touch_session needs write lock)
                let conn_id = state
                    .terminal_sessions
                    .get_session(session_id)
                    .map(|s| s.conn_id);

                if let Some(conn_id) = conn_id {
                    // Now we can safely touch the session (no read lock held)
                    state.terminal_sessions.touch_session(session_id);

                    // Base64 encode the binary data for JSON transmission
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

                    let output_event = appcontrol_common::WsEvent::TerminalOutput {
                        session_id,
                        data: encoded,
                    };

                    if let Ok(json) = serde_json::to_string(&output_event) {
                        state.ws_hub.send_to_connection(conn_id, json);
                    }
                }
            } else {
                // Session not found - might have been closed
                tracing::debug!(
                    request_id = %request_id,
                    "Terminal output received for unknown session"
                );
            }
        }
        appcontrol_common::AgentMessage::TerminalExit {
            request_id,
            exit_code,
        } => {
            // Look up and remove session
            if let Some(session_id) = state
                .terminal_sessions
                .get_session_id_by_request(request_id)
            {
                if let Some(session) = state.terminal_sessions.remove_session(session_id) {
                    tracing::info!(
                        session_id = %session_id,
                        exit_code = exit_code,
                        "Terminal session ended"
                    );

                    let exit_event = appcontrol_common::WsEvent::TerminalExit {
                        session_id,
                        exit_code,
                    };

                    if let Ok(json) = serde_json::to_string(&exit_event) {
                        state.ws_hub.send_to_connection(session.conn_id, json);
                    }
                }
            }
        }
        appcontrol_common::AgentMessage::LogEntries { agent_id, entries } => {
            // Route log entries to subscribed frontend connections
            let subscribers = state.log_subscriptions.get_agent_subscribers(agent_id);
            tracing::debug!(
                agent_id = %agent_id,
                entry_count = entries.len(),
                subscriber_count = subscribers.len(),
                "Processing agent log entries"
            );
            if subscribers.is_empty() {
                return; // No subscribers, skip processing
            }

            // Look up agent hostname for display
            #[cfg(feature = "postgres")]
            let agent_name: String =
                sqlx::query_scalar("SELECT hostname FROM agents WHERE id = $1")
                    .bind(agent_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| agent_id.to_string());
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            let agent_name: String =
                sqlx::query_scalar("SELECT hostname FROM agents WHERE id = $1")
                    .bind(DbUuid::from(agent_id))
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| agent_id.to_string());

            for entry in entries {
                // Create WsEvent for each log entry
                let log_event = appcontrol_common::WsEvent::LogEntry {
                    source_type: "agent".to_string(),
                    source_id: agent_id,
                    source_name: agent_name.clone(),
                    level: entry.level.clone(),
                    target: entry.target,
                    message: entry.message,
                    timestamp: entry.timestamp.to_rfc3339(),
                };

                if let Ok(json) = serde_json::to_string(&log_event) {
                    for conn_id in &subscribers {
                        // Check level filter for this connection
                        if state
                            .log_subscriptions
                            .level_passes_filter(*conn_id, &entry.level)
                        {
                            state.ws_hub.send_to_connection(*conn_id, json.clone());
                        }
                    }
                }
            }
        }
        // Log retrieval responses from agent - store in pending request for API response
        appcontrol_common::AgentMessage::ComponentLogs {
            request_id,
            component_id,
            source_type,
            source_name,
            entries,
            total_lines,
            truncated,
        } => {
            tracing::debug!(
                request_id = %request_id,
                component_id = %component_id,
                source_type = %source_type,
                entry_count = entries.len(),
                "Received ComponentLogs from agent"
            );
            // Store response for pending API request
            state.pending_log_requests.complete(
                request_id,
                Ok(serde_json::json!({
                    "source_type": source_type,
                    "source_name": source_name,
                    "entries": entries,
                    "total_lines": total_lines,
                    "truncated": truncated,
                })),
            );
        }
        appcontrol_common::AgentMessage::FileLogs {
            request_id,
            component_id,
            file_path,
            entries,
            total_lines,
            truncated,
            error,
        } => {
            tracing::debug!(
                request_id = %request_id,
                component_id = %component_id,
                file_path = %file_path,
                entry_count = entries.len(),
                has_error = error.is_some(),
                "Received FileLogs from agent"
            );
            if let Some(err) = error {
                state.pending_log_requests.complete(request_id, Err(err));
            } else {
                state.pending_log_requests.complete(
                    request_id,
                    Ok(serde_json::json!({
                        "source_type": "file",
                        "source_name": file_path,
                        "entries": entries,
                        "total_lines": total_lines,
                        "truncated": truncated,
                    })),
                );
            }
        }
        appcontrol_common::AgentMessage::EventLogs {
            request_id,
            component_id,
            log_name,
            entries,
            total_lines,
            truncated,
            error,
        } => {
            tracing::debug!(
                request_id = %request_id,
                component_id = %component_id,
                log_name = %log_name,
                entry_count = entries.len(),
                has_error = error.is_some(),
                "Received EventLogs from agent"
            );
            if let Some(err) = error {
                state.pending_log_requests.complete(request_id, Err(err));
            } else {
                state.pending_log_requests.complete(
                    request_id,
                    Ok(serde_json::json!({
                        "source_type": "event_log",
                        "source_name": log_name,
                        "entries": entries,
                        "total_lines": total_lines,
                        "truncated": truncated,
                    })),
                );
            }
        }
        appcontrol_common::AgentMessage::DiagnosticCommandResult {
            request_id,
            component_id,
            command_name,
            exit_code,
            stdout,
            stderr,
            duration_ms,
        } => {
            tracing::debug!(
                request_id = %request_id,
                component_id = %component_id,
                command_name = %command_name,
                exit_code = exit_code,
                "Received DiagnosticCommandResult from agent"
            );
            state.pending_log_requests.complete(
                request_id,
                Ok(serde_json::json!({
                    "command_name": command_name,
                    "exit_code": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "duration_ms": duration_ms,
                })),
            );
        }
    }
}

/// Query all components assigned to an agent and send them as an UpdateConfig message.
/// This is called when an agent connects (AgentConnected) so the agent's scheduler
/// knows what components to health-check. Without this, the agent has no work to do.
///
/// Components belonging to suspended applications are excluded from the config.
pub async fn send_config_to_agent(state: &Arc<AppState>, agent_id: uuid::Uuid) {
    #[cfg(feature = "postgres")]
    let rows = sqlx::query_as::<
        _,
        (
            uuid::Uuid, // id
            String,     // name
            Option<String>,
            Option<String>,
            Option<String>, // check, start, stop
            Option<String>,
            Option<String>,
            Option<String>, // integrity, post_start, infra
            Option<String>,
            Option<String>, // rebuild, rebuild_infra
            i32,
            i32,
            i32,               // intervals
            serde_json::Value, // env_vars
        ),
    >(
        "SELECT c.id, c.name, c.check_cmd, c.start_cmd, c.stop_cmd,
                c.integrity_check_cmd, c.post_start_check_cmd, c.infra_check_cmd,
                c.rebuild_cmd, c.rebuild_infra_cmd,
                c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                COALESCE(c.env_vars, '{}'::jsonb)
         FROM components c
         JOIN applications a ON c.application_id = a.id
         WHERE c.agent_id = $1
           AND a.is_suspended = false",
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rows = sqlx::query_as::<
        _,
        (
            DbUuid, // id
            String, // name
            Option<String>,
            Option<String>,
            Option<String>, // check, start, stop
            Option<String>,
            Option<String>,
            Option<String>, // integrity, post_start, infra
            Option<String>,
            Option<String>, // rebuild, rebuild_infra
            i32,
            i32,
            i32,    // intervals
            String, // env_vars as TEXT
        ),
    >(
        "SELECT c.id, c.name, c.check_cmd, c.start_cmd, c.stop_cmd,
                c.integrity_check_cmd, c.post_start_check_cmd, c.infra_check_cmd,
                c.rebuild_cmd, c.rebuild_infra_cmd,
                c.check_interval_seconds, c.start_timeout_seconds, c.stop_timeout_seconds,
                COALESCE(c.env_vars, '{}')
         FROM components c
         JOIN applications a ON c.application_id = a.id
         WHERE c.agent_id = $1
           AND a.is_suspended = 0",
    )
    .bind(DbUuid::from(agent_id))
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(agent_id = %agent_id, "Failed to query components for agent: {}", e);
            return;
        }
    };

    // Always send UpdateConfig, even if empty - this ensures agent's scheduler stays in sync
    let components: Vec<appcontrol_common::ComponentConfig> = rows
        .into_iter()
        .map(
            |(
                id,
                name,
                check,
                start,
                stop,
                integrity,
                post_start,
                infra,
                rebuild,
                rebuild_infra,
                interval,
                start_to,
                stop_to,
                env,
            )| {
                #[cfg(feature = "postgres")]
                let (comp_id, env_vars) = (id, env);
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                let (comp_id, env_vars) = (
                    id.into_inner(),
                    serde_json::from_str::<serde_json::Value>(&env)
                        .unwrap_or(serde_json::json!({})),
                );
                appcontrol_common::ComponentConfig {
                    component_id: comp_id,
                    name,
                    check_cmd: check,
                    start_cmd: start,
                    stop_cmd: stop,
                    integrity_check_cmd: integrity,
                    post_start_check_cmd: post_start,
                    infra_check_cmd: infra,
                    rebuild_cmd: rebuild,
                    rebuild_infra_cmd: rebuild_infra,
                    check_interval_seconds: interval as u32,
                    start_timeout_seconds: start_to as u32,
                    stop_timeout_seconds: stop_to as u32,
                    env_vars,
                }
            },
        )
        .collect();

    let count = components.len();
    let msg = appcontrol_common::BackendMessage::UpdateConfig { components };

    if state.ws_hub.send_to_agent(agent_id, msg) {
        tracing::info!(
            agent_id = %agent_id,
            component_count = count,
            "Sent UpdateConfig to agent"
        );
    } else {
        tracing::warn!(
            agent_id = %agent_id,
            "Failed to send UpdateConfig — agent not reachable via gateway"
        );
    }
}

/// Send RunChecksNow to an agent to trigger immediate health checks.
///
/// This is called when an agent reconnects to quickly restore component states
/// from UNREACHABLE to their actual state without waiting for the next scheduled
/// check interval.
pub fn send_run_checks_now(state: &Arc<AppState>, agent_id: uuid::Uuid) {
    let msg = appcontrol_common::BackendMessage::RunChecksNow {
        request_id: uuid::Uuid::new_v4(),
    };

    if state.ws_hub.send_to_agent(agent_id, msg) {
        tracing::info!(
            agent_id = %agent_id,
            "Sent RunChecksNow to agent"
        );
    } else {
        tracing::debug!(
            agent_id = %agent_id,
            "Could not send RunChecksNow — agent not reachable"
        );
    }
}

/// Mark all components belonging to an agent as UNREACHABLE.
///
/// This is called when an agent disconnects to provide immediate feedback to the UI.
/// Components in STOPPED or STOPPING state are skipped since they're intentionally stopped.
/// The previous state is stored in the state_transition details for recovery when the
/// agent reconnects.
async fn mark_agent_components_unreachable(
    state: &Arc<AppState>,
    agent_id: uuid::Uuid,
    trigger: &str,
) {
    // Row type for query
    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        id: DbUuid,
        name: String,
        current_state: String,
        application_id: DbUuid,
        app_name: String,
    }

    // Find components that should be transitioned to UNREACHABLE
    #[cfg(feature = "postgres")]
    let components_result = sqlx::query_as::<_, ComponentInfo>(
        r#"
        SELECT c.id, c.name, c.current_state, c.application_id, a.name AS app_name
        FROM components c
        JOIN applications a ON a.id = c.application_id
        WHERE c.agent_id = $1
          AND c.current_state NOT IN ('UNREACHABLE', 'STOPPED', 'STOPPING', 'UNKNOWN')
        "#,
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let components_result = sqlx::query_as::<_, ComponentInfo>(
        r#"
        SELECT c.id, c.name, c.current_state, c.application_id, a.name AS app_name
        FROM components c
        JOIN applications a ON a.id = c.application_id
        WHERE c.agent_id = $1
          AND c.current_state NOT IN ('UNREACHABLE', 'STOPPED', 'STOPPING', 'UNKNOWN')
        "#,
    )
    .bind(DbUuid::from(agent_id))
    .fetch_all(&state.db)
    .await;
    let components = match components_result
    {
        Ok(comps) => comps,
        Err(e) => {
            tracing::error!(
                agent_id = %agent_id,
                "Failed to query components for UNREACHABLE transition: {}", e
            );
            return;
        }
    };

    if components.is_empty() {
        tracing::debug!(
            agent_id = %agent_id,
            "No components need UNREACHABLE transition"
        );
        return;
    }

    tracing::info!(
        agent_id = %agent_id,
        count = components.len(),
        "Transitioning components to UNREACHABLE due to agent disconnect"
    );

    for comp in components {
        // Insert state transition (append-only)
        #[cfg(feature = "postgres")]
        let trans_result = sqlx::query(
            r#"
            INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
            VALUES ($1, $2, 'UNREACHABLE', $3,
                    jsonb_build_object('previous_state', $2, 'agent_id', $4::text))
            "#,
        )
        .bind(comp.id)
        .bind(&comp.current_state)
        .bind(trigger)
        .bind(agent_id.to_string())
        .execute(&state.db)
        .await;
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let trans_result = {
            let details = serde_json::json!({
                "previous_state": &comp.current_state,
                "agent_id": agent_id.to_string(),
            });
            sqlx::query(
                r#"
                INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
                VALUES ($1, $2, 'UNREACHABLE', $3, $4)
                "#,
            )
            .bind(comp.id)
            .bind(&comp.current_state)
            .bind(trigger)
            .bind(serde_json::to_string(&details).unwrap_or_default())
            .execute(&state.db)
            .await
        };
        if let Err(e) = trans_result
        {
            tracing::warn!(
                component_id = %comp.id,
                "Failed to insert state_transition to UNREACHABLE: {}", e
            );
            continue;
        }

        // Update cached current_state on the component
        if let Err(e) =
            sqlx::query("UPDATE components SET current_state = 'UNREACHABLE' WHERE id = $1")
                .bind(comp.id)
                .execute(&state.db)
                .await
        {
            tracing::warn!(
                component_id = %comp.id,
                "Failed to update component current_state: {}", e
            );
            continue;
        }

        // Parse current_state as ComponentState enum
        let from_state = match comp.current_state.as_str() {
            "RUNNING" => appcontrol_common::ComponentState::Running,
            "STOPPED" => appcontrol_common::ComponentState::Stopped,
            "STARTING" => appcontrol_common::ComponentState::Starting,
            "STOPPING" => appcontrol_common::ComponentState::Stopping,
            "FAILED" => appcontrol_common::ComponentState::Failed,
            "DEGRADED" => appcontrol_common::ComponentState::Degraded,
            "UNREACHABLE" => appcontrol_common::ComponentState::Unreachable,
            _ => appcontrol_common::ComponentState::Unknown,
        };

        // Broadcast WebSocket event
        state.ws_hub.broadcast(
            comp.application_id,
            appcontrol_common::WsEvent::StateChange {
                component_id: *comp.id,
                app_id: *comp.application_id,
                component_name: Some(comp.name.clone()),
                app_name: Some(comp.app_name.clone()),
                from: from_state,
                to: appcontrol_common::ComponentState::Unreachable,
                at: chrono::Utc::now(),
            },
        );

        tracing::debug!(
            component_id = %comp.id,
            from = %comp.current_state,
            "Component transitioned to UNREACHABLE"
        );
    }
}

/// Push updated config to all agents affected by changes to an application or components.
///
/// Call this after:
/// - Importing a new application (pass application_id)
/// - Suspending/resuming an application (pass application_id)
/// - Creating/updating/deleting components (pass component_ids)
/// - Changing a component's agent_id (pass both old and new agent_ids)
///
/// This ensures agents receive real-time config updates without requiring reconnection.
pub async fn push_config_to_affected_agents(
    state: &Arc<AppState>,
    application_id: Option<uuid::Uuid>,
    component_ids: Option<&[uuid::Uuid]>,
    additional_agent_ids: Option<&[uuid::Uuid]>,
) {
    let mut agent_ids: Vec<uuid::Uuid> = Vec::new();

    // Find agents affected by application
    if let Some(app_id) = application_id {
        #[cfg(feature = "postgres")]
        let app_agents: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM components
             WHERE application_id = $1 AND agent_id IS NOT NULL",
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let app_agents: Vec<uuid::Uuid> = {
            let rows: Vec<DbUuid> = sqlx::query_scalar(
                "SELECT DISTINCT agent_id FROM components
                 WHERE application_id = $1 AND agent_id IS NOT NULL",
            )
            .bind(DbUuid::from(app_id))
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
            rows.into_iter().map(|u| u.into_inner()).collect()
        };
        agent_ids.extend(app_agents);
    }

    // Find agents affected by specific components
    if let Some(comp_ids) = component_ids {
        if !comp_ids.is_empty() {
            let comp_agents = fetch_agents_for_components(&state.db, comp_ids)
                .await
                .unwrap_or_default();
            agent_ids.extend(comp_agents);
        }
    }

    // Add any additional agents (e.g., old agent when component moves to new agent)
    if let Some(extra) = additional_agent_ids {
        agent_ids.extend(extra.iter().copied());
    }

    // Deduplicate
    agent_ids.sort();
    agent_ids.dedup();

    if agent_ids.is_empty() {
        tracing::debug!("No agents to notify for config push");
        return;
    }

    tracing::info!(
        agent_count = agent_ids.len(),
        "Pushing config updates to affected agents"
    );

    // Send UpdateConfig to each connected agent
    for agent_id in agent_ids {
        if state.ws_hub.is_agent_connected(agent_id) {
            send_config_to_agent(state, agent_id).await;
        } else {
            tracing::debug!(
                agent_id = %agent_id,
                "Skipping config push — agent not connected"
            );
        }
    }
}

/// Validate a gateway enrollment token and return the organization ID.
///
/// This is used when a gateway connects via WebSocket and presents an enrollment token.
/// The token must:
/// - Be a valid enrollment token (hash matches stored hash)
/// - Have scope = "gateway"
/// - Not be expired
/// - Not be revoked
/// - Not exceed max_uses (if set)
///
/// Returns Ok(org_id) if valid, Err(reason) if invalid.
async fn validate_gateway_enrollment_token(
    db: &crate::db::DbPool,
    token: &str,
) -> Result<uuid::Uuid, &'static str> {
    use sha2::Digest;

    if !token.starts_with("ac_enroll_") {
        return Err("Invalid token format");
    }

    // Hash the token and look it up
    let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));

    #[cfg(feature = "postgres")]
    let token_row = sqlx::query_as::<
        _,
        (
            uuid::Uuid,                    // id
            uuid::Uuid,                    // organization_id
            String,                        // scope
            Option<i32>,                   // max_uses
            i32,                           // current_uses
            chrono::DateTime<chrono::Utc>, // expires_at
        ),
    >(
        r#"SELECT id, organization_id, scope, max_uses, current_uses, expires_at
           FROM enrollment_tokens
           WHERE token_hash = $1
           AND revoked_at IS NULL"#,
    )
    .bind(&token_hash)
    .fetch_optional(db)
    .await
    .map_err(|_| "Database error")?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let token_row = sqlx::query_as::<
        _,
        (
            DbUuid,       // id
            DbUuid,       // organization_id
            String,       // scope
            Option<i32>,  // max_uses
            i32,          // current_uses
            String,       // expires_at as TEXT
        ),
    >(
        r#"SELECT id, organization_id, scope, max_uses, current_uses, expires_at
           FROM enrollment_tokens
           WHERE token_hash = $1
           AND revoked_at IS NULL"#,
    )
    .bind(&token_hash)
    .fetch_optional(db)
    .await
    .map_err(|_| "Database error")?;

    #[cfg(feature = "postgres")]
    let parsed_row = token_row;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let parsed_row = token_row.map(|(id, org, scope, max, cur, exp_str)| {
        let exp = chrono::DateTime::parse_from_rfc3339(&exp_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(&exp_str, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| ndt.and_utc())
            })
            .unwrap_or_else(|_| chrono::Utc::now());
        (id.into_inner(), org.into_inner(), scope, max, cur, exp)
    });

    let (token_id, org_id, scope, max_uses, current_uses, expires_at) = match parsed_row {
        Some(row) => row,
        None => return Err("Invalid or revoked token"),
    };

    // Check scope
    if scope != "gateway" {
        tracing::warn!(
            token_id = %token_id,
            scope = %scope,
            "Gateway attempted to use non-gateway enrollment token"
        );
        return Err("Token scope must be 'gateway'");
    }

    // Check expiry
    if chrono::Utc::now() > expires_at {
        return Err("Token has expired");
    }

    // Check usage limit
    if let Some(max) = max_uses {
        if current_uses >= max {
            return Err("Token has reached max uses");
        }
    }

    // Increment usage count
    let _ =
        sqlx::query("UPDATE enrollment_tokens SET current_uses = current_uses + 1 WHERE id = $1")
            .bind(DbUuid::from(token_id))
            .execute(db)
            .await;

    tracing::info!(
        token_id = %token_id,
        org_id = %org_id,
        "Gateway enrollment token validated"
    );

    Ok(org_id)
}

// Helper for cross-database component agent lookup
#[cfg(feature = "postgres")]
async fn fetch_agents_for_components(
    db: &crate::db::DbPool,
    comp_ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT agent_id FROM components WHERE id = ANY($1) AND agent_id IS NOT NULL",
    )
    .bind(comp_ids)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_agents_for_components(
    db: &crate::db::DbPool,
    comp_ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
    if comp_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=comp_ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        "SELECT DISTINCT agent_id FROM components WHERE id IN ({}) AND agent_id IS NOT NULL",
        placeholders.join(", ")
    );
    let mut q = sqlx::query_scalar::<_, String>(&query);
    for id in comp_ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<String> = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|s| uuid::Uuid::parse_str(&s).ok())
        .collect())
}
