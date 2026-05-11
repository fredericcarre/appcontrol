//! SQLite schema self-heal — runs after `run_migrations` to recover
//! installs where the `migrations/` folder on disk is out of date.
//!
//! Real-world cause: the Windows `appcontrol.ps1 upgrade` command pulls
//! a fresh binary but does not refresh the on-disk migrations folder.
//! A backend that previously ran against a v1.17.x migrations dir has
//! `_migrations` recording up to V050, so the new binary considers "no
//! new migrations to apply" while expecting columns introduced by V051
//! onwards (`components.check_native`, `cluster_members.*_native_override`,
//! the `manual_task_validations` table, etc.). The first SELECT against
//! `components` then 500s with `no column found for name: check_native`.
//!
//! This pass detects each expected column via `PRAGMA table_info(...)`
//! and adds it via `ALTER TABLE ADD COLUMN` when missing. SQLite's
//! `ADD COLUMN IF NOT EXISTS` is 3.35+ which we don't gate on, so the
//! presence-check is explicit. Both operations are no-ops on a fresh DB
//! that went through every migration cleanly.

use crate::db::DbPool;

pub async fn sqlite_schema_self_heal(pool: &DbPool) -> anyhow::Result<()> {
    async fn column_exists(pool: &DbPool, table: &str, column: &str) -> anyhow::Result<bool> {
        let sql = format!("PRAGMA table_info({table})");
        let rows: Vec<(i32, String, String, i32, Option<String>, i32)> =
            sqlx::query_as(&sql).fetch_all(pool).await?;
        Ok(rows.iter().any(|r| r.1 == column))
    }

    async fn ensure_column(
        pool: &DbPool,
        table: &str,
        column: &str,
        column_type: &str,
    ) -> anyhow::Result<()> {
        if column_exists(pool, table, column).await? {
            return Ok(());
        }
        tracing::warn!(
            "Schema self-heal: adding missing column {}.{} ({})",
            table,
            column,
            column_type
        );
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}");
        sqlx::query(&sql).execute(pool).await?;
        Ok(())
    }

    let components_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='components'",
    )
    .fetch_optional(pool)
    .await?;
    let applications_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='applications'",
    )
    .fetch_optional(pool)
    .await?;
    let cluster_members_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='cluster_members'",
    )
    .fetch_optional(pool)
    .await?;

    if applications_exists.is_some() {
        // V051
        ensure_column(pool, "applications", "map_display_options", "TEXT").await?;
    }

    if components_exists.is_some() {
        // V052
        ensure_column(pool, "components", "cluster_concurrency_mode", "TEXT").await?;
        ensure_column(pool, "components", "cluster_batch_size", "INTEGER").await?;
        // V053
        ensure_column(pool, "components", "check_native", "TEXT").await?;
        ensure_column(pool, "components", "start_native", "TEXT").await?;
        ensure_column(pool, "components", "stop_native", "TEXT").await?;
        // V054
        ensure_column(pool, "components", "manual_description", "TEXT").await?;
    }

    if cluster_members_exists.is_some() {
        // V055
        ensure_column(pool, "cluster_members", "check_native_override", "TEXT").await?;
        ensure_column(pool, "cluster_members", "start_native_override", "TEXT").await?;
        ensure_column(pool, "cluster_members", "stop_native_override", "TEXT").await?;
    }

    // V054 also adds a `manual_task_validations` table.
    let mt_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='manual_task_validations'",
    )
    .fetch_optional(pool)
    .await?;
    if mt_exists.is_none() && components_exists.is_some() {
        tracing::warn!("Schema self-heal: creating missing table manual_task_validations");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS manual_task_validations ( \
                id TEXT PRIMARY KEY, \
                component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE, \
                application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE, \
                started_at TEXT NOT NULL DEFAULT (datetime('now')), \
                started_by TEXT, \
                validated_at TEXT, \
                validated_by TEXT, \
                status TEXT NOT NULL DEFAULT 'pending' \
                    CHECK (status IN ('pending', 'validated', 'skipped', 'failed')), \
                comment TEXT, \
                duration_seconds INTEGER \
            )",
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_manual_task_validations_component \
                ON manual_task_validations (component_id, started_at DESC)",
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_manual_task_validations_pending \
                ON manual_task_validations (component_id) WHERE status = 'pending'",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
