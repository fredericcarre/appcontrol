mod rate_limit;
mod registry;
mod router;

use axum::{
    extract::{ws, State},
    http::HeaderMap,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_common::{BackendMessage, GatewayEnvelope, GatewayMessage};
use rate_limit::AgentRateLimiter;
use registry::AgentRegistry;
use router::{MessageRouter, AGENT_CHANNEL_CAPACITY, CHANNEL_CAPACITY};

#[derive(Parser)]
#[command(name = "appcontrol-gateway", about = "AppControl Gateway")]
struct Args {
    #[arg(short, long, default_value = "/etc/appcontrol/gateway.yaml")]
    config: String,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct GatewayConfig {
    gateway: GatewaySection,
    backend: BackendSection,
    tls: Option<TlsSection>,
}

#[derive(Debug, serde::Deserialize, Clone)]
struct GatewaySection {
    id: String,
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
                    zone: "default".to_string(),
                    listen_addr: "0.0.0.0".to_string(),
                    listen_port: 4443,
                },
                backend: BackendSection {
                    url: "ws://localhost:3000/ws/gateway".to_string(),
                    reconnect_interval_secs: 5,
                },
                tls: None,
            }
        };

        if let Ok(v) = std::env::var("GATEWAY_ID") {
            config.gateway.id = v;
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

        // TLS env var overrides
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
            config.tls = Some(TlsSection {
                enabled: tls_enabled.unwrap_or(existing.enabled),
                cert_file: tls_cert.unwrap_or(existing.cert_file),
                key_file: tls_key.unwrap_or(existing.key_file),
                ca_file: tls_ca.unwrap_or(existing.ca_file),
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appcontrol_gateway=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
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

    // Serve with mTLS if TLS is configured, otherwise plaintext
    if let Some(ref tls) = config.tls {
        if tls.enabled {
            tracing::info!(
                "Gateway {} ({}) listening with mTLS on {} [id={}]",
                config.gateway.id,
                config.gateway.zone,
                addr,
                gateway_id
            );

            let rustls_config = build_server_tls_config(tls)?;
            let tls_acceptor = tokio_rustls::TlsAcceptor::from(rustls_config);
            let tcp_listener = tokio::net::TcpListener::bind(&addr).await?;

            // Accept TLS connections with client cert verification, then serve with axum
            loop {
                let (tcp_stream, peer_addr) = tcp_listener.accept().await?;
                let acceptor = tls_acceptor.clone();
                let app = app.clone();

                tokio::spawn(async move {
                    match acceptor.accept(tcp_stream).await {
                        Ok(tls_stream) => {
                            // Extract client cert fingerprint from the verified TLS session
                            let fingerprint = extract_client_cert_fingerprint(&tls_stream);
                            if let Some(ref fp) = fingerprint {
                                tracing::debug!(
                                    peer = %peer_addr,
                                    fingerprint = %fp,
                                    "mTLS: client certificate verified"
                                );
                            }

                            // Serve the connection using hyper + axum with the TLS stream
                            let io = hyper_util::rt::TokioIo::new(tls_stream);
                            let service = hyper_util::service::TowerToHyperService::new(app);
                            if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                                hyper_util::rt::TokioExecutor::new(),
                            )
                            .serve_connection(io, service)
                            .await
                            {
                                tracing::debug!("Connection from {} ended: {}", peer_addr, e);
                            }
                        }
                        Err(e) => {
                            // TLS handshake failed — agent did not present a valid cert
                            tracing::warn!(
                                peer = %peer_addr,
                                "mTLS handshake rejected: {} — agent must present a valid certificate",
                                e
                            );
                        }
                    }
                });
            }
        } else {
            tracing::warn!(
                "Gateway {} ({}) listening in PLAINTEXT on {} [id={}] (TLS disabled in config)",
                config.gateway.id,
                config.gateway.zone,
                addr,
                gateway_id
            );
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;
        }
    } else {
        tracing::warn!(
            "Gateway {} ({}) listening in PLAINTEXT on {} [id={}] (no TLS config)",
            config.gateway.id,
            config.gateway.zone,
            addr,
            gateway_id
        );
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    }

    Ok(())
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
    ws: ws::WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
) -> impl axum::response::IntoResponse {
    // Extract mTLS fingerprint from proxy header (set by nginx/envoy TLS termination).
    // When a TLS-terminating proxy (nginx, envoy, HAProxy) handles mTLS, it can
    // pass the client certificate fingerprint via a trusted header. This allows the
    // gateway to verify agent identity even when TLS is terminated upstream.
    // If the header is not present, we fall back to the fingerprint the agent sends
    // in its Register message.
    let cert_fingerprint = headers
        .get("x-client-cert-fingerprint")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    ws.on_upgrade(move |socket| handle_agent_connection(socket, state, cert_fingerprint))
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

                    match &agent_msg {
                        appcontrol_common::AgentMessage::Register {
                            agent_id,
                            hostname,
                            cert_fingerprint,
                            ..
                        } => {
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

                            // Register in our local registry (with cert fingerprint for re-announce)
                            state_clone.registry.register(
                                conn_id,
                                *agent_id,
                                hostname.clone(),
                                effective_fingerprint.clone(),
                            );

                            // Register the agent's sender in the router (keyed by agent_id)
                            state_clone.router.add_agent(*agent_id, tx_clone.clone());

                            // Remember the agent_id for cleanup
                            *agent_id_clone.lock().unwrap() = Some(*agent_id);

                            // Forward agent's cert fingerprint to backend.
                            // The effective fingerprint may come from either the proxy header
                            // (mTLS termination) or the agent's self-reported value.
                            let notification = GatewayMessage::AgentConnected {
                                agent_id: *agent_id,
                                hostname: hostname.clone(),
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
                    if let Ok(json) = serde_json::to_string(&wrapped) {
                        state_clone.router.forward_to_backend(&json);
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
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
    use tokio_tungstenite::connect_async;

    let (ws_stream, _) = connect_async(&state.config.backend.url).await?;
    let (mut write, mut read) = ws_stream.split();

    tracing::info!("Connected to backend");

    // Send gateway registration
    let register_msg = GatewayMessage::Register {
        gateway_id: state.gateway_id,
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
        tokio::select! {
            // Messages from agents to forward to backend
            Some(msg) = backend_rx.recv() => {
                write.send(tokio_tungstenite::tungstenite::Message::Text(msg)).await?;
            }
            // Messages from backend to route to agents
            Some(ws_msg) = read.next() => {
                match ws_msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        handle_backend_message(state, &text);
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

/// Build a rustls ServerConfig with mTLS: server cert + mandatory client cert verification.
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

    // Build server config with MANDATORY client cert verification
    // Agents MUST present a valid certificate signed by the configured CA.
    let client_verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build client cert verifier: {}", e))?;

    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(server_certs, server_key)
        .map_err(|e| anyhow::anyhow!("Failed to build server TLS config: {}", e))?;

    tracing::info!(
        "mTLS server config built: cert={}, ca={} — client certificates REQUIRED",
        tls.cert_file,
        tls.ca_file
    );

    Ok(Arc::new(config))
}

/// Parse and route a message from the backend.
fn handle_backend_message(state: &Arc<GatewayState>, text: &str) {
    match serde_json::from_str::<GatewayEnvelope>(text) {
        Ok(GatewayEnvelope::ForwardToAgent {
            target_agent_id,
            message,
        }) => {
            // Serialize the inner BackendMessage (what the agent expects)
            let inner_json = match serde_json::to_string(&message) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("Failed to serialize BackendMessage: {}", e);
                    return;
                }
            };

            if !state.router.forward_to_agent(target_agent_id, &inner_json) {
                tracing::warn!(
                    agent_id = %target_agent_id,
                    "Failed to route command to agent — not connected to this gateway"
                );
            }
        }
        Err(e) => {
            // Try to parse as raw BackendMessage for backwards compatibility
            if let Ok(backend_msg) = serde_json::from_str::<BackendMessage>(text) {
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
