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

    ws.on_upgrade(move |socket| handle_socket(socket, state, user_id))
        .into_response()
}

async fn handle_socket(socket: ws::WebSocket, state: Arc<AppState>, user_id: uuid::Uuid) {
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

    // Process messages from client (frontend subscriptions) and agent messages (forwarded by gateway)
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                // Try frontend client message first
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
                // Try agent message (forwarded by gateway)
                else if let Ok(agent_msg) =
                    serde_json::from_str::<appcontrol_common::AgentMessage>(&text)
                {
                    match agent_msg {
                        appcontrol_common::AgentMessage::CheckResult(cr) => {
                            if let Err(e) = crate::core::fsm::process_check_result(
                                &state_clone,
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
                        appcontrol_common::AgentMessage::Heartbeat { agent_id, .. } => {
                            tracing::trace!(agent_id = %agent_id, "Agent heartbeat received");
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.ws_hub.remove_connection(conn_id);
}
