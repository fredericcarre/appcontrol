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

    ws.on_upgrade(move |socket| handle_client_socket(socket, state, user_id))
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

    // Process frontend subscription messages
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                if let Ok(client_msg) =
                    serde_json::from_str::<appcontrol_common::WsClientMessage>(&text)
                {
                    match client_msg {
                        appcontrol_common::WsClientMessage::Subscribe { app_id } => {
                            state_clone.ws_hub.subscribe(conn_id, app_id);
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

/// Handle the gateway WebSocket connection.
/// Receives agent messages (heartbeats, check results, command results) and
/// forwards backend commands to agents.
async fn handle_gateway_socket(socket: ws::WebSocket, state: Arc<AppState>) {
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();

    // Register the gateway sender so the Hub can push commands to agents
    let (gw_tx, mut gw_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.ws_hub.set_gateway_sender(gw_tx);
    tracing::info!("Gateway connected to backend");

    // Forward commands from hub to gateway
    let send_task = tokio::spawn(async move {
        while let Some(msg) = gw_rx.recv().await {
            if sender.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process agent messages forwarded by gateway
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                if let Ok(agent_msg) =
                    serde_json::from_str::<appcontrol_common::AgentMessage>(&text)
                {
                    process_agent_message(&state_clone, agent_msg).await;
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.ws_hub.clear_gateway_sender();
    tracing::info!("Gateway disconnected from backend");
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
            if let Err(e) = crate::core::fsm::process_check_result(
                state,
                cr.component_id,
                cr.exit_code,
            )
            .await
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
            ..
        } => {
            tracing::info!(
                request_id = %request_id,
                exit_code = exit_code,
                "Command result received"
            );
            // Command results are informational — the FSM transition happens
            // when the next health check detects the new state.
            // We still broadcast to frontend for real-time feedback.
            if !stdout.is_empty() || !stderr.is_empty() {
                tracing::debug!(stdout = %stdout, stderr = %stderr, "Command output");
            }
        }
        appcontrol_common::AgentMessage::Heartbeat { agent_id, .. } => {
            tracing::trace!(agent_id = %agent_id, "Agent heartbeat");
        }
        appcontrol_common::AgentMessage::Register {
            agent_id, hostname, ..
        } => {
            tracing::info!(
                agent_id = %agent_id,
                hostname = %hostname,
                "Agent registered via gateway"
            );
        }
    }
}
