//! Repository queries for the component catalog.

#![allow(unused_imports, dead_code, clippy::too_many_arguments)]
use crate::db::{DbPool, DbUuid};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug, serde::Serialize)]
pub struct CatalogEntry {
    pub id: Uuid,
    pub org_id: Uuid,
    pub type_key: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: String,
    pub color: String,
    pub category: Option<String>,
    pub default_check_cmd: Option<String>,
    pub default_start_cmd: Option<String>,
    pub default_stop_cmd: Option<String>,
    pub default_env_vars: Option<Value>,
    pub display_order: i32,
    pub is_builtin: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// Internal row for sqlx mapping
#[derive(Debug, sqlx::FromRow)]
struct CatalogRow {
    #[cfg(feature = "postgres")]
    id: Uuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    id: DbUuid,
    #[cfg(feature = "postgres")]
    org_id: Uuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    org_id: DbUuid,
    type_key: String,
    label: String,
    description: Option<String>,
    icon: String,
    color: String,
    category: Option<String>,
    default_check_cmd: Option<String>,
    default_start_cmd: Option<String>,
    default_stop_cmd: Option<String>,
    #[cfg(feature = "postgres")]
    default_env_vars: Option<Value>,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    default_env_vars: Option<crate::db::DbJson>,
    display_order: i32,
    is_builtin: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl CatalogRow {
    fn into_entry(self) -> CatalogEntry {
        CatalogEntry {
            #[cfg(feature = "postgres")]
            id: self.id,
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            id: self.id.into_inner(),
            #[cfg(feature = "postgres")]
            org_id: self.org_id,
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            org_id: self.org_id.into_inner(),
            type_key: self.type_key,
            label: self.label,
            description: self.description,
            icon: self.icon,
            color: self.color,
            category: self.category,
            default_check_cmd: self.default_check_cmd,
            default_start_cmd: self.default_start_cmd,
            default_stop_cmd: self.default_stop_cmd,
            #[cfg(feature = "postgres")]
            default_env_vars: self.default_env_vars,
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            default_env_vars: self.default_env_vars.map(|j| j.0),
            display_order: self.display_order,
            is_builtin: self.is_builtin,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

const SELECT_COLS: &str = "\
    id, org_id, type_key, label, description, icon, color, category, \
    default_check_cmd, default_start_cmd, default_stop_cmd, default_env_vars, \
    display_order, is_builtin, created_at, updated_at";

// ============================================================================
// Queries
// ============================================================================

/// List all catalog entries for an organization (builtin + custom), ordered.
pub async fn list_catalog(pool: &DbPool, org_id: Uuid) -> Result<Vec<CatalogEntry>, sqlx::Error> {
    let sql = format!(
        "SELECT {} FROM component_catalog WHERE org_id = $1 ORDER BY display_order, category, label",
        SELECT_COLS
    );

    #[cfg(feature = "postgres")]
    let rows = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(crate::db::bind_id(org_id))
        .fetch_all(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rows = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(DbUuid::from(org_id))
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(|r| r.into_entry()).collect())
}

/// Get a single catalog entry by ID.
pub async fn get_catalog_entry(
    pool: &DbPool,
    entry_id: Uuid,
) -> Result<Option<CatalogEntry>, sqlx::Error> {
    let sql = format!(
        "SELECT {} FROM component_catalog WHERE id = $1",
        SELECT_COLS
    );

    #[cfg(feature = "postgres")]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(entry_id)
        .fetch_optional(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(DbUuid::from(entry_id))
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| r.into_entry()))
}

/// Get a catalog entry by org + type_key.
pub async fn get_catalog_entry_by_key(
    pool: &DbPool,
    org_id: Uuid,
    type_key: &str,
) -> Result<Option<CatalogEntry>, sqlx::Error> {
    let sql = format!(
        "SELECT {} FROM component_catalog WHERE org_id = $1 AND type_key = $2",
        SELECT_COLS
    );

    #[cfg(feature = "postgres")]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(crate::db::bind_id(org_id))
        .bind(type_key)
        .fetch_optional(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(DbUuid::from(org_id))
        .bind(type_key)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| r.into_entry()))
}

/// Create a new catalog entry.
pub async fn create_catalog_entry(
    pool: &DbPool,
    id: Uuid,
    org_id: Uuid,
    type_key: &str,
    label: &str,
    description: Option<&str>,
    icon: &str,
    color: &str,
    category: Option<&str>,
    default_check_cmd: Option<&str>,
    default_start_cmd: Option<&str>,
    default_stop_cmd: Option<&str>,
    default_env_vars: Option<&Value>,
    display_order: i32,
    is_builtin: bool,
) -> Result<CatalogEntry, sqlx::Error> {
    let sql = format!(
        "INSERT INTO component_catalog \
            (id, org_id, type_key, label, description, icon, color, category, \
             default_check_cmd, default_start_cmd, default_stop_cmd, default_env_vars, \
             display_order, is_builtin) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
         RETURNING {}",
        SELECT_COLS
    );

    #[cfg(feature = "postgres")]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(id)
        .bind(crate::db::bind_id(org_id))
        .bind(type_key)
        .bind(label)
        .bind(description)
        .bind(icon)
        .bind(color)
        .bind(category)
        .bind(default_check_cmd)
        .bind(default_start_cmd)
        .bind(default_stop_cmd)
        .bind(default_env_vars)
        .bind(display_order)
        .bind(is_builtin)
        .fetch_one(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(type_key)
        .bind(label)
        .bind(description)
        .bind(icon)
        .bind(color)
        .bind(category)
        .bind(default_check_cmd)
        .bind(default_start_cmd)
        .bind(default_stop_cmd)
        .bind(default_env_vars.map(|v| crate::db::DbJson(v.clone())))
        .bind(display_order)
        .bind(is_builtin)
        .fetch_one(pool)
        .await?;

    Ok(row.into_entry())
}

/// Update an existing catalog entry (only non-builtin entries can be fully updated).
pub async fn update_catalog_entry(
    pool: &DbPool,
    entry_id: Uuid,
    org_id: Uuid,
    label: Option<&str>,
    description: Option<&str>,
    icon: Option<&str>,
    color: Option<&str>,
    category: Option<&str>,
    default_check_cmd: Option<&str>,
    default_start_cmd: Option<&str>,
    default_stop_cmd: Option<&str>,
    default_env_vars: Option<&Value>,
    display_order: Option<i32>,
) -> Result<Option<CatalogEntry>, sqlx::Error> {
    let sql = format!(
        "UPDATE component_catalog SET \
            label = COALESCE($3, label), \
            description = COALESCE($4, description), \
            icon = COALESCE($5, icon), \
            color = COALESCE($6, color), \
            category = COALESCE($7, category), \
            default_check_cmd = COALESCE($8, default_check_cmd), \
            default_start_cmd = COALESCE($9, default_start_cmd), \
            default_stop_cmd = COALESCE($10, default_stop_cmd), \
            default_env_vars = COALESCE($11, default_env_vars), \
            display_order = COALESCE($12, display_order), \
            updated_at = {} \
         WHERE id = $1 AND org_id = $2 \
         RETURNING {}",
        crate::db::sql::now(),
        SELECT_COLS
    );

    #[cfg(feature = "postgres")]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(entry_id)
        .bind(crate::db::bind_id(org_id))
        .bind(label)
        .bind(description)
        .bind(icon)
        .bind(color)
        .bind(category)
        .bind(default_check_cmd)
        .bind(default_start_cmd)
        .bind(default_stop_cmd)
        .bind(default_env_vars)
        .bind(display_order)
        .fetch_optional(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = sqlx::query_as::<_, CatalogRow>(&sql)
        .bind(DbUuid::from(entry_id))
        .bind(DbUuid::from(org_id))
        .bind(label)
        .bind(description)
        .bind(icon)
        .bind(color)
        .bind(category)
        .bind(default_check_cmd)
        .bind(default_start_cmd)
        .bind(default_stop_cmd)
        .bind(default_env_vars.map(|v| crate::db::DbJson(v.clone())))
        .bind(display_order)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| r.into_entry()))
}

/// Delete a catalog entry. Returns true if a row was deleted.
/// Builtin entries cannot be deleted (enforced at API layer).
pub async fn delete_catalog_entry(
    pool: &DbPool,
    entry_id: Uuid,
    org_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let result = sqlx::query(
        "DELETE FROM component_catalog WHERE id = $1 AND org_id = $2 AND is_builtin = false",
    )
    .bind(entry_id)
    .bind(crate::db::bind_id(org_id))
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query(
        "DELETE FROM component_catalog WHERE id = $1 AND org_id = $2 AND is_builtin = false",
    )
    .bind(DbUuid::from(entry_id))
    .bind(DbUuid::from(org_id))
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Seed built-in types for an organization (idempotent — skips existing).
pub async fn seed_builtin_types(pool: &DbPool, org_id: Uuid) -> Result<u64, sqlx::Error> {
    let builtins = [
        (
            "database",
            "Database",
            "SQL, NoSQL, or data stores",
            "database",
            "#1565C0",
            "database",
            0,
        ),
        (
            "middleware",
            "Middleware",
            "Message queues, cache, ESB",
            "layers",
            "#6A1B9A",
            "middleware",
            1,
        ),
        (
            "appserver",
            "App Server",
            "Application servers, backends",
            "server",
            "#2E7D32",
            "application",
            2,
        ),
        (
            "webfront",
            "Web Front",
            "Web servers, load balancers",
            "globe",
            "#E65100",
            "access",
            3,
        ),
        (
            "service",
            "Service",
            "Microservices, APIs",
            "cog",
            "#37474F",
            "application",
            4,
        ),
        (
            "batch",
            "Batch",
            "Scheduled jobs, ETL",
            "clock",
            "#4E342E",
            "scheduler",
            5,
        ),
        (
            "custom",
            "Custom",
            "Other component types",
            "box",
            "#455A64",
            "other",
            6,
        ),
        (
            "application",
            "Application",
            "Reference to another app (synthetic)",
            "folder",
            "#3B82F6",
            "composite",
            7,
        ),
    ];

    let mut inserted: u64 = 0;
    for (type_key, label, description, icon, color, category, order) in builtins {
        let sql = "INSERT INTO component_catalog \
            (id, org_id, type_key, label, description, icon, color, category, display_order, is_builtin) \
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, true) \
            ON CONFLICT (org_id, type_key) DO NOTHING";

        #[cfg(feature = "postgres")]
        let result = sqlx::query(sql)
            .bind(Uuid::new_v4())
            .bind(crate::db::bind_id(org_id))
            .bind(type_key)
            .bind(label)
            .bind(description)
            .bind(icon)
            .bind(color)
            .bind(category)
            .bind(order)
            .execute(pool)
            .await?;

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let result = sqlx::query(sql)
            .bind(DbUuid::new_v4())
            .bind(DbUuid::from(org_id))
            .bind(type_key)
            .bind(label)
            .bind(description)
            .bind(icon)
            .bind(color)
            .bind(category)
            .bind(order)
            .execute(pool)
            .await?;

        inserted += result.rows_affected();
    }

    Ok(inserted)
}
