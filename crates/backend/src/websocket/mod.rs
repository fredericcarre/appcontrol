pub mod hub;
pub use hub::Hub;

use axum::{
    extract::{ws, Query, State},
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::auth::jwt;
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
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                match serde_json::from_str::<appcontrol_common::GatewayMessage>(&text) {
                    Ok(gw_msg) => {
                        process_gateway_message(&state_clone, &gw_id_clone, &gw_tx_clone, gw_msg)
                            .await;
                    }
                    Err(_) => {
                        // Backwards compatibility: try parsing as raw AgentMessage
                        if let Ok(agent_msg) =
                            serde_json::from_str::<appcontrol_common::AgentMessage>(&text)
                        {
                            process_agent_message(&state_clone, agent_msg).await;
                        } else {
                            tracing::warn!("Unknown message from gateway");
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
            zone,
            version,
        } => {
            tracing::info!(
                gateway_id = %gateway_id,
                zone = %zone,
                version = %version,
                "Gateway registered"
            );
            // Store the gateway_id for this connection
            *gateway_id_cell.lock().unwrap() = Some(gateway_id);
            // Register in the hub with the sender channel
            state
                .ws_hub
                .register_gateway(gateway_id, zone, gw_tx.clone());
        }
        appcontrol_common::GatewayMessage::AgentMessage(agent_msg) => {
            process_agent_message(state, agent_msg).await;
        }
        appcontrol_common::GatewayMessage::AgentConnected {
            agent_id,
            hostname,
            cert_fingerprint,
            cert_cn,
        } => {
            // Copy the value out and drop the MutexGuard before any .await
            let gw_id = { *gateway_id_cell.lock().unwrap() };
            if let Some(gw_id) = gw_id {
                tracing::info!(
                    agent_id = %agent_id,
                    hostname = %hostname,
                    gateway_id = %gw_id,
                    identity_verified = cert_fingerprint.is_some(),
                    "Agent connected via gateway"
                );
                state.ws_hub.register_agent_route(agent_id, gw_id);

                // Store certificate fingerprint for identity verification
                if cert_fingerprint.is_some() || cert_cn.is_some() {
                    if let Err(e) = sqlx::query(
                        "UPDATE agents SET certificate_fingerprint = COALESCE($2, certificate_fingerprint), \
                         certificate_cn = COALESCE($3, certificate_cn), \
                         identity_verified = ($2 IS NOT NULL) \
                         WHERE id = $1",
                    )
                    .bind(agent_id)
                    .bind(&cert_fingerprint)
                    .bind(&cert_cn)
                    .execute(&state.db)
                    .await
                    {
                        tracing::warn!(
                            agent_id = %agent_id,
                            "Failed to store cert fingerprint: {}", e
                        );
                    }
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
        }
    }
}

/// Process an incoming agent message: update FSM, record events, broadcast.
async fn process_agent_message(state: &Arc<AppState>, msg: appcontrol_common::AgentMessage) {
    match msg {
        appcontrol_common::AgentMessage::CheckResult(cr) => {
            tracing::debug!(
                component_id = %cr.component_id,
                exit_code = cr.exit_code,
                "Processing check result"
            );
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
            // Send Ack back to agent if sequence_id was provided
            if let Some(seq) = sequence_id {
                let ack = appcontrol_common::BackendMessage::Ack {
                    request_id,
                    sequence_id: Some(seq),
                };
                // Find the agent's gateway and send the ack
                // (The CommandResult doesn't contain agent_id directly,
                //  so we broadcast to the agent that sent the original request)
                tracing::debug!(
                    request_id = %request_id,
                    sequence_id = seq,
                    "Sending Ack for command result"
                );
                // Note: In a full implementation, we'd look up the agent_id from request_id
                // and send the ack through the correct gateway. For now, we log it.
                let _ = ack;
            }
        }
        appcontrol_common::AgentMessage::Heartbeat { agent_id, .. } => {
            tracing::trace!(agent_id = %agent_id, "Agent heartbeat");
            // Update last_heartbeat_at in database
            if let Err(e) = sqlx::query("UPDATE agents SET last_heartbeat_at = now() WHERE id = $1")
                .bind(agent_id)
                .execute(&state.db)
                .await
            {
                tracing::warn!(agent_id = %agent_id, "Failed to update heartbeat: {}", e);
            }
        }
        appcontrol_common::AgentMessage::Register {
            agent_id,
            hostname,
            ip_addresses,
            cert_fingerprint,
            ..
        } => {
            tracing::info!(
                agent_id = %agent_id,
                hostname = %hostname,
                ip_count = ip_addresses.len(),
                has_cert = cert_fingerprint.is_some(),
                "Agent registered via gateway"
            );
            // Update agent record with hostname, IPs, version, and heartbeat
            if let Err(e) = sqlx::query(
                "UPDATE agents SET hostname = $2, ip_addresses = $3, last_heartbeat_at = now(), \
                 is_active = true, \
                 certificate_fingerprint = COALESCE($4, certificate_fingerprint), \
                 identity_verified = ($4 IS NOT NULL) \
                 WHERE id = $1",
            )
            .bind(agent_id)
            .bind(&hostname)
            .bind(serde_json::json!(&ip_addresses))
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
        }
        appcontrol_common::AgentMessage::CertificateRenewal { agent_id, csr_pem } => {
            tracing::info!(
                agent_id = %agent_id,
                csr_len = csr_pem.len(),
                "Agent certificate renewal request received"
            );
            // TODO: Forward CSR to CA, get signed cert, send CertificateResponse back
        }
    }
}
