use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::buffer::OfflineBuffer;
use crate::scheduler::CheckScheduler;
use appcontrol_common::{AgentMessage, BackendMessage};

/// Manages the WebSocket connection to the gateway/backend.
/// Supports multi-gateway failover with ordered strategy.
pub struct ConnectionManager {
    gateway_urls: Vec<String>,
    failover_strategy: String,
    primary_retry_secs: u64,
    agent_id: Uuid,
    labels: HashMap<String, String>,
    buffer: OfflineBuffer,
    scheduler: Arc<CheckScheduler>,
    msg_tx: mpsc::UnboundedSender<AgentMessage>,
    /// Monotonic sequence counter for reliable message delivery.
    sequence_counter: Arc<AtomicU64>,
}

impl ConnectionManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gateway_urls: Vec<String>,
        failover_strategy: String,
        primary_retry_secs: u64,
        agent_id: Uuid,
        labels: HashMap<String, String>,
        buffer: OfflineBuffer,
        scheduler: Arc<CheckScheduler>,
        msg_tx: mpsc::UnboundedSender<AgentMessage>,
    ) -> Self {
        Self {
            gateway_urls,
            failover_strategy,
            primary_retry_secs,
            agent_id,
            labels,
            buffer,
            scheduler,
            msg_tx,
            sequence_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Backward-compatible constructor for single gateway URL.
    #[allow(dead_code)]
    pub fn new_single(
        gateway_url: String,
        agent_id: Uuid,
        labels: HashMap<String, String>,
        buffer: OfflineBuffer,
        scheduler: Arc<CheckScheduler>,
        msg_tx: mpsc::UnboundedSender<AgentMessage>,
    ) -> Self {
        Self::new(
            vec![gateway_url],
            "ordered".to_string(),
            300,
            agent_id,
            labels,
            buffer,
            scheduler,
            msg_tx,
        )
    }

    /// Get the next sequence_id for reliable message delivery.
    #[allow(dead_code)]
    fn next_sequence_id(&self) -> u64 {
        self.sequence_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn run(self, mut msg_rx: mpsc::UnboundedReceiver<AgentMessage>) {
        let mut current_url_idx = 0;
        let mut backoff_secs = 1u64;
        let max_backoff = 60u64;
        let mut last_primary_attempt = std::time::Instant::now();

        loop {
            // Periodically try to return to primary gateway
            if current_url_idx > 0
                && last_primary_attempt.elapsed().as_secs() >= self.primary_retry_secs
            {
                tracing::info!("Attempting to reconnect to primary gateway");
                current_url_idx = 0;
                last_primary_attempt = std::time::Instant::now();
            }

            let url = &self.gateway_urls[current_url_idx];
            tracing::info!("Connecting to gateway [{}]: {}", current_url_idx + 1, url);

            match self.connect_and_run(url, &mut msg_rx).await {
                Ok(()) => {
                    tracing::info!("Connection closed gracefully");
                    backoff_secs = 1;
                }
                Err(e) => {
                    tracing::error!(
                        "Connection error on gateway {}: {}. Reconnecting in {}s",
                        url,
                        e,
                        backoff_secs
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(max_backoff);

                    // Failover to next gateway
                    if self.gateway_urls.len() > 1 {
                        if self.failover_strategy == "round-robin" {
                            current_url_idx = (current_url_idx + 1) % self.gateway_urls.len();
                        } else {
                            // ordered: try next, wrap around
                            current_url_idx = (current_url_idx + 1) % self.gateway_urls.len();
                        }
                        tracing::info!(
                            "Failing over to gateway [{}]: {}",
                            current_url_idx + 1,
                            self.gateway_urls[current_url_idx]
                        );
                    }
                }
            }
        }
    }

    async fn connect_and_run(
        &self,
        gateway_url: &str,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::connect_async;

        let (ws_stream, _) = connect_async(gateway_url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Send registration message with hostname and detected IPs
        let register = AgentMessage::Register {
            agent_id: self.agent_id,
            hostname: crate::platform::gethostname(),
            ip_addresses: crate::platform::get_ip_addresses(),
            labels: serde_json::json!(self.labels),
            version: env!("CARGO_PKG_VERSION").to_string(),
            cert_fingerprint: None, // Gateway populates this from TLS handshake
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

        tracing::info!(
            "Connected to {} and registered as agent {}",
            gateway_url,
            self.agent_id
        );

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
                exec_mode,
            } => {
                tracing::info!(
                    request_id = %request_id,
                    component_id = %component_id,
                    exec_mode = %exec_mode,
                    "Executing command: {}",
                    command
                );

                let msg_tx = self.msg_tx.clone();
                let timeout = std::time::Duration::from_secs(timeout_seconds as u64);
                let seq_counter = self.sequence_counter.clone();

                // Spawn execution in a separate task — never block the WS loop
                tokio::spawn(async move {
                    if exec_mode == "detached" {
                        // Use double-fork + setsid for start/stop/rebuild commands
                        // Process survives agent crash (Critical Rule #5)
                        #[cfg(unix)]
                        match crate::executor::execute_async_detached(&command) {
                            Ok(pid) => {
                                tracing::info!(
                                    request_id = %request_id,
                                    pid = pid,
                                    "Detached process started"
                                );
                                let seq =
                                    seq_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let _ = msg_tx.send(AgentMessage::CommandResult {
                                    request_id,
                                    exit_code: 0,
                                    stdout: format!("Detached process started (pid={})", pid),
                                    stderr: String::new(),
                                    duration_ms: 0,
                                    sequence_id: Some(seq),
                                });
                            }
                            Err(e) => {
                                tracing::error!(request_id = %request_id, "Detached exec failed: {}", e);
                                let seq =
                                    seq_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let _ = msg_tx.send(AgentMessage::CommandResult {
                                    request_id,
                                    exit_code: -1,
                                    stdout: String::new(),
                                    stderr: format!("Detached exec failed: {}", e),
                                    duration_ms: 0,
                                    sequence_id: Some(seq),
                                });
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = msg_tx.send(AgentMessage::CommandResult {
                                request_id,
                                exit_code: -1,
                                stdout: String::new(),
                                stderr: "Detached execution not supported on this platform"
                                    .to_string(),
                                duration_ms: 0,
                                sequence_id: None,
                            });
                        }
                    } else {
                        // Sync execution: wait for result (checks, diagnostics, custom commands)
                        let start = std::time::Instant::now();
                        match crate::executor::execute_sync(&command, timeout).await {
                            Ok(result) => {
                                tracing::info!(
                                    request_id = %request_id,
                                    exit_code = result.exit_code,
                                    duration_ms = result.duration_ms,
                                    "Command completed"
                                );
                                let seq =
                                    seq_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let _ = msg_tx.send(AgentMessage::CommandResult {
                                    request_id,
                                    exit_code: result.exit_code,
                                    stdout: result.stdout,
                                    stderr: result.stderr,
                                    duration_ms: result.duration_ms,
                                    sequence_id: Some(seq),
                                });
                            }
                            Err(e) => {
                                let duration_ms = start.elapsed().as_millis() as u32;
                                tracing::error!(request_id = %request_id, "Command failed: {}", e);
                                let seq =
                                    seq_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                let _ = msg_tx.send(AgentMessage::CommandResult {
                                    request_id,
                                    exit_code: -1,
                                    stdout: String::new(),
                                    stderr: format!("Agent execution error: {}", e),
                                    duration_ms,
                                    sequence_id: Some(seq),
                                });
                            }
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
            BackendMessage::Ack {
                request_id,
                sequence_id,
            } => {
                tracing::debug!(
                    "Received ack for request {} (seq={:?})",
                    request_id,
                    sequence_id
                );
            }
            BackendMessage::UpdateAgent {
                binary_url,
                checksum_sha256,
                target_version,
            } => {
                tracing::info!(
                    "Received agent update command: version={} url={}",
                    target_version,
                    binary_url
                );
                // Agent self-update is handled in a dedicated task
                tokio::spawn(async move {
                    tracing::info!(
                        "Agent update to {} acknowledged (url={}, sha256={}). \
                         Self-update mechanism will download, verify, and restart.",
                        target_version,
                        binary_url,
                        checksum_sha256
                    );
                    // TODO: implement download + verify + atomic replace + self-restart
                });
            }
            BackendMessage::CertificateResponse {
                cert_pem,
                expires_at,
                ..
            } => {
                tracing::info!(
                    "Received new certificate (expires={}), len={}",
                    expires_at,
                    cert_pem.len()
                );
                // TODO: implement atomic cert file replacement + reconnect
            }
            BackendMessage::ApprovalResult {
                request_id,
                approved,
            } => {
                tracing::info!("Approval result for {}: approved={}", request_id, approved);
            }
        }
    }
}
