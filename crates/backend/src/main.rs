#[cfg(windows)]
mod win_service;

/// SQLite migrations embedded at compile time for standalone deployment.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
mod embedded_sqlite {
    include!(concat!(env!("OUT_DIR"), "/embedded_sqlite_migrations.rs"));
}

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_backend::{
    config, create_router, db, middleware, repository, terminal, websocket, AppState,
};

#[derive(Parser)]
#[command(
    name = "appcontrol-backend",
    about = "AppControl Backend API",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_HASH"), " ", env!("BUILD_TIME"), ")")
)]
struct Args {
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
    Install,
    /// Remove the Windows service
    Uninstall,
    /// Run as a Windows service (called by SCM)
    Run,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Handle service subcommands (Windows only)
    if let Some(command) = args.command {
        return handle_service_command(command);
    }

    let config = config::AppConfig::from_env();

    // Structured logging: JSON in production, text in dev
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "appcontrol_backend=debug,tower_http=debug".into());

    if config.log_format == "json" {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let pool = db::create_pool(&config).await?;

    // Auto-run migrations on startup (Flyway-style V001__ naming)
    tracing::info!("Running database migrations...");
    run_migrations(&pool).await?;
    tracing::info!("Database migrations completed successfully");

    // Auto-create partitions for check_events (current + next year)
    // PostgreSQL only - SQLite doesn't support table partitioning
    #[cfg(feature = "postgres")]
    if let Err(e) = ensure_check_event_partitions(&pool).await {
        tracing::warn!("Failed to ensure check_event partitions: {}", e);
    }

    let ws_hub = websocket::Hub::new();

    let heartbeat_batcher = appcontrol_backend::core::heartbeat_batcher::HeartbeatBatcher::new();

    // Seed a default organization and admin user if none exist.
    // Controlled by SEED_ENABLED (default: true in dev, false in prod).
    // All values come from SEED_* environment variables.
    if config.seed.enabled {
        seed_initial_user(&pool, &config.seed).await;
    }

    // Auto-initialize PKI (CA) for all organizations that don't have one yet.
    // This eliminates the manual `POST /api/v1/pki/init` step.
    auto_init_pki(&pool).await;

    // Export PKI CA and gateway certificates to shared volume (for mTLS).
    // This runs if CERT_EXPORT_PATH is set (e.g., /certs in docker-compose).
    if let Err(e) =
        appcontrol_backend::api::pki_export::export_certs_to_volume_if_configured(&pool).await
    {
        tracing::warn!("Failed to export certificates to volume: {}", e);
    }

    // Install Prometheus metrics recorder
    let prometheus_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Register application metrics
    metrics::describe_counter!("http_requests_total", "Total HTTP requests");
    metrics::describe_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    );
    metrics::describe_gauge!("ws_connections_active", "Active WebSocket connections");
    metrics::describe_gauge!("agents_connected", "Number of connected agents");
    metrics::describe_counter!("state_transitions_total", "Total FSM state transitions");
    metrics::describe_counter!("commands_executed_total", "Total commands executed");
    metrics::describe_gauge!("db_pool_connections", "Database pool active connections");

    let shutdown_timeout_secs = config.shutdown_timeout_secs;
    let retention_action_log_days = config.retention_action_log_days;
    let retention_check_events_days = config.retention_check_events_days;

    let operation_lock = appcontrol_backend::core::operation_lock::OperationLock::new(pool.clone());

    // Cleanup stale operation locks at startup
    if let Err(e) = operation_lock.cleanup_all_stale_locks().await {
        tracing::warn!("Failed to cleanup stale operation locks at startup: {}", e);
    }

    let terminal_sessions = terminal::TerminalSessionManager::new();
    let log_subscriptions = websocket::LogSubscriptionManager::new();

    let state = Arc::new(AppState {
        app_repo: repository::apps::create_app_repository(pool.clone()),
        component_repo: repository::components::create_component_repository(pool.clone()),
        team_repo: repository::teams::create_team_repository(pool.clone()),
        permission_repo: repository::permissions::create_permission_repository(pool.clone()),
        site_repo: repository::sites::create_site_repository(pool.clone()),
        enrollment_repo: repository::enrollment::create_enrollment_repository(pool.clone()),
        agent_repo: repository::agents::create_agent_repository(pool.clone()),
        gateway_repo: repository::gateways::create_gateway_repository(pool.clone()),
        db: pool,
        ws_hub,
        config,
        rate_limiter: middleware::rate_limit::RateLimitState::new(),
        heartbeat_batcher,
        operation_lock,
        terminal_sessions,
        log_subscriptions,
        pending_log_requests: websocket::PendingLogRequests::new(),
    });

    // Store prometheus handle in a leaked box for the metrics handler
    let prom_handle: &'static metrics_exporter_prometheus::PrometheusHandle =
        Box::leak(Box::new(prometheus_handle));

    // Set the global prometheus handle for the health module
    appcontrol_backend::api::health::set_prometheus_handle(prom_handle);

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

    // Start stale operation lock cleanup task (every 30s)
    let lock_cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = lock_cleanup_state
                .operation_lock
                .cleanup_all_stale_locks()
                .await
            {
                tracing::warn!("Failed to cleanup stale operation locks: {}", e);
            }
        }
    });

    // Start snapshot scheduler background task (checks every 60s)
    let scheduler_state = state.clone();
    tokio::spawn(async move {
        appcontrol_backend::core::snapshot_scheduler::run_snapshot_scheduler(
            scheduler_state,
            std::time::Duration::from_secs(60),
        )
        .await;
    });

    // Start operation scheduler background task (checks every 60s)
    let op_scheduler_state = state.clone();
    tokio::spawn(async move {
        appcontrol_backend::core::operation_scheduler::run_operation_scheduler(
            op_scheduler_state,
            std::time::Duration::from_secs(60),
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
            // Also clean up PostgreSQL-backed counters and expired revocations
            middleware::rate_limit::cleanup_rate_limit_counters(&rl_state.db).await;
            middleware::auth::cleanup_expired_revocations(&rl_state.db).await;
        }
    });

    // Partition maintenance task (runs daily, creates partitions for current + next year)
    // PostgreSQL only - SQLite doesn't support table partitioning
    #[cfg(feature = "postgres")]
    {
        let partition_pool = state.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
            loop {
                interval.tick().await;
                if let Err(e) = ensure_check_event_partitions(&partition_pool).await {
                    tracing::warn!("Partition maintenance failed: {}", e);
                }
            }
        });
    }

    // Database pool metrics reporter (every 10s)
    db::spawn_pool_metrics(state.db.clone());

    // WebSocket connection gauge updater
    let ws_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            metrics::gauge!("ws_connections_active").set(ws_state.ws_hub.connection_count() as f64);
            metrics::gauge!("agents_connected").set(ws_state.ws_hub.routed_agent_count() as f64);
        }
    });

    // Data retention task (runs daily)
    let retention_pool = state.db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
        // Skip the immediate first tick
        interval.tick().await;
        loop {
            interval.tick().await;
            run_data_retention(
                &retention_pool,
                retention_action_log_days,
                retention_check_events_days,
            )
            .await;
        }
    });

    let addr = format!("0.0.0.0:{}", state.config.port);
    tracing::info!(
        "Starting AppControl backend v{} ({}) on {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        addr
    );
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Graceful shutdown on SIGTERM / Ctrl-C with configurable timeout
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_timeout_secs))
        .await?;

    tracing::info!("AppControl backend shut down gracefully");
    Ok(())
}

/// Wait for SIGTERM (container stop) or Ctrl-C (interactive), with a hard timeout.
async fn shutdown_signal(timeout_secs: u64) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received Ctrl-C, starting graceful shutdown..."); },
        _ = terminate => { tracing::info!("Received SIGTERM, starting graceful shutdown..."); },
    }

    // Give in-flight requests time to complete, then force exit
    tracing::info!(
        timeout_secs,
        "Waiting for in-flight requests to complete..."
    );
    tokio::time::sleep(std::time::Duration::from_secs(timeout_secs)).await;
    tracing::warn!("Shutdown timeout reached — forcing exit");
}

/// Ensure check_events partitions exist for the current and next year.
/// PostgreSQL only - SQLite doesn't support table partitioning.
#[cfg(feature = "postgres")]
async fn ensure_check_event_partitions(pool: &crate::db::DbPool) -> anyhow::Result<()> {
    let current_year = chrono::Utc::now().year();

    for year in [current_year, current_year + 1] {
        for month in 1..=12 {
            let partition_name = format!("check_events_y{}m{:02}", year, month);
            let next_month_year = if month == 12 { year + 1 } else { year };
            let next_month = if month == 12 { 1 } else { month + 1 };

            if let Err(e) = appcontrol_backend::repository::startup_queries::create_check_event_partition(
                pool, &partition_name, year, month, next_month_year, next_month,
            ).await {
                let err_str = e.to_string();
                // Ignore "already exists" errors (partition overlap)
                if !err_str.contains("already exists") && !err_str.contains("overlap") {
                    tracing::warn!("Failed to create partition {}: {}", partition_name, e);
                }
            }
        }
    }

    tracing::debug!(
        "Partition maintenance complete: ensured partitions for {} and {}",
        current_year,
        current_year + 1
    );
    Ok(())
}

#[cfg(feature = "postgres")]
use chrono::Datelike;

/// Seed a default organization and admin user on first start.
///
/// Only runs when SEED_ENABLED=true (default: true) and no users exist yet.
/// All values come from SEED_* environment variables — nothing is hardcoded.
///
/// Uses UPSERT to override any migration-seeded data with the configured values.
async fn seed_initial_user(pool: &crate::db::DbPool, seed: &config::SeedConfig) {
    use appcontrol_backend::repository::startup_queries as repo;

    let user_count = repo::count_users(pool).await;

    if user_count > 0 {
        tracing::debug!("Users already exist — skipping seed");
        return;
    }

    // Hash the admin password
    let password_hash = match bcrypt::hash(&seed.admin_password, bcrypt::DEFAULT_COST) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("Failed to hash admin password: {}", e);
            return;
        }
    };

    let org_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

    // Create or update the default organization
    if let Err(e) = repo::upsert_organization(pool, org_id, &seed.org_name, &seed.org_slug).await {
        tracing::warn!("Failed to seed organization: {}", e);
        return;
    }

    // Create or update the admin user (platform super-admin + org admin)
    match repo::upsert_admin_user(pool, user_id, org_id, &seed.admin_email, &seed.admin_display_name, &password_hash).await {
        Ok(_) => {
            tracing::info!(
                email = %seed.admin_email,
                org = %seed.org_name,
                "Seeded initial admin user (super_admin). \
                 Login with email/password at /api/v1/auth/login or the web UI."
            );
        }
        Err(e) => {
            tracing::warn!("Failed to seed admin user: {}", e);
        }
    }
}

/// Auto-initialize PKI (CA) for organizations that don't have one yet.
///
/// On first startup, there's typically one organization (created by migration or seed).
/// Without this, the admin must manually call `POST /api/v1/pki/init` before any
/// agent can enroll. This eliminates that manual step — zero-config mTLS.
async fn auto_init_pki(pool: &crate::db::DbPool) {
    use appcontrol_backend::repository::startup_queries as repo;

    let orgs_without_ca = repo::find_orgs_without_ca(pool).await;

    for (org_id, org_name) in orgs_without_ca {
        match appcontrol_common::generate_ca(&org_name, 3650) {
            Ok(ca) => {
                if let Err(e) = repo::store_ca_cert(pool, org_id, &ca.cert_pem, &ca.key_pem).await {
                    tracing::warn!(org = %org_name, "Failed to store auto-generated CA: {}", e);
                } else {
                    let fp = appcontrol_common::fingerprint_pem(&ca.cert_pem).unwrap_or_default();
                    tracing::info!(
                        org = %org_name,
                        fingerprint = %fp,
                        "Auto-initialized PKI (CA valid 10 years)"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(org = %org_name, "Failed to generate CA: {}", e);
            }
        }
    }
}

/// Run data retention policies.
///
/// action_log is APPEND-ONLY (Critical Rule #2): we archive old entries to
/// action_log_archive instead of deleting them. The archive table uses the same
/// schema and is cheap to query for auditors, while keeping the hot table small.
///
/// PostgreSQL: Uses partitions for check_events, archives action_log via CTE.
/// SQLite: Simple DELETE for check_events (no partitioning), same archive logic.
#[cfg(feature = "postgres")]
async fn run_data_retention(
    pool: &crate::db::DbPool,
    action_log_days: u32,
    check_events_days: u32,
) {
    use appcontrol_backend::repository::startup_queries as repo;

    if action_log_days > 0 {
        let interval = format!("{} days", action_log_days);

        // Ensure archive table exists (idempotent)
        repo::ensure_action_log_archive_pg(pool).await;

        // Move old entries to archive (INSERT + DELETE in a transaction)
        match repo::archive_action_log_pg(pool, &interval).await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(
                        archived = count,
                        retention_days = action_log_days,
                        "Action log: archived old entries to action_log_archive"
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Action log archival failed: {}", e);
            }
        }
    }

    if check_events_days > 0 {
        // Drop old partitions for check_events
        let cutoff = chrono::Utc::now() - chrono::Duration::days(check_events_days as i64);
        let cutoff_year = cutoff.year();
        let cutoff_month = cutoff.month();

        let partitions = repo::list_check_event_partitions(pool).await;

        for partition_name in partitions {
            // Parse year/month from partition name: check_events_y2025m03
            if let Some(ym) = partition_name.strip_prefix("check_events_y") {
                let parts: Vec<&str> = ym.split('m').collect();
                if parts.len() == 2 {
                    if let (Ok(year), Ok(month)) =
                        (parts[0].parse::<i32>(), parts[1].parse::<u32>())
                    {
                        if year < cutoff_year || (year == cutoff_year && month < cutoff_month) {
                            match repo::drop_partition(pool, &partition_name).await {
                                Ok(_) => {
                                    tracing::info!(
                                        partition = partition_name,
                                        "Dropped old check_events partition"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        partition = partition_name,
                                        "Failed to drop partition: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// SQLite version of data retention.
/// Uses simpler DELETE queries since SQLite doesn't support partitioning.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn run_data_retention(
    pool: &crate::db::DbPool,
    action_log_days: u32,
    check_events_days: u32,
) {
    use appcontrol_backend::repository::startup_queries as repo;
    use chrono::Duration;

    if action_log_days > 0 {
        let cutoff = chrono::Utc::now() - Duration::days(action_log_days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        // Create archive table if needed (SQLite syntax)
        repo::ensure_action_log_archive_sqlite(pool).await;

        // Archive old entries
        if let Ok(count) = repo::archive_action_log_sqlite(pool, &cutoff_str).await {
            if count > 0 {
                tracing::info!(
                    archived = count,
                    retention_days = action_log_days,
                    "Action log: archived old entries to action_log_archive"
                );
            }
        }
    }

    if check_events_days > 0 {
        let cutoff = chrono::Utc::now() - Duration::days(check_events_days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        // SQLite: simple DELETE (no partitioning)
        match repo::delete_old_check_events_sqlite(pool, &cutoff_str).await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(
                        deleted = count,
                        retention_days = check_events_days,
                        "Check events: deleted old entries"
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Check events cleanup failed: {}", e);
            }
        }
    }
}

/// Run migrations from the migrations/ directory.
/// Handles Flyway-style naming (V001__name.sql) by executing them in order.
/// Uses a `_migrations` tracking table to avoid re-running already-applied migrations.
///
/// For PostgreSQL: uses migrations/postgres/ directory
/// For SQLite: uses migrations/sqlite/ directory
async fn run_migrations(pool: &crate::db::DbPool) -> anyhow::Result<()> {
    // Determine the database type subdirectory
    #[cfg(feature = "postgres")]
    let db_subdir = "postgres";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let db_subdir = "sqlite";

    // Ensure tracking table exists (cross-database compatible syntax)
    #[cfg(feature = "postgres")]
    appcontrol_backend::repository::startup_queries::ensure_migrations_table_pg(pool).await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    appcontrol_backend::repository::startup_queries::ensure_migrations_table_sqlite(pool).await?;

    // Find migration files.
    // Try multiple locations in priority order:
    // 1. MIGRATIONS_DIR env var (custom deployments)
    // 2. CARGO_MANIFEST_DIR-relative (dev builds)
    // 3. Executable-relative (standalone Windows deployment)
    // 4. /app/migrations (Docker)
    // For dual-database support, look in the database-specific subdirectory.
    let cargo_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .join(db_subdir);
    let docker_dir = std::path::PathBuf::from("/app/migrations").join(db_subdir);
    let env_dir = std::env::var("MIGRATIONS_DIR")
        .ok()
        .map(|p| std::path::PathBuf::from(p).join(db_subdir));

    // Executable-relative paths for standalone deployment (e.g., Windows .exe in bin/ next to migrations/)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    // <exe_dir>/migrations/sqlite — exe sits next to migrations/
    let exe_sub = exe_dir
        .as_ref()
        .map(|d| d.join("migrations").join(db_subdir));
    let exe_root = exe_dir.as_ref().map(|d| d.join("migrations"));
    // <exe_dir>/../migrations/sqlite — exe is in bin/ subdirectory
    let exe_parent_sub = exe_dir
        .as_ref()
        .and_then(|d| d.parent().map(|p| p.join("migrations").join(db_subdir)));
    let exe_parent_root = exe_dir
        .as_ref()
        .and_then(|d| d.parent().map(|p| p.join("migrations")));

    // Also check the root migrations directory (for backwards compatibility)
    let cargo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    // Docker root fallback for PostgreSQL (migrations are in /app/migrations/, not /app/migrations/postgres/)
    let docker_root = std::path::PathBuf::from("/app/migrations");

    // Current working directory fallback (user runs exe from project root or bin/)
    let cwd_sub = std::path::PathBuf::from("migrations").join(db_subdir);
    let cwd_root = std::path::PathBuf::from("migrations");
    // CWD parent — user is in bin/ subdirectory
    let cwd_parent_sub = std::path::PathBuf::from("../migrations").join(db_subdir);
    let cwd_parent_root = std::path::PathBuf::from("../migrations");

    let migrations_dir = env_dir
        .filter(|p| p.exists())
        .or_else(|| {
            if cargo_dir.exists() {
                Some(cargo_dir.clone())
            } else if cargo_root.exists() {
                Some(cargo_root)
            } else if exe_sub.as_ref().is_some_and(|p| p.exists()) {
                exe_sub
            } else if exe_root.as_ref().is_some_and(|p| p.exists()) {
                exe_root
            } else if exe_parent_sub.as_ref().is_some_and(|p| p.exists()) {
                exe_parent_sub
            } else if exe_parent_root.as_ref().is_some_and(|p| p.exists()) {
                exe_parent_root
            } else if cwd_sub.exists() {
                Some(cwd_sub)
            } else if cwd_root.exists() {
                Some(cwd_root)
            } else if cwd_parent_sub.exists() {
                Some(cwd_parent_sub)
            } else if cwd_parent_root.exists() {
                Some(cwd_parent_root)
            } else if docker_dir.exists() {
                Some(docker_dir.clone())
            } else if docker_root.exists() {
                Some(docker_root)
            } else {
                None
            }
        })
        .unwrap_or(docker_dir);

    tracing::debug!(
        "Migration path resolution: dir={}, exists={}",
        migrations_dir.display(),
        migrations_dir.exists()
    );

    if !migrations_dir.exists() {
        // Log all paths tried to help diagnose deployment issues
        tracing::warn!(
            "No migration files found in {} — check MIGRATIONS_DIR or ensure migrations are present. \
             Paths tried: CARGO_MANIFEST_DIR={}, exe_dir={:?}, exe_parent={:?}, cwd=migrations/{}, cwd_parent=../migrations/{}",
            migrations_dir.display(),
            cargo_dir.display(),
            exe_dir.as_ref().map(|d| d.join("migrations").join(db_subdir)),
            exe_dir.as_ref().and_then(|d| d.parent().map(|p| p.join("migrations").join(db_subdir))),
            db_subdir,
            db_subdir,
        );
    }

    // Collect migrations — either from filesystem or embedded in binary.
    // Each entry is (version, name, sql_content).
    let mut migration_entries: Vec<(i32, String, String)> = Vec::new();

    if migrations_dir.exists() {
        let mut file_entries: Vec<(i32, String, std::path::PathBuf)> = Vec::new();
        for entry in std::fs::read_dir(&migrations_dir)? {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.ends_with(".sql") && filename.starts_with('V') {
                // Parse version from "V001__name.sql"
                if let Some(version_str) = filename
                    .strip_prefix('V')
                    .and_then(|s| s.split("__").next())
                {
                    if let Ok(version) = version_str.parse::<i32>() {
                        file_entries.push((version, filename, entry.path()));
                    }
                }
            }
        }
        file_entries.sort_by_key(|(v, _, _)| *v);
        for (version, name, path) in &file_entries {
            let sql = std::fs::read_to_string(path)?;
            migration_entries.push((*version, name.clone(), sql));
        }
    }

    // SQLite: if no files found on disk, use migrations embedded in the binary.
    // This makes the standalone Windows .exe fully self-contained.
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    if migration_entries.is_empty() {
        tracing::info!(
            "Using {} embedded SQLite migrations (no files found on disk)",
            embedded_sqlite::MIGRATIONS.len()
        );
        for &(version, name, sql) in embedded_sqlite::MIGRATIONS {
            migration_entries.push((version, name.to_string(), sql.to_string()));
        }
    }

    migration_entries.sort_by_key(|(v, _, _)| *v);

    // Get already applied versions
    let applied = appcontrol_backend::repository::startup_queries::get_applied_migrations(pool).await?;

    let mut applied_count = 0;
    for (version, name, sql) in &migration_entries {
        if applied.contains(version) {
            continue;
        }

        tracing::info!("Applying migration V{:03}: {}", version, name);

        // Execute migration in a transaction.
        // Migration files contain multiple SQL statements separated by semicolons.
        // sqlx::query() uses the extended protocol which only supports one statement,
        // so we split on semicolons and execute each statement individually.
        let mut tx = pool.begin().await?;
        for statement in sql.split(';') {
            // Strip comment-only lines before checking if the statement is empty.
            // After splitting on ';', a chunk may start with "-- comment\nCREATE TABLE..."
            // and we must not skip the whole chunk just because it starts with "--".
            let stripped: String = statement
                .lines()
                .filter(|line| !line.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            let trimmed = stripped.trim();
            if trimmed.is_empty() {
                continue;
            }
            appcontrol_backend::repository::startup_queries::execute_migration_statement(&mut tx, trimmed).await.map_err(|e| {
                tracing::error!(
                    "Migration V{:03} failed on statement: {}\nError: {}",
                    version,
                    &trimmed[..trimmed.len().min(100)],
                    e
                );
                e
            })?;
        }
        appcontrol_backend::repository::startup_queries::record_migration(&mut tx, *version, name).await?;
        tx.commit().await?;

        applied_count += 1;
    }

    if applied_count > 0 {
        tracing::info!("Applied {} new migration(s)", applied_count);
    } else if migration_entries.is_empty() {
        tracing::warn!(
            "No migration files found in {} - check MIGRATIONS_DIR or ensure migrations are present",
            migrations_dir.display()
        );
    } else {
        tracing::info!("Database is up to date (no new migrations)");
    }

    Ok(())
}

#[allow(unreachable_code)]
fn handle_service_command(command: ServiceCommand) -> anyhow::Result<()> {
    match command {
        ServiceCommand::Service { action } => match action {
            ServiceAction::Install => {
                #[cfg(windows)]
                {
                    win_service::install_service()?;
                    return Ok(());
                }
                #[cfg(not(windows))]
                {
                    anyhow::bail!(
                        "Windows service commands are only available on Windows.\n\
                         On Linux, use systemd: systemctl enable/start appcontrol-backend"
                    );
                }
            }
            ServiceAction::Uninstall => {
                #[cfg(windows)]
                {
                    win_service::uninstall_service()?;
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
                    win_service::run_as_service()?;
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
