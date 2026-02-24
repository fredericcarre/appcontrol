mod buffer;
mod config;
mod connection;
mod enroll;
mod executor;
mod native_commands;
mod platform;
mod scheduler;
mod self_update;
#[cfg(windows)]
mod service;
mod tls;

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "appcontrol-agent", about = "AppControl Agent")]
struct Args {
    /// Path to configuration file (default: platform-specific)
    #[arg(short, long, default_value_t = config::default_config_path(), global = true)]
    config: String,

    /// Override agent ID
    #[arg(long)]
    agent_id: Option<String>,

    /// Enroll this agent with a gateway (provide gateway URL, e.g., https://gateway:4443)
    #[arg(long)]
    enroll: Option<String>,

    /// Enrollment token (required with --enroll)
    #[arg(long)]
    token: Option<String>,

    /// Directory to write enrollment certs and config
    #[arg(long, default_value_t = config::default_config_dir())]
    enroll_dir: String,

    /// Windows service management commands
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
        /// Path to agent configuration file
        #[arg(short, long, default_value_t = config::default_config_path())]
        config: String,
    },
    /// Remove the Windows service
    Uninstall,
    /// Run as a Windows service (called by SCM, not by user)
    Run,
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

    // Handle service subcommands (Windows only)
    if let Some(command) = args.command {
        return handle_service_command(command);
    }

    // Enrollment mode: get certs from gateway and exit
    if let Some(ref gateway_url) = args.enroll {
        let token = args
            .token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--token is required with --enroll"))?;
        return enroll::enroll(gateway_url, token, &args.enroll_dir).await;
    }

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
    let advisory = config.is_advisory();
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
        advisory,
    );

    if advisory {
        tracing::warn!(
            "Agent running in ADVISORY mode — health checks active, command execution disabled"
        );
    }

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

    // Platform-specific signal handling for configuration reload
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
                reload_config(&config_path);
            }
        });
    }

    // Windows: Ctrl+C / shutdown signal handling (foreground mode only, not service)
    #[cfg(windows)]
    {
        tokio::spawn(async {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::error!("Failed to listen for Ctrl+C: {}", e);
                return;
            }
            tracing::info!("Received Ctrl+C — shutting down gracefully");
            std::process::exit(0);
        });
    }

    tokio::select! {
        r = conn_handle => { tracing::error!("Connection manager exited: {:?}", r); }
        r = sched_handle => { tracing::error!("Scheduler exited: {:?}", r); }
    }

    Ok(())
}

fn reload_config(config_path: &str) {
    match config::AgentConfig::load(config_path) {
        Ok(new_config) => {
            tracing::info!("Configuration reloaded successfully");
            if let Ok(_filter) = tracing_subscriber::EnvFilter::try_new(new_config.log_level()) {
                tracing::info!("Updated log filter: {}", new_config.log_level());
            }
        }
        Err(e) => {
            tracing::error!("Failed to reload configuration: {}", e);
        }
    }
}

#[allow(unreachable_code)]
fn handle_service_command(command: ServiceCommand) -> anyhow::Result<()> {
    match command {
        ServiceCommand::Service { action } => match action {
            ServiceAction::Install { config } => {
                #[cfg(windows)]
                {
                    service::install_service(&config)?;
                    return Ok(());
                }
                #[cfg(not(windows))]
                {
                    let _ = config;
                    anyhow::bail!(
                        "Windows service commands are only available on Windows.\n\
                         On Linux, use systemd: systemctl enable/start appcontrol-agent"
                    );
                }
            }
            ServiceAction::Uninstall => {
                #[cfg(windows)]
                {
                    service::uninstall_service()?;
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
                    service::run_as_service()?;
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
