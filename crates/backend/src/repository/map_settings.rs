//! Read/write the per-application `map_display_options` JSON blob added by
//! migration V051. Two backends (PG = JSONB, SQLite = TEXT) so the queries
//! are gated by the same `feature` flags as the rest of the repo.
//!
//! Treated as opaque JSON on the backend — the option vocabulary lives in
//! the frontend (it's the renderer's contract, not the API's). This avoids
//! a migration each time we add a toggle.

use serde_json::Value;
use uuid::Uuid;

use crate::db::DbPool;

#[cfg(feature = "postgres")]
pub async fn get(pool: &DbPool, app_id: Uuid) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<Value>>(
        "SELECT map_display_options FROM applications WHERE id = $1",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_optional(pool)
    .await
    .map(|r| r.flatten())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn get(pool: &DbPool, app_id: Uuid) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<Option<String>> =
        sqlx::query_scalar("SELECT map_display_options FROM applications WHERE id = $1")
            .bind(crate::db::DbUuid::from(app_id))
            .fetch_optional(pool)
            .await?;
    Ok(row.flatten().and_then(|s| serde_json::from_str(&s).ok()))
}

#[cfg(feature = "postgres")]
pub async fn set(pool: &DbPool, app_id: Uuid, opts: &Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE applications SET map_display_options = $2, updated_at = now() WHERE id = $1",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(opts)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn set(pool: &DbPool, app_id: Uuid, opts: &Value) -> Result<(), sqlx::Error> {
    let serialized = serde_json::to_string(opts).unwrap_or_else(|_| "{}".to_string());
    sqlx::query(
        "UPDATE applications SET map_display_options = $2, updated_at = datetime('now') WHERE id = $1",
    )
    .bind(crate::db::DbUuid::from(app_id))
    .bind(serialized)
    .execute(pool)
    .await?;
    Ok(())
}
