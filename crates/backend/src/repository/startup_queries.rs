//! Query functions for startup, seeding, PKI auto-init, and data retention.
//!
//! These are used by main.rs for boot-time and maintenance tasks.

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid};
use uuid::Uuid;

// ============================================================================
// Seed queries
// ============================================================================

/// Count total users in the database.
pub async fn count_users(pool: &DbPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Upsert the default organization (ON CONFLICT by id).
pub async fn upsert_organization(
    pool: &DbPool,
    org_id: Uuid,
    name: &str,
    slug: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3) \
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, slug = EXCLUDED.slug",
    )
    .bind(DbUuid::from(org_id))
    .bind(name)
    .bind(slug)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert the admin user (ON CONFLICT by id).
pub async fn upsert_admin_user(
    pool: &DbPool,
    user_id: Uuid,
    org_id: Uuid,
    email: &str,
    display_name: &str,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, platform_role, auth_provider, password_hash) \
         VALUES ($1, $2, 'seed-admin', $3, $4, 'admin', 'super_admin', 'local', $5) \
         ON CONFLICT (id) DO UPDATE SET email = EXCLUDED.email, display_name = EXCLUDED.display_name, password_hash = EXCLUDED.password_hash",
    )
    .bind(DbUuid::from(user_id))
    .bind(DbUuid::from(org_id))
    .bind(email)
    .bind(display_name)
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// PKI auto-init queries
// ============================================================================

/// Find organizations without a CA certificate.
pub async fn find_orgs_without_ca(pool: &DbPool) -> Vec<(DbUuid, String)> {
    sqlx::query_as::<_, (DbUuid, String)>(
        "SELECT id, name FROM organizations WHERE ca_cert_pem IS NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Store auto-generated CA cert and key for an organization.
pub async fn store_ca_cert(
    pool: &DbPool,
    org_id: DbUuid,
    cert_pem: &str,
    key_pem: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE organizations SET ca_cert_pem = $2, ca_key_pem = $3 WHERE id = $1")
        .bind(org_id)
        .bind(cert_pem)
        .bind(key_pem)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Data retention queries (PostgreSQL)
// ============================================================================

/// Ensure action_log_archive table exists (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn ensure_action_log_archive_pg(pool: &DbPool) {
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS action_log_archive (LIKE action_log INCLUDING ALL)",
    )
    .execute(pool)
    .await;
}

/// Archive old action_log entries (PostgreSQL). Returns count of archived rows.
#[cfg(feature = "postgres")]
pub async fn archive_action_log_pg(
    pool: &DbPool,
    interval: &str,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
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
    .bind(interval)
    .fetch_one(pool)
    .await?;
    use sqlx::Row;
    Ok(row.get(0))
}

/// List check_events partition table names (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn list_check_event_partitions(pool: &DbPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE tablename LIKE 'check_events_y%' AND schemaname = 'public'",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Drop a check_events partition by name (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn drop_partition(pool: &DbPool, partition_name: &str) -> Result<(), sqlx::Error> {
    let sql = format!("DROP TABLE IF EXISTS {}", partition_name);
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

// ============================================================================
// Data retention queries (SQLite)
// ============================================================================

/// Ensure action_log_archive table exists (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn ensure_action_log_archive_sqlite(pool: &DbPool) {
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
}

/// Archive old action_log entries (SQLite). Returns count of archived rows.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn archive_action_log_sqlite(
    pool: &DbPool,
    cutoff_str: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO action_log_archive SELECT * FROM action_log WHERE created_at < ?",
    )
    .bind(cutoff_str)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Delete old check_events (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn delete_old_check_events_sqlite(
    pool: &DbPool,
    cutoff_str: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM check_events WHERE created_at < ?")
        .bind(cutoff_str)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Migration queries
// ============================================================================

/// Ensure _migrations tracking table exists (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn ensure_migrations_table_pg(pool: &DbPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Ensure _migrations tracking table exists (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn ensure_migrations_table_sqlite(pool: &DbPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Get list of already-applied migration versions.
pub async fn get_applied_migrations(pool: &DbPool) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT version FROM _migrations ORDER BY version")
        .fetch_all(pool)
        .await
}

/// Execute a single migration statement within a transaction (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn execute_migration_statement(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    statement: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(statement).execute(&mut **tx).await?;
    Ok(())
}

/// Execute a single migration statement within a transaction (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn execute_migration_statement(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    statement: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(statement).execute(&mut **tx).await?;
    Ok(())
}

/// Record that a migration version was applied (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn record_migration(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version: i32,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO _migrations (version, name) VALUES ($1, $2)")
        .bind(version)
        .bind(name)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Record that a migration version was applied (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn record_migration(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    version: i32,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO _migrations (version, name) VALUES ($1, $2)")
        .bind(version)
        .bind(name)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Create a check_events partition for a given year/month (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn create_check_event_partition(
    pool: &DbPool,
    partition_name: &str,
    year: i32,
    month: u32,
    next_year: i32,
    next_month: u32,
) -> Result<(), sqlx::Error> {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} PARTITION OF check_events \
         FOR VALUES FROM ('{}-{:02}-01') TO ('{}-{:02}-01')",
        partition_name, year, month, next_year, next_month
    );
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}
