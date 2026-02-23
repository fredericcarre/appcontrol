use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::buffer::OfflineBuffer;
use crate::config::TlsSection;
use crate::scheduler::CheckScheduler;
use appcontrol_common::{AgentMessage, BackendMessage};

/// Manages the WebSocket connection to the gateway/backend.
/// Supports multi-gateway failover with ordered strategy.
/// When TLS is configured, uses mTLS with client certificate authentication.
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
    /// TLS connector for mTLS connections (None = plaintext, Some = mTLS enforced).
    tls_connector: Option<tokio_rustls::TlsConnector>,
    /// SHA-256 fingerprint of the agent's client certificate.
    cert_fingerprint: Option<String>,
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
        tls_config: Option<&TlsSection>,
    ) -> Self {
        let (tls_connector, cert_fingerprint) = match tls_config {
            Some(tls) if tls.enabled => {
                let connector = match crate::tls::build_tls_connector(tls) {
                    Ok(c) => {
                        tracing::info!("mTLS enabled — agent will present client certificate");
                        Some(c)
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to build TLS connector: {} — connections will fail",
                            e
                        );
                        None
                    }
                };
                let fingerprint = crate::tls::compute_cert_fingerprint(tls);
                if let Some(ref fp) = fingerprint {
                    tracing::info!("Agent cert fingerprint: {}", fp);
                }
                (connector, fingerprint)
            }
            _ => {
                tracing::warn!("TLS not configured — agent will connect in PLAINTEXT (not recommended for production)");
                (None, None)
            }
        };

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
            tls_connector,
            cert_fingerprint,
        }
    }

    /// Backward-compatible constructor for single gateway URL (plaintext, for testing only).
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
            None,
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
        if let Some(ref connector) = self.tls_connector {
            self.connect_tls(connector, gateway_url, msg_rx).await
        } else {
            self.connect_plaintext(gateway_url, msg_rx).await
        }
    }

    /// Connect with mTLS: TCP → TLS handshake (with client cert) → WebSocket upgrade.
    async fn connect_tls(
        &self,
        connector: &tokio_rustls::TlsConnector,
        gateway_url: &str,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()> {
        use futures_util::StreamExt;

        // Parse host from URL for SNI
        let url = url::Url::parse(gateway_url)
            .map_err(|e| anyhow::anyhow!("Invalid gateway URL: {}", e))?;
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in gateway URL"))?;
        let port = url.port().unwrap_or(4443);

        // Establish TCP connection
        let tcp_stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await?;

        // Perform TLS handshake with mTLS (client cert presented automatically)
        let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|e| anyhow::anyhow!("Invalid server name for TLS: {}", e))?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;

        tracing::info!(
            "mTLS handshake complete with gateway {}:{} — client certificate presented",
            host,
            port
        );

        // Upgrade to WebSocket over TLS
        let ws_url = if gateway_url.starts_with("ws://") {
            gateway_url.replace("ws://", "wss://")
        } else {
            gateway_url.to_string()
        };
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Host", host)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .header("Sec-WebSocket-Version", "13")
            .body(())
            .map_err(|e| anyhow::anyhow!("Failed to build WS request: {}", e))?;

        let (ws_stream, _) =
            tokio_tungstenite::client_async(request, tokio_rustls::TlsStream::from(tls_stream))
                .await?;
        let (mut write, mut read) = ws_stream.split();

        self.register_and_run(&mut write, &mut read, gateway_url, msg_rx)
            .await
    }

    /// Connect without TLS (development/testing only).
    async fn connect_plaintext(
        &self,
        gateway_url: &str,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()> {
        use futures_util::StreamExt;
        let (ws_stream, _) = tokio_tungstenite::connect_async(gateway_url).await?;
        let (mut write, mut read) = ws_stream.split();

        self.register_and_run(&mut write, &mut read, gateway_url, msg_rx)
            .await
    }

    /// Send registration, replay buffer, and run the message loop.
    /// Generic over the WebSocket stream type (works with both TLS and plaintext).
    async fn register_and_run<S>(
        &self,
        write: &mut futures_util::stream::SplitSink<S, tokio_tungstenite::tungstenite::Message>,
        read: &mut futures_util::stream::SplitStream<S>,
        gateway_url: &str,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()>
    where
        S: futures_util::Stream<
                Item = Result<
                    tokio_tungstenite::tungstenite::Message,
                    tokio_tungstenite::tungstenite::Error,
                >,
            > + futures_util::Sink<
                tokio_tungstenite::tungstenite::Message,
                Error = tokio_tungstenite::tungstenite::Error,
            > + Unpin,
    {
        use futures_util::{SinkExt, StreamExt};

        // Send registration message with hostname, detected IPs, and cert fingerprint
        let register = AgentMessage::Register {
            agent_id: self.agent_id,
            hostname: crate::platform::gethostname(),
            ip_addresses: crate::platform::get_ip_addresses(),
            labels: serde_json::json!(self.labels),
            version: env!("CARGO_PKG_VERSION").to_string(),
            cert_fingerprint: self.cert_fingerprint.clone(),
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
                        // Use platform-specific detachment for start/stop/rebuild commands.
                        // Unix: double-fork + setsid — process survives agent crash.
                        // Windows: CreateProcess with DETACHED_PROCESS flag.
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
