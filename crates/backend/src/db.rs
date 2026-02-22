use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

use crate::config::AppConfig;

/// Create a PostgreSQL connection pool with configurable parameters.
pub async fn create_pool(config: &AppConfig) -> Result<PgPool, sqlx::Error> {
    tracing::info!(
        max_connections = config.db_pool_size,
        idle_timeout_secs = config.db_idle_timeout_secs,
        connect_timeout_secs = config.db_connect_timeout_secs,
        "Creating database connection pool"
    );

    PgPoolOptions::new()
        .max_connections(config.db_pool_size)
        .idle_timeout(Some(Duration::from_secs(config.db_idle_timeout_secs)))
        .acquire_timeout(Duration::from_secs(config.db_connect_timeout_secs))
        .max_lifetime(Some(Duration::from_secs(1800))) // 30 min max lifetime
        .connect(&config.database_url)
        .await
}

/// Spawn a background task that periodically reports pool metrics to Prometheus.
pub fn spawn_pool_metrics(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let idle = pool.num_idle() as f64;
            let total = pool.size() as f64;
            let active = total - idle;

            metrics::gauge!("db_pool_connections", "state" => "idle").set(idle);
            metrics::gauge!("db_pool_connections", "state" => "active").set(active);
            metrics::gauge!("db_pool_connections", "state" => "total").set(total);
        }
    });
}
