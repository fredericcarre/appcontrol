use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::buffer::OfflineBuffer;
use crate::scheduler::CheckScheduler;
use appcontrol_common::{AgentMessage, BackendMessage};

/// Manages the WebSocket connection to the gateway/backend.
pub struct ConnectionManager {
    gateway_url: String,
    agent_id: Uuid,
    labels: HashMap<String, String>,
    buffer: OfflineBuffer,
    scheduler: Arc<CheckScheduler>,
    msg_tx: mpsc::UnboundedSender<AgentMessage>,
}

impl ConnectionManager {
    pub fn new(
        gateway_url: String,
        agent_id: Uuid,
        labels: HashMap<String, String>,
        buffer: OfflineBuffer,
        scheduler: Arc<CheckScheduler>,
        msg_tx: mpsc::UnboundedSender<AgentMessage>,
    ) -> Self {
        Self {
            gateway_url,
            agent_id,
            labels,
            buffer,
            scheduler,
            msg_tx,
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

        // Send registration message with hostname and detected IPs
        let register = AgentMessage::Register {
            agent_id: self.agent_id,
            hostname: crate::platform::gethostname(),
            ip_addresses: crate::platform::get_ip_addresses(),
            labels: serde_json::json!(self.labels),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        let msg = serde_json::to_string(&register)?;
        write
            .send(tokio_tungstenite::tungstenite::Message::Text(msg))
            .await?;

        // Replay buffered messages
        let buffered = self.buffer.drain()?;
        for msg in buffered {
            let json = serde_json::to_string(&msg)?;
            write
                .send(tokio_tungstenite::tungstenite::Message::Text(json))
                .await?;
        }

        tracing::info!("Connected and registered as agent {}", self.agent_id);

        loop {
            tokio::select! {
                // Messages from scheduler to send
                Some(agent_msg) = msg_rx.recv() => {
                    let json = serde_json::to_string(&agent_msg)?;
                    write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await?;
                }
                // Messages from backend
                Some(ws_msg) = read.next() => {
                    match ws_msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Ok(backend_msg) = serde_json::from_str::<BackendMessage>(&text) {
                                self.handle_backend_message(backend_msg);
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

    /// Handle a message from the backend. Commands are spawned in separate tasks
    /// to avoid blocking the WebSocket loop.
    fn handle_backend_message(&self, msg: BackendMessage) {
        match msg {
            BackendMessage::ExecuteCommand {
                request_id,
                component_id,
                command,
                timeout_seconds,
            } => {
                tracing::info!(
                    request_id = %request_id,
                    component_id = %component_id,
                    "Executing command: {}",
                    command
                );

                let msg_tx = self.msg_tx.clone();
                let timeout = std::time::Duration::from_secs(timeout_seconds as u64);

                // Spawn execution in a separate task — never block the WS loop
                tokio::spawn(async move {
                    let start = std::time::Instant::now();
                    match crate::executor::execute_sync(&command, timeout).await {
                        Ok(result) => {
                            tracing::info!(
                                request_id = %request_id,
                                exit_code = result.exit_code,
                                duration_ms = result.duration_ms,
                                "Command completed"
                            );
                            // Send result back to backend
                            let _ = msg_tx.send(AgentMessage::CommandResult {
                                request_id,
                                exit_code: result.exit_code,
                                stdout: result.stdout,
                                stderr: result.stderr,
                                duration_ms: result.duration_ms,
                            });
                        }
                        Err(e) => {
                            let duration_ms = start.elapsed().as_millis() as u32;
                            tracing::error!(request_id = %request_id, "Command failed: {}", e);
                            let _ = msg_tx.send(AgentMessage::CommandResult {
                                request_id,
                                exit_code: -1,
                                stdout: String::new(),
                                stderr: format!("Agent execution error: {}", e),
                                duration_ms,
                            });
                        }
                    }
                });
            }
            BackendMessage::UpdateConfig { components } => {
                tracing::info!("Received config update for {} components", components.len());
                let scheduler = self.scheduler.clone();
                tokio::spawn(async move {
                    scheduler.update_components(components).await;
                });
            }
            BackendMessage::Ack { request_id } => {
                tracing::debug!("Received ack for request {}", request_id);
            }
        }
    }
}
