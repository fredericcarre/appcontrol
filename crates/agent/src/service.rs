//! Windows Service support for the AppControl Agent.
//!
//! Allows the agent to run as a native Windows service, managed via `sc.exe`
//! or the Services MMC snap-in.
//!
//! ## Usage
//!
//! ```cmd
//! rem Install the service (run as Administrator)
//! appcontrol-agent.exe service install --config "C:\ProgramData\AppControl\config\agent.yaml"
//!
//! rem Start / stop / remove
//! sc start AppControlAgent
//! sc stop AppControlAgent
//! appcontrol-agent.exe service uninstall
//! ```

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::time::Duration;
#[cfg(windows)]
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

/// The Windows service name registered with SCM.
#[cfg(windows)]
pub const SERVICE_NAME: &str = "AppControlAgent";

/// The display name shown in Services MMC.
#[cfg(windows)]
pub const SERVICE_DISPLAY_NAME: &str = "AppControl Agent";

/// Install the agent as a Windows service.
///
/// Registers the service with the Windows Service Control Manager (SCM).
/// The service binary path includes the `service run` subcommand so that
/// when SCM starts the service, it enters the service dispatcher.
#[cfg(windows)]
pub fn install_service(config_path: &str) -> anyhow::Result<()> {
    use std::path::PathBuf;
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)?;

    let exe_path = std::env::current_exe()?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe_path.clone(),
        launch_arguments: vec![
            OsString::from("service"),
            OsString::from("run"),
            OsString::from("--config"),
            OsString::from(config_path),
        ],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let service = manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;

    // Set a description
    service.set_description(
        "AppControl distributed agent — monitors and manages application components",
    )?;

    println!("Service '{}' installed successfully.", SERVICE_NAME);
    println!("  Binary:  {}", exe_path.display());
    println!("  Config:  {}", config_path);
    println!();
    println!("Start the service with:  sc start {}", SERVICE_NAME);
    println!("Stop the service with:   sc stop {}", SERVICE_NAME);

    Ok(())
}

/// Uninstall (remove) the Windows service.
#[cfg(windows)]
pub fn uninstall_service() -> anyhow::Result<()> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

    let service =
        manager.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE)?;

    // Try to stop the service first (ignore errors if already stopped)
    let _ = service.stop();

    service.delete()?;

    println!("Service '{}' removed.", SERVICE_NAME);
    Ok(())
}

/// Entry point when the Windows SCM starts the service.
///
/// This function is called from `main()` when invoked with `service run`.
/// It dispatches to the Windows service framework which calls `service_main`.
#[cfg(windows)]
pub fn run_as_service() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| anyhow::anyhow!("Failed to start service dispatcher: {}", e))
}

// The Windows service framework requires a specific function signature.
#[cfg(windows)]
define_windows_service!(ffi_service_main, service_main);

/// The actual service main function called by the Windows SCM.
#[cfg(windows)]
fn service_main(arguments: Vec<OsString>) {
    if let Err(e) = run_service(arguments) {
        tracing::error!("Service failed: {}", e);
    }
}

#[cfg(windows)]
fn run_service(_arguments: Vec<OsString>) -> anyhow::Result<()> {
    // Channel to receive stop events
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    // Register the service control handler
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    // Report that we are starting
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    // Find config path from the service launch arguments.
    // The arguments come from the service registration (--config <path>).
    let config_path = std::env::args()
        .skip_while(|a| a != "--config")
        .nth(1)
        .unwrap_or_else(|| crate::config::default_config_path());

    // Build the tokio runtime inside the service
    let runtime = tokio::runtime::Runtime::new()?;

    // Report running
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    // Run the agent in the tokio runtime
    runtime.block_on(async {
        let config = match crate::config::AgentConfig::load(&config_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to load config: {}", e);
                return;
            }
        };

        let agent_id = config.agent_id();
        tracing::info!("Starting AppControl Agent {} (Windows Service)", agent_id);

        let buffer = match crate::buffer::OfflineBuffer::new(&config.buffer_path()) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Failed to init buffer: {}", e);
                return;
            }
        };

        let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();
        let check_scheduler = std::sync::Arc::new(crate::scheduler::CheckScheduler::new(
            agent_id,
            msg_tx.clone(),
        ));

        let gateway_urls = config.gateway_urls();
        let connection = crate::connection::ConnectionManager::new(
            gateway_urls,
            config.gateway.failover_strategy.clone(),
            config.gateway.primary_retry_secs,
            agent_id,
            config.labels.clone(),
            buffer.clone(),
            check_scheduler.clone(),
            msg_tx,
            config.tls.as_ref(),
        );

        let conn_handle = tokio::spawn(connection.run(msg_rx));
        let sched_handle = tokio::spawn(check_scheduler.run());

        // Wait for either the agent tasks to end or the service stop signal
        let stop_signal = tokio::task::spawn_blocking(move || {
            let _ = shutdown_rx.recv(); // blocks until stop
        });

        tokio::select! {
            _ = stop_signal => {
                tracing::info!("Service stop signal received");
            }
            r = conn_handle => {
                tracing::error!("Connection manager exited: {:?}", r);
            }
            r = sched_handle => {
                tracing::error!("Scheduler exited: {:?}", r);
            }
        }
    });

    // Report stopped
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}
