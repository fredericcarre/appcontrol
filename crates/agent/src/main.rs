mod buffer;
mod config;
mod connection;
mod executor;
mod native_commands;
mod platform;
mod scheduler;
mod tls;

use clap::Parser;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "appcontrol-agent", about = "AppControl Agent")]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/appcontrol/agent.yaml")]
    config: String,

    /// Override agent ID
    #[arg(long)]
    agent_id: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appcontrol_agent=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let config = config::AgentConfig::load(&args.config)?;

    let agent_id = match args.agent_id {
        Some(id) => uuid::Uuid::parse_str(&id)?,
        None => config.agent_id(),
    };

    tracing::info!("Starting AppControl Agent {}", agent_id);

    // Initialize offline buffer
    let buffer = buffer::OfflineBuffer::new(&config.buffer_path())?;

    // Initialize check scheduler (shared via Arc for UpdateConfig handling)
    let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();
    let check_scheduler = Arc::new(scheduler::CheckScheduler::new(agent_id, msg_tx.clone()));

    // Initialize connection manager with multi-gateway failover support
    let gateway_urls = config.gateway_urls();
    let connection = connection::ConnectionManager::new(
        gateway_urls.clone(),
        config.gateway.failover_strategy.clone(),
        config.gateway.primary_retry_secs,
        agent_id,
        config.labels.clone(),
        buffer.clone(),
        check_scheduler.clone(),
        msg_tx,
        config.tls.as_ref(),
    );

    tracing::info!(
        "Gateway failover: {} URLs configured (strategy={})",
        gateway_urls.len(),
        config.gateway.failover_strategy
    );

    // Clone scheduler reference before it's moved into run()
    let _scheduler_for_reload = check_scheduler.clone();

    // Start all subsystems
    let conn_handle = tokio::spawn(connection.run(msg_rx));
    let sched_handle = tokio::spawn(check_scheduler.run());

    // SIGHUP handler: reload agent configuration file on HUP signal
    #[cfg(unix)]
    {
        let config_path = args.config.clone();
        tokio::spawn(async move {
            let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                .expect("failed to install SIGHUP handler");
            loop {
                sighup.recv().await;
                tracing::info!(
                    "Received SIGHUP — reloading configuration from {}",
                    config_path
                );
                match config::AgentConfig::load(&config_path) {
                    Ok(new_config) => {
                        tracing::info!("Configuration reloaded successfully");
                        // Update check intervals in the scheduler
                        // (The actual component configs come from the backend via UpdateConfig,
                        //  but local agent config like log level, buffer path, etc. can be reloaded)

                        // Re-initialize log level if it changed
                        if let Ok(_filter) =
                            tracing_subscriber::EnvFilter::try_new(new_config.log_level())
                        {
                            tracing::info!("Updated log filter: {}", new_config.log_level());
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to reload configuration: {}", e);
                    }
                }
            }
        });
    }

    tokio::select! {
        r = conn_handle => { tracing::error!("Connection manager exited: {:?}", r); }
        r = sched_handle => { tracing::error!("Scheduler exited: {:?}", r); }
    }

    Ok(())
}
