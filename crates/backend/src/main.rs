use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_backend::{config, create_router, db, middleware, websocket, AppState};

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

    let heartbeat_batcher =
        appcontrol_backend::core::heartbeat_batcher::HeartbeatBatcher::new();

    let state = Arc::new(AppState {
        db: pool,
        ws_hub,
        config,
        rate_limiter: middleware::rate_limit::RateLimitState::new(),
        heartbeat_batcher,
    });

    let app = create_router(state.clone());

    // Start heartbeat batcher flush loop (flushes every 5s)
    let batcher_state = state.clone();
    tokio::spawn(async move {
        batcher_state
            .heartbeat_batcher
            .run(batcher_state.db.clone())
            .await;
    });

    // Start heartbeat monitor background task (checks every 30s)
    let monitor_state = state.clone();
    tokio::spawn(async move {
        appcontrol_backend::core::heartbeat_monitor::run_heartbeat_monitor(
            monitor_state,
            std::time::Duration::from_secs(30),
        )
        .await;
    });

    // Rate limiter cleanup task (every 5 minutes)
    let rl_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            rl_state.rate_limiter.auth.cleanup();
            rl_state.rate_limiter.operations.cleanup();
            rl_state.rate_limiter.reads.cleanup();
        }
    });

    let addr = format!("0.0.0.0:{}", state.config.port);
    tracing::info!("Starting AppControl backend on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
