//! Database abstraction layer for dual PostgreSQL/SQLite support.
//!
//! This module provides database pool creation and utilities that work
//! with both PostgreSQL and SQLite via sqlx's Any driver, enabling
//! runtime database selection for portable Windows deployment.

use std::time::Duration;

use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ══════════════════════════════════════════════════════════════════════════════
// DbUuid — cross-database UUID type
// ══════════════════════════════════════════════════════════════════════════════
//
// sqlx for SQLite encodes Uuid as BLOB (16 bytes), but our SQLite migrations
// define UUID columns as TEXT (human-readable strings). This causes a mismatch:
// - Encode sends blob → column stores blob (or text if bound as string)
// - Decode calls from_slice(blob) → fails on TEXT data (36 bytes != 16)
//
// DbUuid solves this by always encoding/decoding as TEXT for SQLite,
// while remaining transparent to Uuid for PostgreSQL.

/// A UUID type that works correctly with both PostgreSQL and SQLite.
///
/// For PostgreSQL: transparent wrapper around `uuid::Uuid`.
/// For SQLite: encodes as TEXT (hyphenated string), decodes via `Uuid::parse_str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DbUuid(pub Uuid);

impl DbUuid {
    pub fn new_v4() -> Self {
        DbUuid(Uuid::new_v4())
    }

    pub fn nil() -> Self {
        DbUuid(Uuid::nil())
    }

    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for DbUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for DbUuid {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Uuid> for DbUuid {
    fn from(u: Uuid) -> Self {
        DbUuid(u)
    }
}

impl From<DbUuid> for Uuid {
    fn from(u: DbUuid) -> Self {
        u.0
    }
}

impl std::str::FromStr for DbUuid {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DbUuid(Uuid::parse_str(s)?))
    }
}

// PostgreSQL: transparent to Uuid
#[cfg(feature = "postgres")]
impl<'r> sqlx::Decode<'r, sqlx::Postgres> for DbUuid {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let uuid = <Uuid as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(DbUuid(uuid))
    }
}

#[cfg(feature = "postgres")]
impl sqlx::Type<sqlx::Postgres> for DbUuid {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <Uuid as sqlx::Type<sqlx::Postgres>>::type_info()
    }
    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <Uuid as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

#[cfg(feature = "postgres")]
impl<'q> sqlx::Encode<'q, sqlx::Postgres> for DbUuid {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <Uuid as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.0, buf)
    }
}

// SQLite: encode/decode as TEXT (hyphenated UUID string)
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for DbUuid {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text: &str = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        let uuid = Uuid::parse_str(text)?;
        Ok(DbUuid(uuid))
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl sqlx::Type<sqlx::Sqlite> for DbUuid {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
    fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for DbUuid {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> sqlx::encode::IsNull {
        let text = self.0.to_string();
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(text, args)
    }
}

/// Type alias for the database pool.
/// We use PgPool for PostgreSQL-specific features but can switch to AnyPool
/// for portable deployment.
#[cfg(feature = "postgres")]
pub type DbPool = sqlx::PgPool;

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type DbPool = sqlx::SqlitePool;

/// Create a database connection pool based on configuration.
///
/// For PostgreSQL:
/// - Uses PgPoolOptions with configurable max connections, timeouts
///
/// For SQLite:
/// - Uses SqlitePoolOptions with WAL mode for better concurrency
/// - Creates the database file if it doesn't exist
pub async fn create_pool(config: &AppConfig) -> Result<DbPool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        tracing::info!(
            max_connections = config.db_pool_size,
            idle_timeout_secs = config.db_idle_timeout_secs,
            connect_timeout_secs = config.db_connect_timeout_secs,
            "Creating PostgreSQL connection pool"
        );

        sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.db_pool_size)
            .idle_timeout(Some(Duration::from_secs(config.db_idle_timeout_secs)))
            .acquire_timeout(Duration::from_secs(config.db_connect_timeout_secs))
            .max_lifetime(Some(Duration::from_secs(1800))) // 30 min max lifetime
            .connect(&config.database_url)
            .await
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        tracing::info!(
            url = %config.database_url,
            "Creating SQLite connection pool with WAL mode"
        );

        // Extract path from sqlite:path URL format
        let path = config
            .database_url
            .strip_prefix("sqlite:")
            .unwrap_or(&config.database_url);

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    sqlx::Error::Configuration(
                        format!("Failed to create database directory: {}", e).into(),
                    )
                })?;
            }
        }

        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(config.db_pool_size.min(16)) // SQLite doesn't benefit from many connections
            .idle_timeout(Some(Duration::from_secs(config.db_idle_timeout_secs)))
            .acquire_timeout(Duration::from_secs(config.db_connect_timeout_secs))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    use sqlx::Executor;
                    // Enable WAL mode for better concurrency
                    conn.execute("PRAGMA journal_mode=WAL").await?;
                    // Busy timeout for handling concurrent access
                    conn.execute("PRAGMA busy_timeout=30000").await?;
                    // Foreign keys enforcement (off by default in SQLite)
                    conn.execute("PRAGMA foreign_keys=ON").await?;
                    // Synchronous mode for durability (NORMAL is good balance)
                    conn.execute("PRAGMA synchronous=NORMAL").await?;
                    Ok(())
                })
            })
            .connect_with(
                config
                    .database_url
                    .parse::<sqlx::sqlite::SqliteConnectOptions>()?
                    .create_if_missing(true),
            )
            .await
    }
}

/// Check if the current database is PostgreSQL
#[cfg(feature = "postgres")]
pub fn is_postgres() -> bool {
    true
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub fn is_postgres() -> bool {
    false
}

/// Check if the current database is SQLite
#[cfg(feature = "postgres")]
pub fn is_sqlite() -> bool {
    false
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub fn is_sqlite() -> bool {
    true
}

/// Spawn a background task that periodically reports pool metrics to Prometheus.
pub fn spawn_pool_metrics(pool: DbPool) {
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

            // Add database type label
            let db_type = if is_postgres() { "postgres" } else { "sqlite" };
            metrics::gauge!("db_type_info", "type" => db_type).set(1.0);
        }
    });
}

/// SQL dialect helper for queries that differ between PostgreSQL and SQLite.
///
/// This module provides functions that return the correct SQL syntax
/// for the current database type at compile time.
pub mod sql {
    /// Returns the placeholder syntax for the given parameter index (1-based).
    /// PostgreSQL: $1, $2, $3...
    /// SQLite: ?1, ?2, ?3... (but sqlx handles this automatically)
    #[inline]
    pub fn placeholder(_n: usize) -> &'static str {
        // sqlx normalizes placeholders, so we always use $N syntax
        // The driver handles translation
        ""
    }

    /// Returns the SQL for getting the current timestamp.
    /// PostgreSQL: now()
    /// SQLite: datetime('now')
    #[cfg(feature = "postgres")]
    #[inline]
    pub const fn now() -> &'static str {
        "now()"
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[inline]
    pub const fn now() -> &'static str {
        "datetime('now')"
    }

    /// Returns the SQL for generating a random UUID.
    /// PostgreSQL: gen_random_uuid()
    /// SQLite: Application must generate UUID (returns placeholder comment)
    #[cfg(feature = "postgres")]
    #[inline]
    pub const fn gen_uuid() -> &'static str {
        "gen_random_uuid()"
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[inline]
    pub const fn gen_uuid() -> &'static str {
        // SQLite doesn't have gen_random_uuid() - caller must provide UUID
        "NULL" // This will cause an error if used - intentional
    }

    /// Returns the SQL for JSON field access.
    /// PostgreSQL: column->>'field'
    /// SQLite: json_extract(column, '$.field')
    #[cfg(feature = "postgres")]
    pub fn json_extract(column: &str, field: &str) -> String {
        format!("{}->>'{}'", column, field)
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub fn json_extract(column: &str, field: &str) -> String {
        format!("json_extract({}, '$.{}')", column, field)
    }

    /// Returns the SQL for checking if an array contains a value.
    /// PostgreSQL: value = ANY(array_column)
    /// SQLite: Requires different approach (IN with subquery)
    #[cfg(feature = "postgres")]
    pub fn array_contains(value_expr: &str, array_column: &str) -> String {
        format!("{} = ANY({})", value_expr, array_column)
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub fn array_contains(value_expr: &str, array_column: &str) -> String {
        // SQLite doesn't have native arrays - this assumes JSON array stored as text
        format!(
            "EXISTS(SELECT 1 FROM json_each({}) WHERE value = {})",
            array_column, value_expr
        )
    }

    /// Returns the SQL for FILTER clause (aggregate with condition).
    /// PostgreSQL: COUNT(*) FILTER (WHERE condition)
    /// SQLite: SUM(CASE WHEN condition THEN 1 ELSE 0 END)
    #[cfg(feature = "postgres")]
    pub fn count_filter(condition: &str) -> String {
        format!("COUNT(*) FILTER (WHERE {})", condition)
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub fn count_filter(condition: &str) -> String {
        format!("SUM(CASE WHEN {} THEN 1 ELSE 0 END)", condition)
    }

    /// Returns the SQL for case-insensitive LIKE.
    /// PostgreSQL: ILIKE
    /// SQLite: LIKE (case-insensitive by default for ASCII)
    #[cfg(feature = "postgres")]
    #[inline]
    pub const fn ilike() -> &'static str {
        "ILIKE"
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[inline]
    pub const fn ilike() -> &'static str {
        "LIKE"
    }

    /// Returns the SQL for boolean TRUE.
    /// PostgreSQL: TRUE
    /// SQLite: 1
    #[cfg(feature = "postgres")]
    #[inline]
    pub const fn bool_true() -> &'static str {
        "TRUE"
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[inline]
    pub const fn bool_true() -> &'static str {
        "1"
    }

    /// Returns the SQL for boolean FALSE.
    /// PostgreSQL: FALSE
    /// SQLite: 0
    #[cfg(feature = "postgres")]
    #[inline]
    pub const fn bool_false() -> &'static str {
        "FALSE"
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[inline]
    pub const fn bool_false() -> &'static str {
        "0"
    }

    /// Returns the SQL for interval arithmetic.
    /// PostgreSQL: column + INTERVAL '1 day'
    /// SQLite: datetime(column, '+1 day')
    #[cfg(feature = "postgres")]
    pub fn add_interval(column: &str, interval: &str) -> String {
        format!("{} + INTERVAL '{}'", column, interval)
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub fn add_interval(column: &str, interval: &str) -> String {
        // Convert PostgreSQL interval format to SQLite modifier
        // "1 day" -> "+1 day", "1 hour" -> "+1 hour"
        let modifier = if interval.starts_with('-') {
            interval.to_string()
        } else {
            format!("+{}", interval)
        };
        format!("datetime({}, '{}')", column, modifier)
    }

    /// Returns the SQL for date/time subtraction.
    /// PostgreSQL: column - INTERVAL '1 day'
    /// SQLite: datetime(column, '-1 day')
    #[cfg(feature = "postgres")]
    pub fn sub_interval(column: &str, interval: &str) -> String {
        format!("{} - INTERVAL '{}'", column, interval)
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    pub fn sub_interval(column: &str, interval: &str) -> String {
        // Convert PostgreSQL interval format to SQLite modifier
        let modifier = format!("-{}", interval.trim_start_matches('-'));
        format!("datetime({}, '{}')", column, modifier)
    }

    /// Generate IN clause with multiple placeholders for an array of values.
    /// Returns (sql_fragment, start_index) where start_index is the next placeholder index.
    ///
    /// Usage:
    /// ```ignore
    /// let (in_clause, _) = sql::in_clause(3, ids.len()); // $3, $4, $5, ...
    /// let query = format!("SELECT * FROM t WHERE id IN ({})", in_clause);
    /// ```
    pub fn in_clause(start_idx: usize, count: usize) -> (String, usize) {
        if count == 0 {
            return ("NULL".to_string(), start_idx); // Empty IN clause
        }
        let placeholders: Vec<String> = (start_idx..start_idx + count)
            .map(|i| format!("${}", i))
            .collect();
        (placeholders.join(", "), start_idx + count)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Array type wrapper for cross-database compatibility
// ══════════════════════════════════════════════════════════════════════════════

/// A wrapper for Vec<Uuid> that serializes to JSON for SQLite and uses native arrays for PostgreSQL.
/// Use this type in FromRow structs where the database column is UUID[] (Postgres) or TEXT/JSON (SQLite).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UuidArray(pub Vec<Uuid>);

impl std::ops::Deref for UuidArray {
    type Target = Vec<Uuid>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for UuidArray {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<Uuid>> for UuidArray {
    fn from(v: Vec<Uuid>) -> Self {
        UuidArray(v)
    }
}

impl From<UuidArray> for Vec<Uuid> {
    fn from(v: UuidArray) -> Self {
        v.0
    }
}

// PostgreSQL: Decode from UUID[]
#[cfg(feature = "postgres")]
impl<'r> sqlx::Decode<'r, sqlx::Postgres> for UuidArray {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let vec: Vec<Uuid> = <Vec<Uuid> as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(UuidArray(vec))
    }
}

#[cfg(feature = "postgres")]
impl sqlx::Type<sqlx::Postgres> for UuidArray {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <Vec<Uuid> as sqlx::Type<sqlx::Postgres>>::type_info()
    }
    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <Vec<Uuid> as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

#[cfg(feature = "postgres")]
impl<'q> sqlx::Encode<'q, sqlx::Postgres> for UuidArray {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <Vec<Uuid> as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.0, buf)
    }
}

// SQLite: Decode from TEXT (JSON)
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for UuidArray {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text: &str = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        if text.is_empty() || text == "[]" {
            return Ok(UuidArray(Vec::new()));
        }
        let vec: Vec<Uuid> = serde_json::from_str(text)?;
        Ok(UuidArray(vec))
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl sqlx::Type<sqlx::Sqlite> for UuidArray {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
    fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for UuidArray {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> sqlx::encode::IsNull {
        let json = serde_json::to_string(&self.0).unwrap_or_else(|_| "[]".to_string());
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(json, args)
    }
}

/// Helper to convert Vec<Uuid> for SQL binding.
/// For PostgreSQL, returns the vec directly (binds as array).
/// For SQLite, returns JSON string.
#[cfg(feature = "postgres")]
pub fn bind_uuid_array(ids: &[Uuid]) -> Vec<Uuid> {
    ids.to_vec()
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub fn bind_uuid_array(ids: &[Uuid]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string())
}

/// Execute a DELETE statement for multiple IDs.
/// PostgreSQL uses ANY($1), SQLite uses a loop.
#[cfg(feature = "postgres")]
pub async fn delete_by_ids(
    pool: &DbPool,
    table: &str,
    column: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let query = format!("DELETE FROM {} WHERE {} = ANY($1)", table, column);
    let result = sqlx::query(&query).bind(ids).execute(pool).await?;
    Ok(result.rows_affected())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn delete_by_ids(
    pool: &DbPool,
    table: &str,
    column: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    // Generate placeholders: $1, $2, $3, ...
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        "DELETE FROM {} WHERE {} IN ({})",
        table,
        column,
        placeholders.join(", ")
    );
    let mut q = sqlx::query(&query);
    for id in ids {
        q = q.bind(id.to_string());
    }
    let result = q.execute(pool).await?;
    Ok(result.rows_affected())
}

/// Execute an UPDATE statement for multiple IDs.
/// PostgreSQL uses ANY($1), SQLite uses IN clause with individual binds.
#[cfg(feature = "postgres")]
pub async fn update_by_ids(
    pool: &DbPool,
    table: &str,
    set_clause: &str,
    column: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let query = format!(
        "UPDATE {} SET {} WHERE {} = ANY($1)",
        table, set_clause, column
    );
    let result = sqlx::query(&query).bind(ids).execute(pool).await?;
    Ok(result.rows_affected())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn update_by_ids(
    pool: &DbPool,
    table: &str,
    set_clause: &str,
    column: &str,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        "UPDATE {} SET {} WHERE {} IN ({})",
        table,
        set_clause,
        column,
        placeholders.join(", ")
    );
    let mut q = sqlx::query(&query);
    for id in ids {
        q = q.bind(id.to_string());
    }
    let result = q.execute(pool).await?;
    Ok(result.rows_affected())
}

/// Select rows by IDs (returning Vec<Uuid>).
/// PostgreSQL uses ANY($1), SQLite uses IN clause.
#[cfg(feature = "postgres")]
pub async fn select_ids_by_ids<'e, E>(
    executor: E,
    table: &str,
    select_column: &str,
    where_column: &str,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let query = format!(
        "SELECT {} FROM {} WHERE {} = ANY($1)",
        select_column, table, where_column
    );
    let rows: Vec<(Uuid,)> = sqlx::query_as(&query).bind(ids).fetch_all(executor).await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn select_ids_by_ids<'e, E>(
    executor: E,
    table: &str,
    select_column: &str,
    where_column: &str,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
    let query = format!(
        "SELECT {} FROM {} WHERE {} IN ({})",
        select_column,
        table,
        where_column,
        placeholders.join(", ")
    );
    let mut q = sqlx::query_as(&query);
    for id in ids {
        q = q.bind(id.to_string());
    }
    let rows: Vec<(String,)> = q.fetch_all(executor).await?;
    // Parse UUID strings back to Uuid
    let uuids: Vec<Uuid> = rows
        .into_iter()
        .filter_map(|(s,)| Uuid::parse_str(&s).ok())
        .collect();
    Ok(uuids)
}

/// A wrapper for Vec<i32> that serializes to JSON for SQLite and uses native arrays for PostgreSQL.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntArray(pub Vec<i32>);

impl std::ops::Deref for IntArray {
    type Target = Vec<i32>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<i32>> for IntArray {
    fn from(v: Vec<i32>) -> Self {
        IntArray(v)
    }
}

// PostgreSQL: Decode from INTEGER[]
#[cfg(feature = "postgres")]
impl<'r> sqlx::Decode<'r, sqlx::Postgres> for IntArray {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let vec: Vec<i32> = <Vec<i32> as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(IntArray(vec))
    }
}

#[cfg(feature = "postgres")]
impl sqlx::Type<sqlx::Postgres> for IntArray {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <Vec<i32> as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

#[cfg(feature = "postgres")]
impl<'q> sqlx::Encode<'q, sqlx::Postgres> for IntArray {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        <Vec<i32> as sqlx::Encode<sqlx::Postgres>>::encode_by_ref(&self.0, buf)
    }
}

// SQLite: Decode from TEXT (JSON)
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for IntArray {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text: &str = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        if text.is_empty() || text == "[]" {
            return Ok(IntArray(Vec::new()));
        }
        let vec: Vec<i32> = serde_json::from_str(text)?;
        Ok(IntArray(vec))
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl sqlx::Type<sqlx::Sqlite> for IntArray {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for IntArray {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> sqlx::encode::IsNull {
        let json = serde_json::to_string(&self.0).unwrap_or_else(|_| "[]".to_string());
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(json, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_postgres() {
        #[cfg(feature = "postgres")]
        assert!(is_postgres());

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        assert!(!is_postgres());
    }

    #[test]
    fn test_is_sqlite() {
        #[cfg(feature = "postgres")]
        assert!(!is_sqlite());

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        assert!(is_sqlite());
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn test_sql_helpers_postgres() {
        assert_eq!(sql::now(), "now()");
        assert_eq!(sql::gen_uuid(), "gen_random_uuid()");
        assert_eq!(sql::json_extract("col", "field"), "col->>'field'");
        assert_eq!(sql::ilike(), "ILIKE");
        assert_eq!(sql::bool_true(), "TRUE");
        assert_eq!(sql::bool_false(), "FALSE");
        assert_eq!(sql::count_filter("x = 1"), "COUNT(*) FILTER (WHERE x = 1)");
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[test]
    fn test_sql_helpers_sqlite() {
        assert_eq!(sql::now(), "datetime('now')");
        assert_eq!(
            sql::json_extract("col", "field"),
            "json_extract(col, '$.field')"
        );
        assert_eq!(sql::ilike(), "LIKE");
        assert_eq!(sql::bool_true(), "1");
        assert_eq!(sql::bool_false(), "0");
        assert_eq!(
            sql::count_filter("x = 1"),
            "SUM(CASE WHEN x = 1 THEN 1 ELSE 0 END)"
        );
    }
}
