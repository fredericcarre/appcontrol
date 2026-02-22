use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use appcontrol_backend::{config, create_router, db, middleware, websocket, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let pool = db::create_pool(&config.database_url).await?;

    // Auto-run migrations on startup (Flyway-style V001__ naming)
    tracing::info!("Running database migrations...");
    run_migrations(&pool).await?;
    tracing::info!("Database migrations completed successfully");

    // Auto-create partitions for check_events (current + next year)
    if let Err(e) = ensure_check_event_partitions(&pool).await {
        tracing::warn!("Failed to ensure check_event partitions: {}", e);
    }

    let ws_hub = websocket::Hub::new();

    let heartbeat_batcher =
        appcontrol_backend::core::heartbeat_batcher::HeartbeatBatcher::new();

    // Connect to Redis if configured
    let redis = if let Some(ref redis_url) = config.redis_url {
        match redis::Client::open(redis_url.as_str()) {
            Ok(client) => match redis::aio::ConnectionManager::new(client).await {
                Ok(conn) => {
                    tracing::info!("Connected to Redis");
                    Some(conn)
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to Redis: {} — continuing without cache", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Invalid REDIS_URL: {} — continuing without cache", e);
                None
            }
        }
    } else {
        tracing::info!("REDIS_URL not set — running without Redis cache");
        None
    };

    // Install Prometheus metrics recorder
    let prometheus_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Register application metrics
    metrics::describe_counter!("http_requests_total", "Total HTTP requests");
    metrics::describe_histogram!("http_request_duration_seconds", "HTTP request duration in seconds");
    metrics::describe_gauge!("ws_connections_active", "Active WebSocket connections");
    metrics::describe_gauge!("agents_connected", "Number of connected agents");
    metrics::describe_counter!("state_transitions_total", "Total FSM state transitions");
    metrics::describe_counter!("commands_executed_total", "Total commands executed");
    metrics::describe_gauge!("db_pool_connections", "Database pool active connections");

    let state = Arc::new(AppState {
        db: pool,
        ws_hub,
        config,
        rate_limiter: middleware::rate_limit::RateLimitState::new(),
        heartbeat_batcher,
        redis,
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

    // Partition maintenance task (runs daily, creates partitions for current + next year)
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

    let addr = format!("0.0.0.0:{}", state.config.port);
    tracing::info!("Starting AppControl backend on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Graceful shutdown on SIGTERM / Ctrl-C
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("AppControl backend shut down gracefully");
    Ok(())
}

/// Wait for SIGTERM (container stop) or Ctrl-C (interactive).
async fn shutdown_signal() {
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
}

/// Ensure check_events partitions exist for the current and next year.
async fn ensure_check_event_partitions(pool: &sqlx::PgPool) -> anyhow::Result<()> {
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

use chrono::Datelike;

/// Run migrations from the migrations/ directory.
/// Handles Flyway-style naming (V001__name.sql) by executing them in order.
/// Uses a `_migrations` tracking table to avoid re-running already-applied migrations.
async fn run_migrations(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    // Ensure tracking table exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;

    // Find migration files
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations");

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

        // Execute migration in a transaction
        let mut tx = pool.begin().await?;
        sqlx::query(&sql).execute(&mut *tx).await.map_err(|e| {
            tracing::error!("Migration V{:03} failed: {}", version, e);
            e
        })?;
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
    } else {
        tracing::info!("Database is up to date (no new migrations)");
    }

    Ok(())
}
