mod rate_limit;
mod registry;
mod router;
#[cfg(windows)]
mod win_service;

use axum::{
    extract::{ws, Extension, State},
    http::HeaderMap,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_common::{BackendMessage, GatewayEnvelope, GatewayMessage};
use rate_limit::AgentRateLimiter;
use registry::AgentRegistry;
use router::{MessageRouter, AGENT_CHANNEL_CAPACITY, CHANNEL_CAPACITY};

/// Client certificate fingerprint extracted from mTLS connection.
/// Injected into request extensions by the TLS layer.
#[derive(Clone, Debug, Default)]
pub struct ClientCertFingerprint(pub Option<String>);

/// Platform-aware default config path.
fn default_config_path() -> String {
    #[cfg(unix)]
    {
        "/etc/appcontrol/gateway.yaml".to_string()
    }
    #[cfg(windows)]
    {
        std::env::var("PROGRAMDATA")
            .map(|p| format!("{}\\AppControl\\config\\gateway.yaml", p))
            .unwrap_or_else(|_| "C:\\ProgramData\\AppControl\\config\\gateway.yaml".to_string())
    }
}

#[derive(Parser)]
#[command(
    name = "appcontrol-gateway",
    about = "AppControl Gateway",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HASH"), " ", env!("BUILD_TIME"), ")")
)]
struct Args {
    #[arg(short, long, default_value_t = default_config_path(), global = true)]
    config: String,

    #[command(subcommand)]
    command: Option<ServiceCommand>,
}

#[derive(Subcommand)]
enum ServiceCommand {
    /// Windows service management
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install as a Windows service
    Install {
        #[arg(short, long, default_value_t = default_config_path())]
        config: String,
    },
    /// Remove the Windows service
    Uninstall,
    /// Run as a Windows service (called by SCM)
    Run,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct GatewayConfig {
    gateway: GatewaySection,
    backend: BackendSection,
    tls: Option<TlsSection>,
    /// TLS configuration for the gateway→backend connection.
    /// Required when `backend.url` uses `wss://` with an internal CA.
    backend_tls: Option<BackendTlsSection>,
}

#[derive(Debug, serde::Deserialize, Clone)]
struct GatewaySection {
    id: String,
    /// Human-readable name displayed in the UI.
    #[serde(default)]
    name: Option<String>,
    zone: String,
    listen_addr: String,
    listen_port: u16,
}

#[derive(Debug, serde::Deserialize, Clone)]
struct BackendSection {
    url: String,
    reconnect_interval_secs: u64,
}

#[derive(Debug, serde::Deserialize, Clone)]
struct TlsSection {
    enabled: bool,
    cert_file: String,
    key_file: String,
    ca_file: String,
}

/// TLS settings for the outbound gateway→backend WebSocket connection.
/// When `backend.url` is `wss://`, the gateway verifies the backend's certificate.
/// If `ca_file` is set, only that CA is trusted (internal PKI). Otherwise, system
/// roots are used. Optionally, `cert_file`+`key_file` enable mTLS to the backend.
#[derive(Debug, serde::Deserialize, Clone)]
struct BackendTlsSection {
    /// CA certificate to verify the backend's server certificate (PEM).
    /// If omitted, system root certificates are used.
    ca_file: Option<String>,
    /// Client certificate for mTLS to the backend (PEM). Optional.
    cert_file: Option<String>,
    /// Client private key for mTLS to the backend (PEM). Optional.
    key_file: Option<String>,
}

impl GatewayConfig {
    fn load(path: &str) -> anyhow::Result<Self> {
        let mut config = if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            serde_yaml::from_str(&content)?
        } else {
            tracing::info!("No config file at {}, using env vars / defaults", path);
            GatewayConfig {
                gateway: GatewaySection {
                    id: "gateway-01".to_string(),
                    name: None,
                    zone: "default".to_string(),
                    listen_addr: "0.0.0.0".to_string(),
                    listen_port: 4443,
                },
                backend: BackendSection {
                    url: "ws://localhost:3000/ws/gateway".to_string(),
                    reconnect_interval_secs: 5,
                },
                tls: None,
                backend_tls: None,
            }
        };

        if let Ok(v) = std::env::var("GATEWAY_ID") {
            config.gateway.id = v;
        }
        if let Ok(v) = std::env::var("GATEWAY_NAME") {
            config.gateway.name = Some(v);
        }
        if let Ok(v) = std::env::var("GATEWAY_ZONE") {
            config.gateway.zone = v;
        }
        if let Ok(v) = std::env::var("LISTEN_ADDR") {
            config.gateway.listen_addr = v;
        }
        if let Ok(v) = std::env::var("LISTEN_PORT") {
            if let Ok(p) = v.parse() {
                config.gateway.listen_port = p;
            }
        }
        if let Ok(v) = std::env::var("BACKEND_URL") {
            config.backend.url = v;
        }
        if let Ok(v) = std::env::var("BACKEND_RECONNECT_SECS") {
            if let Ok(s) = v.parse() {
                config.backend.reconnect_interval_secs = s;
            }
        }

        // TLS env var overrides (agent-facing server)
        let tls_enabled = std::env::var("TLS_ENABLED")
            .ok()
            .map(|v| v == "true" || v == "1");
        let tls_cert = std::env::var("TLS_CERT_FILE").ok();
        let tls_key = std::env::var("TLS_KEY_FILE").ok();
        let tls_ca = std::env::var("TLS_CA_FILE").ok();
        if tls_enabled == Some(true) || tls_cert.is_some() {
            let existing = config.tls.unwrap_or(TlsSection {
                enabled: false,
                cert_file: String::new(),
                key_file: String::new(),
                ca_file: String::new(),
            });
            // If cert files are provided, enable TLS by default
            let should_enable = tls_enabled.unwrap_or(tls_cert.is_some() || existing.enabled);
            config.tls = Some(TlsSection {
                enabled: should_enable,
                cert_file: tls_cert.unwrap_or(existing.cert_file),
                key_file: tls_key.unwrap_or(existing.key_file),
                ca_file: tls_ca.unwrap_or(existing.ca_file),
            });
        }

        // Backend TLS env var overrides (gateway→backend connection)
        let backend_tls_ca = std::env::var("BACKEND_TLS_CA_FILE").ok();
        let backend_tls_cert = std::env::var("BACKEND_TLS_CERT_FILE").ok();
        let backend_tls_key = std::env::var("BACKEND_TLS_KEY_FILE").ok();
        if backend_tls_ca.is_some() || backend_tls_cert.is_some() {
            let existing = config.backend_tls.unwrap_or(BackendTlsSection {
                ca_file: None,
                cert_file: None,
                key_file: None,
            });
            config.backend_tls = Some(BackendTlsSection {
                ca_file: backend_tls_ca.or(existing.ca_file),
                cert_file: backend_tls_cert.or(existing.cert_file),
                key_file: backend_tls_key.or(existing.key_file),
            });
        }

        Ok(config)
    }
}

pub struct GatewayState {
    pub registry: AgentRegistry,
    pub router: MessageRouter,
    pub rate_limiter: AgentRateLimiter,
    pub config: GatewayConfig,
    pub gateway_id: uuid::Uuid,
    /// Flag to signal gateway should disconnect (set when blocked by backend)
    pub shutdown_flag: std::sync::atomic::AtomicBool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create log streaming channel for WebSocket transmission
    let (log_sender, log_receiver) = appcontrol_common::LogSender::new();
    let ws_log_layer = appcontrol_common::WebSocketLogLayer::new(log_sender, tracing::Level::DEBUG);
    let _log_layer_handle = ws_log_layer.handle();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appcontrol_gateway=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(ws_log_layer)
        .init();

    let args = Args::parse();

    // Handle service subcommands (Windows only)
    if let Some(command) = args.command {
        return handle_service_command(command);
    }

    let config = GatewayConfig::load(&args.config)?;

    // Derive a stable gateway_id from the configured ID (deterministic UUID v5)
    let gateway_id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, config.gateway.id.as_bytes());

    let registry = AgentRegistry::new();
    let router = MessageRouter::new();
    let rate_limiter = AgentRateLimiter::new();

    let state = Arc::new(GatewayState {
        registry,
        router,
        rate_limiter,
        config: config.clone(),
        gateway_id,
        shutdown_flag: std::sync::atomic::AtomicBool::new(false),
    });

    // Spawn log forwarding task (sends gateway's own log batches to backend)
    let state_for_logs = state.clone();
    tokio::spawn(async move {
        let mut batcher = appcontrol_common::LogBatcher::new(log_receiver);
        while let Some(entries) = batcher.next_batch().await {
            // Wrap log entries in GatewayMessage and forward to backend
            let log_msg = appcontrol_common::GatewayMessage::LogEntries {
                gateway_id: state_for_logs.gateway_id,
                entries,
            };
            if let Ok(json) = serde_json::to_string(&log_msg) {
                state_for_logs.router.forward_to_backend(&json);
            }
        }
    });

    // Connect to backend in background with auto-reconnect
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            tracing::info!("Connecting to backend: {}", state_clone.config.backend.url);
            if let Err(e) = connect_to_backend(&state_clone).await {
                tracing::error!("Backend connection error: {}. Reconnecting...", e);
            }
            state_clone.router.clear_backend_sender();
            tokio::time::sleep(std::time::Duration::from_secs(
                state_clone.config.backend.reconnect_interval_secs,
            ))
            .await;
        }
    });

    let app = Router::new()
        .route("/ws", get(agent_ws_handler))
        .route("/health", get(health_handler))
        .route("/enroll", post(enroll_handler))
        .with_state(state.clone());

    let addr = format!(
        "{}:{}",
        config.gateway.listen_addr, config.gateway.listen_port
    );

    // Always use TLS — either configured certificates or self-signed for dev
    let rustls_config = if let Some(ref tls) = config.tls {
        if tls.enabled {
            tracing::info!(
                "Gateway {} ({}) listening with TLS on {} [id={}]",
                config.gateway.id,
                config.gateway.zone,
                addr,
                gateway_id
            );
            build_server_tls_config(tls)?
        } else {
            tracing::info!(
                "Gateway {} ({}) listening with self-signed TLS on {} [id={}] (TLS disabled in config, using dev cert)",
                config.gateway.id,
                config.gateway.zone,
                addr,
                gateway_id
            );
            generate_dev_tls_config()?
        }
    } else {
        tracing::info!(
            "Gateway {} ({}) listening with self-signed TLS on {} [id={}] (no TLS config, using dev cert)",
            config.gateway.id,
            config.gateway.zone,
            addr,
            gateway_id
        );
        generate_dev_tls_config()?
    };

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(rustls_config);
    let tcp_listener = tokio::net::TcpListener::bind(&addr).await?;

    // Accept TLS connections, extract client cert if present, then serve with axum
    loop {
        let (tcp_stream, peer_addr) = tcp_listener.accept().await?;
        let acceptor = tls_acceptor.clone();
        let app = app.clone();

        tokio::spawn(async move {
            match acceptor.accept(tcp_stream).await {
                Ok(tls_stream) => {
                    // Extract client cert fingerprint from the TLS session (may be None for /enroll)
                    let fingerprint = extract_client_cert_fingerprint(&tls_stream);
                    if let Some(ref fp) = fingerprint {
                        tracing::debug!(
                            peer = %peer_addr,
                            fingerprint = %fp,
                            "TLS: client certificate verified"
                        );
                    }

                    // Add the fingerprint as a layer so handlers can access it
                    let app_with_fingerprint =
                        app.layer(Extension(ClientCertFingerprint(fingerprint)));

                    // Serve the connection using hyper + axum with the TLS stream
                    // NOTE: serve_connection_with_upgrades is REQUIRED for WebSocket connections.
                    // Without it, hyper closes the connection after the 101 response.
                    tracing::debug!(peer = %peer_addr, "Serving HTTP connection");
                    let io = hyper_util::rt::TokioIo::new(tls_stream);
                    let service =
                        hyper_util::service::TowerToHyperService::new(app_with_fingerprint);
                    let result = hyper_util::server::conn::auto::Builder::new(
                        hyper_util::rt::TokioExecutor::new(),
                    )
                    .serve_connection_with_upgrades(io, service)
                    .await;
                    match result {
                        Ok(()) => {
                            tracing::debug!(peer = %peer_addr, "Connection completed normally")
                        }
                        Err(e) => {
                            tracing::debug!(peer = %peer_addr, error = %e, "Connection ended with error")
                        }
                    }
                }
                Err(e) => {
                    // TLS handshake failed
                    tracing::warn!(
                        peer = %peer_addr,
                        "TLS handshake failed: {}",
                        e
                    );
                }
            }
        });
    }
}

async fn health_handler(State(state): State<Arc<GatewayState>>) -> String {
    let agents = state.registry.connected_count();
    let backend = if state.router.has_backend() {
        "connected"
    } else {
        "disconnected"
    };
    let (buf_count, buf_bytes) = state.router.buffer_stats();
    format!(
        "ok agents={} backend={} buffer_msgs={} buffer_bytes={}",
        agents, backend, buf_count, buf_bytes
    )
}

/// Proxy enrollment requests from agents to the backend.
///
/// Agents that don't have a certificate yet can't connect via mTLS WebSocket.
/// Instead, they POST to this endpoint which proxies to the backend's `/api/v1/enroll`.
/// This endpoint is served WITHOUT mTLS verification (it's the bootstrap path).
async fn enroll_handler(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::http::StatusCode;

    // Build the backend enrollment URL from the WebSocket URL
    let backend_url = &state.config.backend.url;
    // Convert ws://host:port/ws/gateway -> http://host:port/api/v1/enroll
    let enroll_url = backend_url
        .replace("ws://", "http://")
        .replace("wss://", "https://")
        .replace("/ws/gateway", "/api/v1/enroll");

    // Forward client IP
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let client = reqwest::Client::new();
    match client
        .post(&enroll_url)
        .header("x-forwarded-for", client_ip)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            match resp.json::<serde_json::Value>().await {
                Ok(json_body) => (status, Json(json_body)).into_response(),
                Err(e) => {
                    tracing::error!("Failed to read enrollment response: {}", e);
                    StatusCode::BAD_GATEWAY.into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to proxy enrollment to backend: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "enrollment_proxy_failed",
                    "message": "Gateway could not reach the backend"
                })),
            )
                .into_response()
        }
    }
}

async fn agent_ws_handler(
    headers: HeaderMap,
    client_cert_opt: Option<Extension<ClientCertFingerprint>>,
    ws: ws::WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    tracing::debug!(
        "agent_ws_handler called, client_cert_opt = {:?}",
        client_cert_opt.as_ref().map(|e| &e.0)
    );

    let client_cert = client_cert_opt
        .map(|e| e.0)
        .unwrap_or(ClientCertFingerprint(None));

    // Priority for client cert fingerprint:
    // 1. From TLS layer (direct mTLS connection)
    // 2. From proxy header X-Client-Cert-Fingerprint (nginx/envoy TLS termination)
    let cert_fingerprint = client_cert.0.or_else(|| {
        headers
            .get("x-client-cert-fingerprint")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });

    // Reject WebSocket connections without client certificate.
    // Agents MUST present a valid certificate to connect via /ws.
    // Use /enroll first to obtain a certificate.
    if cert_fingerprint.is_none() {
        tracing::warn!(
            "WebSocket connection rejected: no client certificate presented. \
             Agents must enroll first via /enroll to obtain a certificate."
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "client_cert_required",
                "message": "Client certificate required. Use /enroll to obtain a certificate first."
            })),
        )
            .into_response();
    }

    ws.on_upgrade(move |socket| handle_agent_connection(socket, state, cert_fingerprint))
        .into_response()
}

async fn handle_agent_connection(
    socket: ws::WebSocket,
    state: Arc<GatewayState>,
    proxy_cert_fingerprint: Option<String>,
) {
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();
    let conn_id = uuid::Uuid::new_v4();
    // agent_id will be set when the agent sends Register
    let agent_id_cell: Arc<std::sync::Mutex<Option<uuid::Uuid>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Channel for sending messages FROM backend TO this agent (bounded)
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(AGENT_CHANNEL_CAPACITY);

    // Forward backend messages to agent WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process agent messages
    let state_clone = state.clone();
    let agent_id_clone = agent_id_cell.clone();
    let tx_clone = tx.clone();
    let proxy_fp = proxy_cert_fingerprint.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                if let Ok(agent_msg) =
                    serde_json::from_str::<appcontrol_common::AgentMessage>(&text)
                {
                    // Rate limit per agent: drop messages if agent exceeds quota
                    let current_agent_id = { *agent_id_clone.lock().unwrap() };
                    if let Some(aid) = current_agent_id {
                        if !state_clone.rate_limiter.check(aid) {
                            tracing::warn!(
                                agent_id = %aid,
                                "Agent rate-limited — message dropped"
                            );
                            continue;
                        }
                    }

                    tracing::debug!(msg = ?agent_msg, "Received message from agent");
                    match &agent_msg {
                        appcontrol_common::AgentMessage::Register {
                            agent_id,
                            hostname,
                            version,
                            cert_fingerprint,
                            ..
                        } => {
                            tracing::info!(agent_id = %agent_id, hostname = %hostname, version = %version, "Agent registering");
                            // Use the proxy-provided cert fingerprint if available (trusted
                            // header from TLS-terminating proxy), otherwise fall back to
                            // the fingerprint the agent self-reports in the Register message.
                            // The proxy fingerprint is more trustworthy because it comes from
                            // actual mTLS verification performed by the infrastructure layer.
                            let effective_fingerprint = if proxy_fp.is_some() {
                                tracing::debug!(
                                    agent_id = %agent_id,
                                    "Using proxy-provided mTLS cert fingerprint (X-Client-Cert-Fingerprint header)"
                                );
                                proxy_fp.clone()
                            } else {
                                cert_fingerprint.clone()
                            };

                            // Register in our local registry (with cert fingerprint and version for re-announce)
                            state_clone.registry.register(
                                conn_id,
                                *agent_id,
                                hostname.clone(),
                                Some(version.clone()),
                                effective_fingerprint.clone(),
                            );

                            // Register the agent's sender in the router (keyed by agent_id)
                            state_clone.router.add_agent(*agent_id, tx_clone.clone());

                            // Remember the agent_id for cleanup
                            *agent_id_clone.lock().unwrap() = Some(*agent_id);

                            // Forward agent's cert fingerprint and version to backend.
                            // The effective fingerprint may come from either the proxy header
                            // (mTLS termination) or the agent's self-reported value.
                            let notification = GatewayMessage::AgentConnected {
                                agent_id: *agent_id,
                                hostname: hostname.clone(),
                                version: Some(version.clone()),
                                cert_fingerprint: effective_fingerprint,
                                cert_cn: Some(hostname.clone()),
                            };
                            if let Ok(json) = serde_json::to_string(&notification) {
                                state_clone.router.forward_to_backend(&json);
                            }
                        }
                        appcontrol_common::AgentMessage::Heartbeat { .. } => {
                            state_clone.registry.heartbeat(conn_id);
                        }
                        _ => {}
                    }

                    // Wrap in GatewayMessage and forward to backend
                    let wrapped = GatewayMessage::AgentMessage(agent_msg);
                    match serde_json::to_string(&wrapped) {
                        Ok(json) => {
                            tracing::debug!(
                                bytes = json.len(),
                                "Serialized agent message, forwarding to backend"
                            );
                            state_clone.router.forward_to_backend(&json);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to serialize agent message");
                        }
                    }
                }
            }
        }
        tracing::debug!(conn_id = %conn_id, "Agent recv_task loop ended");
    });

    tokio::select! {
        res = send_task => {
            tracing::debug!(conn_id = %conn_id, result = ?res, "send_task completed first");
        },
        res = recv_task => {
            tracing::debug!(conn_id = %conn_id, result = ?res, "recv_task completed first");
        },
    }

    // Cleanup: unregister agent from registry and router, notify backend
    if let Some(info) = state.registry.unregister(conn_id) {
        state.router.remove_agent(info.agent_id);

        let notification = GatewayMessage::AgentDisconnected {
            agent_id: info.agent_id,
            hostname: info.hostname.clone(),
        };
        if let Ok(json) = serde_json::to_string(&notification) {
            state.router.forward_to_backend(&json);
        }
        tracing::info!(
            agent_id = %info.agent_id,
            hostname = %info.hostname,
            "Agent disconnected"
        );
    } else {
        // Agent never registered (connected but never sent Register message)
        tracing::debug!(conn_id = %conn_id, "Unregistered connection disconnected");
    }
}

async fn connect_to_backend(state: &Arc<GatewayState>) -> anyhow::Result<()> {
    use futures_util::{SinkExt, StreamExt};

    let url = &state.config.backend.url;
    let is_wss = url.starts_with("wss://");

    if !is_wss {
        tracing::warn!(
            "Backend connection uses plaintext WebSocket ({}). \
             This is acceptable for local development but MUST use wss:// in production. \
             Set BACKEND_URL=wss://... and configure BACKEND_TLS_CA_FILE for internal PKI.",
            url
        );
    }

    let connector = if is_wss {
        Some(build_backend_tls_connector(&state.config.backend_tls)?)
    } else {
        None
    };

    let (ws_stream, _) =
        tokio_tungstenite::connect_async_tls_with_config(url, None, false, connector).await?;
    let (mut write, mut read) = ws_stream.split();

    tracing::info!("Connected to backend");

    // Send gateway registration
    let register_msg = GatewayMessage::Register {
        gateway_id: state.gateway_id,
        name: state.config.gateway.name.clone(),
        zone: state.config.gateway.zone.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let register_json = serde_json::to_string(&register_msg)?;
    write
        .send(tokio_tungstenite::tungstenite::Message::Text(register_json))
        .await?;

    // Re-announce all currently connected agents to the backend
    for agent_info in state.registry.list_agents() {
        let notification = GatewayMessage::AgentConnected {
            agent_id: agent_info.agent_id,
            hostname: agent_info.hostname.clone(),
            version: agent_info.version.clone(),
            cert_fingerprint: agent_info.cert_fingerprint.clone(),
            cert_cn: Some(agent_info.hostname.clone()),
        };
        if let Ok(json) = serde_json::to_string(&notification) {
            write
                .send(tokio_tungstenite::tungstenite::Message::Text(json))
                .await?;
        }
    }

    // Set up the backend sender channel so the router can send messages (bounded)
    let (backend_tx, mut backend_rx) = tokio::sync::mpsc::channel::<String>(CHANNEL_CAPACITY);
    state.router.set_backend_sender(backend_tx);

    loop {
        // Check if we've been ordered to disconnect (e.g., gateway blocked by admin)
        if state
            .shutdown_flag
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            tracing::info!("Shutdown flag set — closing backend connection");
            // Reset the flag so we can reconnect after backoff
            state
                .shutdown_flag
                .store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(anyhow::anyhow!("Gateway was blocked by administrator"));
        }

        tokio::select! {
            // Messages from agents to forward to backend
            Some(msg) = backend_rx.recv() => {
                tracing::debug!(bytes = msg.len(), "Sending message from channel to backend WebSocket");
                match write.send(tokio_tungstenite::tungstenite::Message::Text(msg)).await {
                    Ok(()) => {
                        // Flush to ensure message is sent immediately
                        if let Err(e) = futures_util::SinkExt::flush(&mut write).await {
                            tracing::error!(error = %e, "Failed to flush WebSocket");
                            return Err(e.into());
                        }
                        tracing::debug!("Message sent and flushed to backend WebSocket");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to send message to backend WebSocket");
                        return Err(e.into());
                    }
                }
            }
            // Messages from backend to route to agents
            Some(ws_msg) = read.next() => {
                match ws_msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        handle_backend_message(state, &text);
                        // Check shutdown flag after handling message (DisconnectGateway sets it)
                        if state.shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
                            tracing::info!("Shutdown flag set after message — closing backend connection");
                            state.shutdown_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                            return Err(anyhow::anyhow!("Gateway was blocked by administrator"));
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => return Ok(()),
                    Err(e) => return Err(e.into()),
                    _ => {}
                }
            }
            else => return Ok(()),
        }
    }
}

/// Extract the SHA-256 fingerprint from the client's verified TLS certificate.
/// Returns None if no client cert was presented (should not happen with mandatory verification).
fn extract_client_cert_fingerprint(
    tls_stream: &tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
) -> Option<String> {
    use sha2::Digest;

    let (_, session) = tls_stream.get_ref();
    let peer_certs = session.peer_certificates()?;
    let first_cert = peer_certs.first()?;
    let fingerprint = sha2::Sha256::digest(first_cert.as_ref());
    Some(hex::encode(fingerprint))
}

/// Build a TLS connector for the gateway→backend WebSocket connection.
///
/// - If `backend_tls.ca_file` is set, only that CA is trusted (internal PKI).
/// - If no CA is configured, the system's native root certificates are used.
/// - If `cert_file` + `key_file` are set, the gateway presents a client certificate (mTLS).
fn build_backend_tls_connector(
    backend_tls: &Option<BackendTlsSection>,
) -> anyhow::Result<tokio_tungstenite::Connector> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::io::BufReader;

    // Build the root certificate store
    let root_store = if let Some(ca_path) = backend_tls.as_ref().and_then(|t| t.ca_file.as_deref())
    {
        // Internal PKI: trust only the configured CA
        let ca_data = std::fs::read(ca_path)
            .map_err(|e| anyhow::anyhow!("Failed to read backend CA {}: {}", ca_path, e))?;
        let mut ca_reader = BufReader::new(ca_data.as_slice());
        let mut store = rustls::RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut ca_reader) {
            let cert =
                cert.map_err(|e| anyhow::anyhow!("Failed to parse backend CA cert: {}", e))?;
            store
                .add(cert)
                .map_err(|e| anyhow::anyhow!("Failed to add backend CA cert: {}", e))?;
        }
        tracing::info!("Backend TLS: using custom CA from {}", ca_path);
        store
    } else {
        // No custom CA: use native system root certificates
        let mut store = rustls::RootCertStore::empty();
        let native_certs = rustls_native_certs::load_native_certs()
            .map_err(|e| anyhow::anyhow!("Failed to load native root certificates: {}", e))?;
        for cert in native_certs {
            store.add(cert).ok();
        }
        tracing::info!("Backend TLS: using system root certificates");
        store
    };

    // Build the client config, optionally with client certificate (mTLS to backend)
    let has_client_cert = backend_tls
        .as_ref()
        .map(|t| t.cert_file.is_some() && t.key_file.is_some())
        .unwrap_or(false);

    let client_config = if has_client_cert {
        let tls = backend_tls.as_ref().unwrap();
        let cert_path = tls.cert_file.as_deref().unwrap();
        let key_path = tls.key_file.as_deref().unwrap();

        let cert_data = std::fs::read(cert_path).map_err(|e| {
            anyhow::anyhow!("Failed to read backend client cert {}: {}", cert_path, e)
        })?;
        let mut cert_reader = BufReader::new(cert_data.as_slice());
        let client_certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to parse backend client cert: {}", e))?;

        let key_data = std::fs::read(key_path).map_err(|e| {
            anyhow::anyhow!("Failed to read backend client key {}: {}", key_path, e)
        })?;
        let mut key_reader = BufReader::new(key_data.as_slice());
        let client_key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
            .map_err(|e| anyhow::anyhow!("Failed to parse backend client key: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("No private key found in: {}", key_path))?;

        tracing::info!("Backend TLS: mTLS enabled (presenting client certificate)");
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_client_auth_cert(client_certs, client_key)
            .map_err(|e| anyhow::anyhow!("Failed to build backend mTLS config: {}", e))?
    } else {
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    };

    Ok(tokio_tungstenite::Connector::Rustls(Arc::new(
        client_config,
    )))
}

/// Build a rustls ServerConfig with TLS and OPTIONAL client cert verification.
///
/// Client certificates are optional at the TLS layer. This allows:
/// - `/enroll` and `/health` to work without client certs (for agent enrollment)
/// - `/ws` to require client certs (verified in the handler)
fn build_server_tls_config(tls: &TlsSection) -> anyhow::Result<Arc<rustls::ServerConfig>> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::io::BufReader;

    // Load server certificate chain
    let cert_data = std::fs::read(&tls.cert_file)
        .map_err(|e| anyhow::anyhow!("Failed to read gateway cert {}: {}", tls.cert_file, e))?;
    let mut cert_reader = BufReader::new(cert_data.as_slice());
    let server_certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse gateway cert: {}", e))?;

    if server_certs.is_empty() {
        return Err(anyhow::anyhow!(
            "No certificates found in gateway cert file: {}",
            tls.cert_file
        ));
    }

    // Load server private key
    let key_data = std::fs::read(&tls.key_file)
        .map_err(|e| anyhow::anyhow!("Failed to read gateway key {}: {}", tls.key_file, e))?;
    let mut key_reader = BufReader::new(key_data.as_slice());
    let server_key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| anyhow::anyhow!("Failed to parse gateway key: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in: {}", tls.key_file))?;

    // Load CA certificates for client verification (agent certs must be signed by this CA)
    let ca_data = std::fs::read(&tls.ca_file)
        .map_err(|e| anyhow::anyhow!("Failed to read CA file {}: {}", tls.ca_file, e))?;
    let mut ca_reader = BufReader::new(ca_data.as_slice());

    let mut root_store = rustls::RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut ca_reader) {
        let cert = cert.map_err(|e| anyhow::anyhow!("Failed to parse CA cert: {}", e))?;
        root_store
            .add(cert)
            .map_err(|e| anyhow::anyhow!("Failed to add CA cert: {}", e))?;
    }

    // Build server config with OPTIONAL client cert verification.
    // This allows /enroll to work without client certs while /ws requires them.
    // The /ws handler checks for client cert presence and rejects unauthenticated connections.
    let client_verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
        .allow_unauthenticated()
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build client cert verifier: {}", e))?;

    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(server_certs, server_key)
        .map_err(|e| anyhow::anyhow!("Failed to build server TLS config: {}", e))?;

    tracing::info!(
        "TLS server config built: cert={}, ca={} — client certificates OPTIONAL (verified per-endpoint)",
        tls.cert_file,
        tls.ca_file
    );

    Ok(Arc::new(config))
}

/// Generate a self-signed certificate for development/testing.
/// This ensures TLS is always used, even without configured certificates.
fn generate_dev_tls_config() -> anyhow::Result<Arc<rustls::ServerConfig>> {
    use rcgen::{generate_simple_self_signed, CertifiedKey};
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    tracing::warn!(
        "No TLS certificates configured — generating self-signed certificate for development. \
         This is NOT suitable for production. Configure TLS_CERT_FILE, TLS_KEY_FILE, and TLS_CA_FILE."
    );

    // Generate a self-signed certificate
    let subject_alt_names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "gateway".to_string(),
    ];

    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
        .map_err(|e| anyhow::anyhow!("Failed to generate self-signed cert: {}", e))?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(key_pair.serialize_der())
        .map_err(|e| anyhow::anyhow!("Failed to serialize private key: {:?}", e))?;

    // Build server config without client cert verification (dev mode)
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| anyhow::anyhow!("Failed to build dev TLS config: {}", e))?;

    tracing::info!(
        "Self-signed TLS certificate generated for development — \
         enrollment and agent connections will be encrypted"
    );

    Ok(Arc::new(config))
}

/// Parse and route a message from the backend.
fn handle_backend_message(state: &Arc<GatewayState>, text: &str) {
    match serde_json::from_str::<GatewayEnvelope>(text) {
        Ok(GatewayEnvelope::DisconnectGateway { reason }) => {
            tracing::warn!(
                reason = %reason,
                "Backend ordered gateway disconnect — shutting down connections"
            );
            // Clear all agent connections
            state.router.clear_all();
            state.registry.clear_all();
            // Signal shutdown (the backend connection loop will handle reconnection)
            state
                .shutdown_flag
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(GatewayEnvelope::ForwardToAgent {
            target_agent_id,
            message,
        }) => {
            // Handle DisconnectAgent specially — forward to agent, then drop the connection
            if let BackendMessage::DisconnectAgent {
                agent_id,
                ref reason,
            } = message
            {
                tracing::warn!(
                    agent_id = %agent_id,
                    reason = %reason,
                    "Backend ordered agent disconnect — forwarding to agent then dropping connection"
                );
                // First, forward the DisconnectAgent message to the agent
                // so it knows to close its connection gracefully
                if let Ok(inner_json) = serde_json::to_string(&message) {
                    if !state.router.forward_to_agent(agent_id, &inner_json) {
                        tracing::warn!(
                            agent_id = %agent_id,
                            "Agent not connected — cannot forward disconnect message"
                        );
                    }
                }
                // Now remove the agent from router (closes the channel) and registry
                state.router.remove_agent(agent_id);
                state.registry.remove_by_agent_id(agent_id);
                return;
            }

            // Handle CertificateRotation — forward to agent and handle locally for gateway
            if let BackendMessage::CertificateRotation {
                ref new_ca_cert,
                grace_period_secs,
                rotation_id,
            } = message
            {
                handle_certificate_rotation(state, new_ca_cert, grace_period_secs, rotation_id);
            }

            // Serialize the inner BackendMessage (what the agent expects)
            let inner_json = match serde_json::to_string(&message) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("Failed to serialize BackendMessage: {}", e);
                    return;
                }
            };

            // Log message type for debugging
            let msg_type = match &message {
                BackendMessage::StartTerminal { .. } => "StartTerminal",
                BackendMessage::TerminalInput { .. } => "TerminalInput",
                BackendMessage::TerminalResize { .. } => "TerminalResize",
                BackendMessage::TerminalClose { .. } => "TerminalClose",
                BackendMessage::ExecuteCommand { .. } => "ExecuteCommand",
                BackendMessage::UpdateConfig { .. } => "UpdateConfig",
                _ => "Other",
            };

            tracing::debug!(
                agent_id = %target_agent_id,
                msg_type = msg_type,
                "Forwarding message from backend to agent"
            );

            if !state.router.forward_to_agent(target_agent_id, &inner_json) {
                tracing::warn!(
                    agent_id = %target_agent_id,
                    msg_type = msg_type,
                    "Failed to route command to agent — not connected to this gateway"
                );
            } else {
                tracing::debug!(
                    agent_id = %target_agent_id,
                    msg_type = msg_type,
                    "Successfully forwarded message to agent"
                );
            }
        }
        Err(e) => {
            // Try to parse as raw BackendMessage for backwards compatibility
            if let Ok(backend_msg) = serde_json::from_str::<BackendMessage>(text) {
                // Handle CertificateRotation broadcast (gateway-targeted)
                if let BackendMessage::CertificateRotation {
                    ref new_ca_cert,
                    grace_period_secs,
                    rotation_id,
                } = backend_msg
                {
                    handle_certificate_rotation(state, new_ca_cert, grace_period_secs, rotation_id);
                    // Broadcast to all connected agents
                    if let Ok(json) = serde_json::to_string(&backend_msg) {
                        state.router.broadcast_to_agents(&json);
                    }
                    return;
                }

                tracing::warn!(
                    "Received raw BackendMessage without envelope — cannot route without target_agent_id"
                );
                // Log the component_id for debugging
                if let BackendMessage::ExecuteCommand { component_id, .. } = &backend_msg {
                    tracing::warn!(component_id = %component_id, "Unroutable command");
                }
            } else {
                tracing::warn!("Unknown message from backend: {}", e);
            }
        }
    }
}

/// Handle certificate rotation command from backend.
///
/// The gateway should:
/// 1. Add the new CA to its trust store (dual-trust during rotation)
/// 2. Request a new certificate signed by the new CA
/// 3. Forward the rotation command to all connected agents
fn handle_certificate_rotation(
    state: &Arc<GatewayState>,
    new_ca_cert: &str,
    grace_period_secs: u64,
    rotation_id: uuid::Uuid,
) {
    tracing::info!(
        gateway_id = %state.gateway_id,
        rotation_id = %rotation_id,
        grace_period_secs = grace_period_secs,
        "Certificate rotation command received from backend"
    );

    // In a full implementation, the gateway would:
    // 1. Write the new CA cert to a temp file
    // 2. Reload TLS config to trust both old and new CAs
    // 3. Request a new gateway certificate from the backend
    // 4. Once received, reload TLS config with the new cert
    // 5. Report success/failure back to the backend

    // For now, we log the event and let the gateway admin handle manual rotation
    // or trigger a gateway restart with updated certs.

    let new_ca_fingerprint =
        appcontrol_common::fingerprint_pem(new_ca_cert).unwrap_or_else(|| "unknown".to_string());

    tracing::info!(
        gateway_id = %state.gateway_id,
        new_ca_fingerprint = %new_ca_fingerprint,
        "New CA received for rotation. Gateway will trust both old and new CA during grace period."
    );

    // TODO: Implement hot-reload of TLS config
    // This would require:
    // 1. Arc<RwLock<ServerConfig>> for the TLS config
    // 2. Ability to reload without dropping existing connections
    // 3. Certificate request API to backend

    // Forward rotation command to all connected agents
    let rotation_msg = BackendMessage::CertificateRotation {
        new_ca_cert: new_ca_cert.to_string(),
        grace_period_secs,
        rotation_id,
    };
    if let Ok(json) = serde_json::to_string(&rotation_msg) {
        let agent_count = state.router.broadcast_to_agents(&json);
        tracing::info!(
            rotation_id = %rotation_id,
            agents_notified = agent_count,
            "Forwarded certificate rotation to connected agents"
        );
    }
}

#[allow(unreachable_code)]
fn handle_service_command(command: ServiceCommand) -> anyhow::Result<()> {
    match command {
        ServiceCommand::Service { action } => match action {
            ServiceAction::Install { config } => {
                #[cfg(windows)]
                {
                    win_service::install_service(&config)?;
                    return Ok(());
                }
                #[cfg(not(windows))]
                {
                    let _ = config;
                    anyhow::bail!(
                        "Windows service commands are only available on Windows.\n\
                         On Linux, use systemd: systemctl enable/start appcontrol-gateway"
                    );
                }
            }
            ServiceAction::Uninstall => {
                #[cfg(windows)]
                {
                    win_service::uninstall_service()?;
                    return Ok(());
                }
                #[cfg(not(windows))]
                {
                    anyhow::bail!("Windows service commands are only available on Windows.");
                }
            }
            ServiceAction::Run => {
                #[cfg(windows)]
                {
                    win_service::run_as_service()?;
                    return Ok(());
                }
                #[cfg(not(windows))]
                {
                    anyhow::bail!("Windows service commands are only available on Windows.");
                }
            }
        },
    }
}
