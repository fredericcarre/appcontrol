mod rate_limit;
mod registry;
mod router;

use axum::{
    extract::{ws, State},
    routing::get,
    Router,
};
use clap::Parser;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_common::{BackendMessage, GatewayEnvelope, GatewayMessage};
use rate_limit::AgentRateLimiter;
use registry::AgentRegistry;
use router::MessageRouter;

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
        .with_state(state.clone());

    let addr = format!(
        "{}:{}",
        config.gateway.listen_addr, config.gateway.listen_port
    );
    tracing::info!(
        "Gateway {} ({}) listening on {} [id={}]",
        config.gateway.id,
        config.gateway.zone,
        addr,
        gateway_id
    );
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

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

async fn agent_ws_handler(
    ws: ws::WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_agent_connection(socket, state))
}

async fn handle_agent_connection(socket: ws::WebSocket, state: Arc<GatewayState>) {
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();
    let conn_id = uuid::Uuid::new_v4();
    // agent_id will be set when the agent sends Register
    let agent_id_cell: Arc<std::sync::Mutex<Option<uuid::Uuid>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Channel for sending messages FROM backend TO this agent
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

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
                            // Register in our local registry (with cert fingerprint for re-announce)
                            state_clone
                                .registry
                                .register(conn_id, *agent_id, hostname.clone(), cert_fingerprint.clone());

                            // Register the agent's sender in the router (keyed by agent_id)
                            state_clone.router.add_agent(*agent_id, tx_clone.clone());

                            // Remember the agent_id for cleanup
                            *agent_id_clone.lock().unwrap() = Some(*agent_id);

                            // Forward agent's cert fingerprint from Register message to backend.
                            // The agent includes its own certificate fingerprint in the Register
                            // message when mTLS is configured. This binds agent identity to cert.
                            let notification = GatewayMessage::AgentConnected {
                                agent_id: *agent_id,
                                hostname: hostname.clone(),
                                cert_fingerprint: cert_fingerprint.clone(),
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

    // Set up the backend sender channel so the router can send messages
    let (backend_tx, mut backend_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
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
