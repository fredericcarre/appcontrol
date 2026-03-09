//! Windows Service support for the AppControl Gateway.

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

#[cfg(windows)]
pub const SERVICE_NAME: &str = "AppControlGateway";
#[cfg(windows)]
const SERVICE_DISPLAY_NAME: &str = "AppControl Gateway";

#[cfg(windows)]
pub fn install_service(config_path: &str) -> anyhow::Result<()> {
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
        account_name: None,
        account_password: None,
    };

    let service = manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;
    service.set_description("AppControl Gateway — WebSocket relay between agents and backend")?;

    println!("Service '{}' installed successfully.", SERVICE_NAME);
    println!("  Binary: {}", exe_path.display());
    println!("  Config: {}", config_path);
    println!();
    println!("Start: sc start {}", SERVICE_NAME);
    println!("Stop:  sc stop {}", SERVICE_NAME);
    Ok(())
}

#[cfg(windows)]
pub fn uninstall_service() -> anyhow::Result<()> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service =
        manager.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE)?;
    let _ = service.stop();
    service.delete()?;
    println!("Service '{}' removed.", SERVICE_NAME);
    Ok(())
}

#[cfg(windows)]
pub fn run_as_service() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| anyhow::anyhow!("Failed to start service dispatcher: {}", e))
}

#[cfg(windows)]
define_windows_service!(ffi_service_main, service_main);

#[cfg(windows)]
fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!("Gateway service failed: {}", e);
    }
}

#[cfg(windows)]
fn run_service() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    let config_path = std::env::args()
        .skip_while(|a| a != "--config")
        .nth(1)
        .unwrap_or_else(super::default_config_path);

    let runtime = tokio::runtime::Runtime::new()?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    runtime.block_on(async {
        // Re-use the gateway's normal startup logic
        let config = match super::GatewayConfig::load(&config_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to load gateway config: {}", e);
                return;
            }
        };

        let gateway_id =
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, config.gateway.id.as_bytes());
        let state = std::sync::Arc::new(super::GatewayState {
            registry: super::AgentRegistry::new(),
            router: super::MessageRouter::new(),
            rate_limiter: super::AgentRateLimiter::new(),
            config: config.clone(),
            gateway_id,
            shutdown_flag: std::sync::atomic::AtomicBool::new(false),
            blocked_agents: std::sync::RwLock::new(std::collections::HashSet::new()),
        });

        // Backend connection loop
        let state_clone = state.clone();
        let backend_handle = tokio::spawn(async move {
            loop {
                tracing::info!("Connecting to backend: {}", state_clone.config.backend.url);
                if let Err(e) = super::connect_to_backend(&state_clone).await {
                    tracing::error!("Backend connection error: {}. Reconnecting...", e);
                }
                state_clone.router.clear_backend_sender();
                tokio::time::sleep(Duration::from_secs(
                    state_clone.config.backend.reconnect_interval_secs,
                ))
                .await;
            }
        });

        let app = axum::Router::new()
            .route("/ws", axum::routing::get(super::agent_ws_handler))
            .route("/health", axum::routing::get(super::health_handler))
            .route("/enroll", axum::routing::post(super::enroll_handler))
            .with_state(state.clone());

        let addr = format!(
            "{}:{}",
            config.gateway.listen_addr, config.gateway.listen_port
        );

        tracing::info!("Gateway (Windows Service) listening on {}", addr);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to bind {}: {}", addr, e);
                return;
            }
        };

        let server_handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("Server error: {}", e);
            }
        });

        let stop_signal = tokio::task::spawn_blocking(move || {
            let _ = shutdown_rx.recv();
        });

        tokio::select! {
            _ = stop_signal => { tracing::info!("Service stop signal received"); }
            _ = server_handle => { tracing::error!("Server exited unexpectedly"); }
            _ = backend_handle => { tracing::error!("Backend connection loop exited"); }
        }
    });

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
