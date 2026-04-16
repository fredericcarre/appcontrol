//! Windows Service support for the AppControl Backend.
//!
//! The backend is configured entirely via environment variables (DATABASE_URL, etc.),
//! so there is no --config flag. Set env vars before installing the service, or use
//! the registry to configure service environment variables.

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
pub const SERVICE_NAME: &str = "AppControlBackend";
#[cfg(windows)]
const SERVICE_DISPLAY_NAME: &str = "AppControl Backend API";

#[cfg(windows)]
pub fn install_service() -> anyhow::Result<()> {
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
        launch_arguments: vec![OsString::from("service"), OsString::from("run")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;
    service.set_description(
        "AppControl Backend API — REST, WebSocket, PKI, and orchestration engine",
    )?;

    println!("Service '{}' installed successfully.", SERVICE_NAME);
    println!("  Binary: {}", exe_path.display());
    println!();
    println!("IMPORTANT: Set environment variables for the service before starting:");
    println!("  DATABASE_URL, JWT_SECRET, PORT, etc.");
    println!("  Use 'sc start {}' to start.", SERVICE_NAME);
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
        tracing::error!("Backend service failed: {}", e);
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
        wait_hint: Duration::from_secs(30),
        process_id: None,
    })?;

    // The backend is configured via env vars (DATABASE_URL, etc.)
    // These must be set as system environment variables or via the service registry.
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
        // Run the same logic as the normal main() but with shutdown via service signal
        let config = appcontrol_backend::config::AppConfig::from_env();
        let pool = match appcontrol_backend::db::create_pool(&config).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Database connection failed: {}", e);
                return;
            }
        };

        let ws_hub = appcontrol_backend::websocket::Hub::new();
        let heartbeat_batcher =
            appcontrol_backend::core::heartbeat_batcher::HeartbeatBatcher::new();
        let gateway_heartbeat_batcher =
            appcontrol_backend::core::heartbeat_batcher::GatewayHeartbeatBatcher::new();
        let latency_tracker = appcontrol_backend::core::latency_tracker::LatencyTracker::new();

        let operation_lock =
            appcontrol_backend::core::operation_lock::OperationLock::new(pool.clone());

        let state = std::sync::Arc::new(appcontrol_backend::AppState {
            app_repo: appcontrol_backend::repository::apps::create_app_repository(pool.clone()),
            component_repo: appcontrol_backend::repository::components::create_component_repository(
                pool.clone(),
            ),
            team_repo: appcontrol_backend::repository::teams::create_team_repository(pool.clone()),
            permission_repo:
                appcontrol_backend::repository::permissions::create_permission_repository(
                    pool.clone(),
                ),
            site_repo: appcontrol_backend::repository::sites::create_site_repository(pool.clone()),
            hosting_repo: appcontrol_backend::repository::hostings::create_hosting_repository(
                pool.clone(),
            ),
            enrollment_repo:
                appcontrol_backend::repository::enrollment::create_enrollment_repository(
                    pool.clone(),
                ),
            agent_repo: appcontrol_backend::repository::agents::create_agent_repository(
                pool.clone(),
            ),
            gateway_repo: appcontrol_backend::repository::gateways::create_gateway_repository(
                pool.clone(),
            ),
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            write_queue: appcontrol_backend::db::WriteQueue::new(pool.clone()),
            db: pool,
            ws_hub,
            config,
            rate_limiter: appcontrol_backend::middleware::rate_limit::RateLimitState::new(),
            heartbeat_batcher,
            gateway_heartbeat_batcher,
            latency_tracker,
            operation_lock,
            terminal_sessions: appcontrol_backend::terminal::TerminalSessionManager::new(),
            log_subscriptions: appcontrol_backend::websocket::LogSubscriptionManager::new(),
            pending_log_requests: appcontrol_backend::websocket::PendingLogRequests::new(),
            probe_results: dashmap::DashMap::new(),
        });

        let app = appcontrol_backend::create_router(state.clone());

        let addr = format!("0.0.0.0:{}", state.config.port);
        tracing::info!("Backend (Windows Service) listening on {}", addr);

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
