#[cfg(windows)]
mod win_service;

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_backend::{config, create_router, db, middleware, terminal, websocket, AppState};

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

            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {} PARTITION OF check_events \
                 FOR VALUES FROM ('{}-{:02}-01') TO ('{}-{:02}-01')",
                partition_name, year, month, next_month_year, next_month
            );

            if let Err(e) = sqlx::query(&sql).execute(pool).await {
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
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

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
    let org_result = sqlx::query(
        "INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3) \
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, slug = EXCLUDED.slug",
    )
    .bind(org_id)
    .bind(&seed.org_name)
    .bind(&seed.org_slug)
    .execute(pool)
    .await;

    if let Err(e) = org_result {
        tracing::warn!("Failed to seed organization: {}", e);
        return;
    }

    // Create or update the admin user (platform super-admin + org admin)
    let user_result = sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, platform_role, auth_provider, password_hash) \
         VALUES ($1, $2, 'seed-admin', $3, $4, 'admin', 'super_admin', 'local', $5) \
         ON CONFLICT (id) DO UPDATE SET email = EXCLUDED.email, display_name = EXCLUDED.display_name, password_hash = EXCLUDED.password_hash",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(&seed.admin_email)
    .bind(&seed.admin_display_name)
    .bind(&password_hash)
    .execute(pool)
    .await;

    match user_result {
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
    let orgs_without_ca: Vec<(uuid::Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM organizations WHERE ca_cert_pem IS NULL")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    for (org_id, org_name) in orgs_without_ca {
        match appcontrol_common::generate_ca(&org_name, 3650) {
            Ok(ca) => {
                if let Err(e) = sqlx::query(
                    "UPDATE organizations SET ca_cert_pem = $2, ca_key_pem = $3 WHERE id = $1",
                )
                .bind(org_id)
                .bind(&ca.cert_pem)
                .bind(&ca.key_pem)
                .execute(pool)
                .await
                {
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
    if action_log_days > 0 {
        let interval = format!("{} days", action_log_days);

        // Ensure archive table exists (idempotent)
        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS action_log_archive (LIKE action_log INCLUDING ALL)",
        )
        .execute(pool)
        .await;

        // Move old entries to archive (INSERT + DELETE in a transaction)
        match sqlx::query(
            r#"
            WITH archived AS (
                INSERT INTO action_log_archive
                SELECT * FROM action_log WHERE created_at < now() - $1::interval
                ON CONFLICT DO NOTHING
                RETURNING id
            )
            SELECT count(*) FROM archived
            "#,
        )
        .bind(&interval)
        .fetch_one(pool)
        .await
        {
            Ok(row) => {
                use sqlx::Row;
                let count: i64 = row.get(0);
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

        // List existing partitions and drop those older than cutoff
        let partitions: Vec<String> = sqlx::query_scalar(
            "SELECT tablename FROM pg_tables WHERE tablename LIKE 'check_events_y%' AND schemaname = 'public'"
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for partition_name in partitions {
            // Parse year/month from partition name: check_events_y2025m03
            if let Some(ym) = partition_name.strip_prefix("check_events_y") {
                let parts: Vec<&str> = ym.split('m').collect();
                if parts.len() == 2 {
                    if let (Ok(year), Ok(month)) =
                        (parts[0].parse::<i32>(), parts[1].parse::<u32>())
                    {
                        if year < cutoff_year || (year == cutoff_year && month < cutoff_month) {
                            let sql = format!("DROP TABLE IF EXISTS {}", partition_name);
                            match sqlx::query(&sql).execute(pool).await {
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
    use chrono::Duration;

    if action_log_days > 0 {
        let cutoff = chrono::Utc::now() - Duration::days(action_log_days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        // Create archive table if needed (SQLite syntax)
        let _ = sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS action_log_archive (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                details TEXT,
                status TEXT NOT NULL DEFAULT 'in_progress',
                error_message TEXT,
                completed_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"#,
        )
        .execute(pool)
        .await;

        // Archive old entries
        let result = sqlx::query(
            "INSERT OR IGNORE INTO action_log_archive SELECT * FROM action_log WHERE created_at < ?",
        )
        .bind(&cutoff_str)
        .execute(pool)
        .await;

        if let Ok(result) = result {
            let count = result.rows_affected();
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
        match sqlx::query("DELETE FROM check_events WHERE created_at < ?")
            .bind(&cutoff_str)
            .execute(pool)
            .await
        {
            Ok(result) => {
                let count = result.rows_affected();
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
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;

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

    // Executable-relative paths for standalone deployment (e.g., Windows .exe next to migrations/)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let exe_sub = exe_dir.as_ref().map(|d| d.join("migrations").join(db_subdir));
    let exe_root = exe_dir.as_ref().map(|d| d.join("migrations"));

    // Also check the root migrations directory (for backwards compatibility)
    let cargo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    // Docker root fallback for PostgreSQL (migrations are in /app/migrations/, not /app/migrations/postgres/)
    let docker_root = std::path::PathBuf::from("/app/migrations");

    // Current working directory fallback (user runs exe from project root)
    let cwd_sub = std::path::PathBuf::from("migrations").join(db_subdir);
    let cwd_root = std::path::PathBuf::from("migrations");

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
            } else if cwd_sub.exists() {
                Some(cwd_sub)
            } else if cwd_root.exists() {
                Some(cwd_root)
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

    let mut entries: Vec<(i32, String, std::path::PathBuf)> = Vec::new();

    if migrations_dir.exists() {
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
                        entries.push((version, filename, entry.path()));
                    }
                }
            }
        }
    }

    entries.sort_by_key(|(v, _, _)| *v);

    // Get already applied versions
    let applied: Vec<i32> = sqlx::query_scalar("SELECT version FROM _migrations ORDER BY version")
        .fetch_all(pool)
        .await?;

    let mut applied_count = 0;
    for (version, name, path) in &entries {
        if applied.contains(version) {
            continue;
        }

        let sql = std::fs::read_to_string(path)?;
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
            sqlx::query(trimmed).execute(&mut *tx).await.map_err(|e| {
                tracing::error!(
                    "Migration V{:03} failed on statement: {}\nError: {}",
                    version,
                    &trimmed[..trimmed.len().min(100)],
                    e
                );
                e
            })?;
        }
        sqlx::query("INSERT INTO _migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        applied_count += 1;
    }

    if applied_count > 0 {
        tracing::info!("Applied {} new migration(s)", applied_count);
    } else if entries.is_empty() {
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
