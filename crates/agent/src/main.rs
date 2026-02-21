mod buffer;
mod config;
mod connection;
mod executor;
mod native_commands;
mod platform;
mod scheduler;

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

    // Initialize connection manager
    let connection = connection::ConnectionManager::new(
        config.gateway_url().to_string(),
        agent_id,
        config.labels.clone(),
        buffer.clone(),
        check_scheduler.clone(),
        msg_tx,
    );

    // Start all subsystems
    let conn_handle = tokio::spawn(connection.run(msg_rx));
    let sched_handle = tokio::spawn(check_scheduler.run());

    tokio::select! {
        r = conn_handle => { tracing::error!("Connection manager exited: {:?}", r); }
        r = sched_handle => { tracing::error!("Scheduler exited: {:?}", r); }
    }

    Ok(())
}
