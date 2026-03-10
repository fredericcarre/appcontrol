use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::buffer::OfflineBuffer;
use crate::config::TlsSection;
use crate::scheduler::CheckScheduler;
use crate::terminal::TerminalManager;
use appcontrol_common::{AgentMessage, BackendMessage};

/// Certificate verifier that accepts any certificate (INSECURE).
/// Only use for development/testing with self-signed certificates.
#[derive(Debug)]
struct InsecureCertVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Accept any certificate without verification
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

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
    /// Operating mode: "active" (full control) or "advisory" (observe-only).
    /// In advisory mode, the agent runs health checks but refuses
    /// start/stop/rebuild commands from the backend.
    advisory_mode: bool,
    /// Terminal session manager for interactive shell access.
    terminal_manager: Arc<TerminalManager>,
    /// Skip TLS certificate verification (for self-signed certs in dev/containers).
    tls_insecure: bool,
    /// TLS configuration (cert/key/ca paths) for mTLS insecure mode.
    tls_config: Option<TlsSection>,
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
        advisory_mode: bool,
        tls_insecure: bool,
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

        let terminal_manager = Arc::new(TerminalManager::new(agent_id, msg_tx.clone()));

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
            advisory_mode,
            terminal_manager,
            tls_insecure,
            tls_config: tls_config.cloned(),
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
        let mut mgr = Self::new(
            vec![gateway_url],
            "ordered".to_string(),
            300,
            agent_id,
            labels,
            buffer,
            scheduler,
            msg_tx,
            None,
            false,
            false, // tls_insecure
        );
        mgr.tls_config = None;
        mgr
    }

    /// Get the next sequence_id for reliable message delivery.
    #[allow(dead_code)]
    fn next_sequence_id(&self) -> u64 {
        self.sequence_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn run(self, mut msg_rx: mpsc::UnboundedReceiver<AgentMessage>) {
        let mut current_url_idx = 0;
        let mut backoff_secs = 1u64;
        // Reduced from 60s to 10s for faster recovery after agent unblock
        let max_backoff = 10u64;
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
            if self.tls_insecure {
                // mTLS with insecure server verification: present client cert but accept
                // any server certificate (for self-signed gateway certs in containers)
                self.connect_tls_mtls_insecure(gateway_url, msg_rx).await
            } else {
                // Normal mTLS: present client cert and verify server certificate
                self.connect_tls(connector, gateway_url, msg_rx).await
            }
        } else if self.tls_insecure && gateway_url.starts_with("wss://") {
            // Insecure TLS mode: connect with TLS but skip certificate verification
            self.connect_tls_insecure(gateway_url, msg_rx).await
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

    /// Connect with mTLS (client cert) but skip server certificate verification.
    /// Use for self-signed gateway certificates when the agent has its own mTLS cert.
    /// WARNING: This skips server verification and should only be used in dev/containers.
    async fn connect_tls_mtls_insecure(
        &self,
        gateway_url: &str,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()> {
        use futures_util::StreamExt;

        tracing::warn!(
            "Connecting with mTLS (client cert) but INSECURE server verification — \
             do not use in production with untrusted networks"
        );

        // Parse host from URL for SNI
        let url = url::Url::parse(gateway_url)
            .map_err(|e| anyhow::anyhow!("Invalid gateway URL: {}", e))?;
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in gateway URL"))?;
        let port = url.port().unwrap_or(4443);

        // Establish TCP connection
        let tcp_stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await?;

        // Build an insecure TLS connector that presents client cert but accepts any server cert.
        // Use the TLS config from agent.yaml if available, otherwise fall back to DATA_DIR paths.
        let tls_config = match &self.tls_config {
            Some(cfg) => {
                tracing::debug!(
                    "Using TLS config from agent.yaml: cert={:?}, key={:?}, ca={:?}",
                    cfg.cert_file,
                    cfg.key_file,
                    cfg.ca_file
                );
                cfg.clone()
            }
            None => {
                // Fallback to DATA_DIR env var or default path
                let data_dir =
                    std::env::var("DATA_DIR").unwrap_or_else(|_| "/var/lib/appcontrol".to_string());
                tracing::debug!(
                    "No TLS config in agent.yaml, using DATA_DIR={} for cert paths",
                    data_dir
                );
                crate::config::TlsSection {
                    enabled: true,
                    cert_file: Some(format!("{}/tls/agent.crt", data_dir)),
                    key_file: Some(format!("{}/tls/agent.key", data_dir)),
                    ca_file: Some(format!("{}/tls/ca.crt", data_dir)),
                }
            }
        };

        let connector = crate::tls::build_tls_connector_insecure(&tls_config)
            .map_err(|e| anyhow::anyhow!("Failed to build insecure mTLS connector: {}", e))?;

        // Perform TLS handshake with mTLS (client cert) but no server verification
        let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|e| anyhow::anyhow!("Invalid server name for TLS: {}", e))?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;

        tracing::info!(
            "mTLS handshake complete (insecure server verification) with gateway {}:{} — client certificate presented",
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

    /// Connect with TLS but skip certificate verification (for self-signed certs).
    /// WARNING: This is insecure and should only be used in development/containers.
    async fn connect_tls_insecure(
        &self,
        gateway_url: &str,
        msg_rx: &mut mpsc::UnboundedReceiver<AgentMessage>,
    ) -> anyhow::Result<()> {
        use futures_util::StreamExt;
        use std::sync::Arc;

        tracing::warn!(
            "Connecting with INSECURE TLS (certificate verification disabled) — \
             do not use in production with untrusted networks"
        );

        // Parse host from URL for SNI
        let url = url::Url::parse(gateway_url)
            .map_err(|e| anyhow::anyhow!("Invalid gateway URL: {}", e))?;
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in gateway URL"))?;
        let port = url.port().unwrap_or(4443);

        // Establish TCP connection
        let tcp_stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await?;

        // Build an insecure TLS config that accepts any certificate
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureCertVerifier))
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

        // Perform TLS handshake (no client cert, no server verification)
        let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|e| anyhow::anyhow!("Invalid server name for TLS: {}", e))?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;

        tracing::info!(
            "TLS handshake complete (insecure mode) with gateway {}:{}",
            host,
            port
        );

        // Upgrade to WebSocket over TLS
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(gateway_url)
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

        // Collect system info once at startup
        let sys_info = crate::platform::get_system_info();

        // Send registration message with hostname, detected IPs, system info, and cert fingerprint
        let register = AgentMessage::Register {
            agent_id: self.agent_id,
            hostname: crate::platform::gethostname(),
            ip_addresses: crate::platform::get_ip_addresses(),
            labels: serde_json::json!(self.labels),
            version: env!("CARGO_PKG_VERSION").to_string(),
            os_name: Some(sys_info.os_name),
            os_version: Some(sys_info.os_version),
            cpu_arch: Some(sys_info.cpu_arch),
            cpu_cores: Some(sys_info.cpu_cores),
            total_memory_mb: Some(sys_info.total_memory_mb),
            disk_total_gb: Some(sys_info.disk_total_gb),
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
                    // Log terminal messages for debugging
                    if let AgentMessage::TerminalOutput { request_id, ref data } = agent_msg {
                        tracing::debug!(
                            request_id = %request_id,
                            bytes = data.len(),
                            "Sending TerminalOutput to gateway"
                        );
                    }
                    let json = serde_json::to_string(&agent_msg)?;
                    write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await?;
                }
                // Messages from backend
                Some(ws_msg) = read.next() => {
                    match ws_msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            if let Ok(backend_msg) = serde_json::from_str::<BackendMessage>(&text) {
                                // handle_backend_message returns false for DisconnectAgent
                                if !self.handle_backend_message(backend_msg) {
                                    tracing::info!("Disconnect signal received, closing connection");
                                    return Ok(());
                                }
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
    ///
    /// Returns `true` to continue processing, `false` to close the connection.
    fn handle_backend_message(&self, msg: BackendMessage) -> bool {
        match msg {
            BackendMessage::ExecuteCommand {
                request_id,
                component_id,
                command,
                timeout_seconds,
                exec_mode,
            } => {
                // Advisory mode: refuse execution commands (start/stop/rebuild).
                // Health checks are handled by the scheduler, not by ExecuteCommand.
                if self.advisory_mode && exec_mode == "detached" {
                    tracing::warn!(
                        request_id = %request_id,
                        component_id = %component_id,
                        "ADVISORY MODE — refusing detached command execution: {}",
                        command
                    );
                    let seq = self.sequence_counter.fetch_add(1, Ordering::SeqCst);
                    let _ = self.msg_tx.send(AgentMessage::CommandResult {
                        request_id,
                        exit_code: -2,
                        stdout: String::new(),
                        stderr: "Agent is in advisory mode — command execution refused. \
                                 Advisory mode is observation-only: health checks run, \
                                 but start/stop/rebuild commands are not executed."
                            .to_string(),
                        duration_ms: 0,
                        sequence_id: Some(seq),
                    });
                    return true; // continue processing other messages
                }

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
                        // Sync execution with streaming: send output chunks as they arrive
                        let chunk_tx = msg_tx.clone();
                        let start = std::time::Instant::now();
                        let on_chunk = move |stdout: String, stderr: String| {
                            let _ = chunk_tx.send(AgentMessage::CommandOutputChunk {
                                request_id,
                                stdout,
                                stderr,
                            });
                        };
                        match crate::executor::execute_sync_streaming(&command, timeout, on_chunk)
                            .await
                        {
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
                // Agent self-update: download, verify SHA-256, atomic replace, restart
                tokio::spawn(async move {
                    match crate::self_update::perform_update(
                        &binary_url,
                        &checksum_sha256,
                        &target_version,
                    )
                    .await
                    {
                        Ok(()) => {
                            // perform_update re-execs the process on success (Unix),
                            // or spawns a new process and exits (Windows).
                            tracing::info!("Agent update complete — process will restart");
                        }
                        Err(e) => {
                            tracing::error!(
                                "Agent self-update failed: {}. Continuing with current version.",
                                e
                            );
                        }
                    }
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
            BackendMessage::RequestDiscovery { request_id } => {
                tracing::info!(
                    request_id = %request_id,
                    "Received discovery scan request"
                );
                let agent_id = self.agent_id;
                let msg_tx = self.msg_tx.clone();
                tokio::spawn(async move {
                    let hostname = crate::platform::gethostname();
                    let report = crate::discovery::scan(agent_id, &hostname);
                    if let Err(e) = msg_tx.send(report) {
                        tracing::error!("Failed to send discovery report: {}", e);
                    }
                });
            }
            BackendMessage::UpdateBinaryChunk {
                update_id,
                target_version,
                checksum_sha256,
                chunk_index,
                total_chunks,
                total_size,
                data,
            } => {
                tracing::info!(
                    update_id = %update_id,
                    chunk_index = chunk_index,
                    total_chunks = total_chunks,
                    "Received binary chunk {}/{} for air-gap update v{}",
                    chunk_index + 1,
                    total_chunks,
                    target_version
                );
                let agent_id = self.agent_id;
                let msg_tx = self.msg_tx.clone();
                tokio::spawn(async move {
                    crate::self_update::handle_binary_chunk(
                        update_id,
                        &target_version,
                        &checksum_sha256,
                        chunk_index,
                        total_chunks,
                        total_size,
                        &data,
                        agent_id,
                        &msg_tx,
                    )
                    .await;
                });
            }
            BackendMessage::DisconnectAgent { agent_id, reason } => {
                tracing::warn!(
                    agent_id = %agent_id,
                    reason = %reason,
                    "Backend ordered disconnect — closing connection"
                );
                // Return false to signal the message loop to close the connection.
                // The agent will then attempt to reconnect after the backoff delay.
                // If the agent is blocked, reconnection will fail with an auth error.
                return false;
            }
            BackendMessage::CertificateRotation {
                new_ca_cert,
                grace_period_secs,
                rotation_id,
            } => {
                tracing::info!(
                    rotation_id = %rotation_id,
                    grace_period_secs = grace_period_secs,
                    "Certificate rotation command received"
                );

                // Handle certificate rotation:
                // 1. Validate the new CA certificate
                // 2. Request a new certificate signed by the new CA
                // 3. Write new certificate to disk
                // 4. Reconnect with the new certificate

                let new_ca_fingerprint = appcontrol_common::fingerprint_pem(&new_ca_cert)
                    .unwrap_or_else(|| "unknown".to_string());

                tracing::info!(
                    rotation_id = %rotation_id,
                    new_ca_fingerprint = %new_ca_fingerprint,
                    "New CA certificate received for rotation"
                );

                // Send a certificate renewal request to get a new cert signed by the new CA
                let agent_id = self.agent_id;
                let msg_tx = self.msg_tx.clone();
                let old_fingerprint = self.cert_fingerprint.clone().unwrap_or_default();

                tokio::spawn(async move {
                    // Generate a CSR for the new certificate
                    // For now, we send a placeholder CSR - in production this would use
                    // rcgen or openssl to generate a proper PKCS#10 CSR
                    let csr_placeholder = format!(
                        "-----BEGIN CERTIFICATE REQUEST-----\n\
                         rotation_id={}\n\
                         old_fingerprint={}\n\
                         -----END CERTIFICATE REQUEST-----",
                        rotation_id, old_fingerprint
                    );

                    if let Err(e) = msg_tx.send(AgentMessage::CertificateRenewal {
                        agent_id,
                        csr_pem: csr_placeholder,
                    }) {
                        tracing::error!("Failed to send certificate renewal request: {}", e);
                    } else {
                        tracing::info!(
                            rotation_id = %rotation_id,
                            "Sent certificate renewal request to backend"
                        );
                    }
                });
            }
            BackendMessage::StartTerminal {
                request_id,
                shell,
                cols,
                rows,
                env,
            } => {
                tracing::info!(
                    request_id = %request_id,
                    shell = ?shell,
                    cols = cols,
                    rows = rows,
                    "Terminal session requested"
                );

                let terminal_manager = self.terminal_manager.clone();
                let msg_tx = self.msg_tx.clone();

                tokio::spawn(async move {
                    match terminal_manager
                        .start_session(request_id, shell, cols, rows, env)
                        .await
                    {
                        Ok(()) => {
                            tracing::info!(request_id = %request_id, "Terminal session started");
                            // The terminal manager will send TerminalOutput messages directly
                        }
                        Err(e) => {
                            tracing::error!(
                                request_id = %request_id,
                                error = %e,
                                "Failed to start terminal session"
                            );
                            // Send an exit message with error code to indicate failure
                            let _ = msg_tx.send(AgentMessage::TerminalExit {
                                request_id,
                                exit_code: -1,
                            });
                        }
                    }
                });
            }
            BackendMessage::TerminalInput { request_id, data } => {
                let terminal_manager = self.terminal_manager.clone();

                tokio::spawn(async move {
                    if let Err(e) = terminal_manager.send_input(request_id, data).await {
                        tracing::debug!(
                            request_id = %request_id,
                            error = %e,
                            "Failed to send terminal input"
                        );
                    }
                });
            }
            BackendMessage::TerminalResize {
                request_id,
                cols,
                rows,
            } => {
                let terminal_manager = self.terminal_manager.clone();

                tokio::spawn(async move {
                    if let Err(e) = terminal_manager.resize(request_id, cols, rows).await {
                        tracing::debug!(
                            request_id = %request_id,
                            error = %e,
                            "Failed to resize terminal"
                        );
                    }
                });
            }
            BackendMessage::TerminalClose { request_id } => {
                tracing::info!(request_id = %request_id, "Terminal close requested");

                let terminal_manager = self.terminal_manager.clone();
                let msg_tx = self.msg_tx.clone();

                tokio::spawn(async move {
                    if let Err(e) = terminal_manager.close_session(request_id).await {
                        tracing::debug!(
                            request_id = %request_id,
                            error = %e,
                            "Failed to close terminal session"
                        );
                    }
                    // Send exit message
                    let _ = msg_tx.send(AgentMessage::TerminalExit {
                        request_id,
                        exit_code: 0,
                    });
                });
            }
            BackendMessage::RunChecksNow { request_id } => {
                tracing::info!(
                    request_id = %request_id,
                    "Received RunChecksNow — triggering immediate health checks"
                );

                let scheduler = self.scheduler.clone();
                tokio::spawn(async move {
                    scheduler.run_all_checks_now().await;
                });
            }
        }
        // Continue processing messages
        true
    }
}
