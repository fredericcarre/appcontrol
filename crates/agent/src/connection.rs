use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

use appcontrol_common::{AgentMessage, BackendMessage};
use crate::buffer::OfflineBuffer;

/// Manages the WebSocket connection to the gateway/backend.
pub struct ConnectionManager {
    gateway_url: String,
    agent_id: Uuid,
    labels: HashMap<String, String>,
    buffer: OfflineBuffer,
}

impl ConnectionManager {
    pub fn new(
        gateway_url: String,
        agent_id: Uuid,
        labels: HashMap<String, String>,
        buffer: OfflineBuffer,
    ) -> Self {
        Self {
            gateway_url,
            agent_id,
            labels,
            buffer,
        }
    }

    pub async fn run(self, mut msg_rx: mpsc::UnboundedReceiver<AgentMessage>) {
        let mut backoff_secs = 1u64;
        let max_backoff = 60u64;

        loop {
            tracing::info!("Connecting to gateway: {}", self.gateway_url);

            match self.connect_and_run(&mut msg_rx).await {
                Ok(()) => {
                    tracing::info!("Connection closed gracefully");
                    backoff_secs = 1;
                }
                Err(e) => {
                    tracing::error!("Connection error: {}. Reconnecting in {}s", e, backoff_secs);
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(max_backoff);
                }
            }
        }
    }

    async fn connect_and_run(
        &self,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::connect_async;

        let (ws_stream, _) = connect_async(&self.gateway_url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Send registration message
        let register = AgentMessage::Register {
            agent_id: self.agent_id,
            hostname: gethostname(),
            labels: serde_json::json!(self.labels),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        let msg = serde_json::to_string(&register)?;
        write.send(tokio_tungstenite::tungstenite::Message::Text(msg.into())).await?;

        // Replay buffered messages
        let buffered = self.buffer.drain()?;
        for msg in buffered {
            let json = serde_json::to_string(&msg)?;
            write.send(tokio_tungstenite::tungstenite::Message::Text(json.into())).await?;
        }

        tracing::info!("Connected and registered as agent {}", self.agent_id);

        loop {
            tokio::select! {
                // Messages from scheduler to send
                Some(agent_msg) = msg_rx.recv() => {
                    let json = serde_json::to_string(&agent_msg)?;
                    write.send(tokio_tungstenite::tungstenite::Message::Text(json.into())).await?;
                }
                // Messages from backend
                Some(ws_msg) = read.next() => {
                    match ws_msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Ok(backend_msg) = serde_json::from_str::<BackendMessage>(&text) {
                                self.handle_backend_message(backend_msg).await;
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                        _ => {}
                    }
                }
                else => return Ok(()),
            }
        }
    }

    async fn handle_backend_message(&self, msg: BackendMessage) {
        match msg {
            BackendMessage::ExecuteCommand { request_id, component_id, command, timeout_seconds } => {
                tracing::info!("Executing command for component {}: {}", component_id, command);

                let timeout = std::time::Duration::from_secs(timeout_seconds as u64);
                match crate::executor::execute_sync(&command, timeout).await {
                    Ok(result) => {
                        tracing::info!("Command {} completed with exit code {}", request_id, result.exit_code);
                    }
                    Err(e) => {
                        tracing::error!("Command {} failed: {}", request_id, e);
                    }
                }
            }
            BackendMessage::UpdateConfig { components } => {
                tracing::info!("Received config update for {} components", components.len());
                // Update check scheduler with new component configs
            }
            BackendMessage::Ack { request_id } => {
                tracing::debug!("Received ack for request {}", request_id);
            }
        }
    }
}

fn gethostname() -> String {
    let mut buf = [0u8; 256];
    let result = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if result != 0 {
        return "unknown".to_string();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).to_string()
}
