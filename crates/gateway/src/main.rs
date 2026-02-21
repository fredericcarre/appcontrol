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
    /// Load config from YAML file, then apply env var overrides.
    /// If no config file exists, build entirely from env vars with defaults.
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
                    url: "ws://localhost:3000/ws".to_string(),
                    reconnect_interval_secs: 5,
                },
            }
        };

        // Env var overrides
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
    pub config: GatewayConfig,
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

    let registry = AgentRegistry::new();
    let router = MessageRouter::new();

    let state = Arc::new(GatewayState {
        registry,
        router,
        config: config.clone(),
    });

    // Connect to backend in background
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            tracing::info!("Connecting to backend: {}", state_clone.config.backend.url);
            if let Err(e) = connect_to_backend(&state_clone).await {
                tracing::error!("Backend connection error: {}. Reconnecting...", e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(
                state_clone.config.backend.reconnect_interval_secs,
            ))
            .await;
        }
    });

    let app = Router::new()
        .route("/ws", get(agent_ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state.clone());

    let addr = format!(
        "{}:{}",
        config.gateway.listen_addr, config.gateway.listen_port
    );
    tracing::info!(
        "Gateway {} ({}) listening on {}",
        config.gateway.id,
        config.gateway.zone,
        addr
    );
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
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

    // Register the connection for routing
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.router.add_agent_connection(conn_id, tx);

    // Forward backend messages to agent
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process agent messages
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let ws::Message::Text(text) = msg {
                // Try to parse as AgentMessage to extract agent_id
                if let Ok(agent_msg) =
                    serde_json::from_str::<appcontrol_common::AgentMessage>(&text)
                {
                    match &agent_msg {
                        appcontrol_common::AgentMessage::Register {
                            agent_id, hostname, ..
                        } => {
                            state_clone
                                .registry
                                .register(conn_id, *agent_id, hostname.clone());
                        }
                        appcontrol_common::AgentMessage::Heartbeat { agent_id: _, .. } => {
                            state_clone.registry.heartbeat(conn_id);
                        }
                        _ => {}
                    }
                }

                // Forward to backend
                state_clone.router.forward_to_backend(&text);
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.registry.unregister(conn_id);
    state.router.remove_agent_connection(conn_id);
    tracing::info!("Agent {} disconnected", conn_id);
}

async fn connect_to_backend(state: &Arc<GatewayState>) -> anyhow::Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;

    let (ws_stream, _) = connect_async(&state.config.backend.url).await?;
    let (mut write, mut read) = ws_stream.split();

    tracing::info!("Connected to backend");

    // Set up backend message forwarding
    let (backend_tx, mut backend_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.router.set_backend_sender(backend_tx);

    loop {
        tokio::select! {
            // Messages from agents to forward to backend
            Some(msg) = backend_rx.recv() => {
                write.send(tokio_tungstenite::tungstenite::Message::Text(msg)).await?;
            }
            // Messages from backend to forward to agents
            Some(ws_msg) = read.next() => {
                match ws_msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        // Route to appropriate agent
                        state.router.forward_to_agent(&text);
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
