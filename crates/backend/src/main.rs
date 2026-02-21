use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_backend::{config, create_router, db, websocket, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appcontrol_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::AppConfig::from_env();
    let pool = db::create_pool(&config.database_url).await?;
    let ws_hub = websocket::Hub::new();

    let state = Arc::new(AppState {
        db: pool,
        ws_hub,
        config,
    });

    let app = create_router(state.clone());

    // Start heartbeat monitor background task (checks every 30s)
    let monitor_state = state.clone();
    tokio::spawn(async move {
        appcontrol_backend::core::heartbeat_monitor::run_heartbeat_monitor(
            monitor_state,
            std::time::Duration::from_secs(30),
        )
        .await;
    });

    let addr = format!("0.0.0.0:{}", state.config.port);
    tracing::info!("Starting AppControl backend on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
