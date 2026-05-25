//! CMDB ingestion connector.
//!
//! Generic enough to consume rows extracted from any CMDB (ServiceNow,
//! BMC Atrium, custom). Each row describes a component to attach to an
//! application: by `name` (within the app), with optional host, type,
//! description and tags. Existing components are matched by `name` and
//! updated; missing ones are created.
//!
//! POST /api/v1/ingestion/cmdb
//! Body: { "application_id": "<uuid>", "source": "servicenow",
//!         "components": [ {...}, ... ] }

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::ApiError;

use super::IngestionReport;

#[derive(Debug, Deserialize)]
pub struct CmdbPayload {
    pub application_id: Uuid,
    /// Free-form label of the originating CMDB (e.g. "servicenow", "bmc").
    /// Stored in the per-row report for traceability.
    #[serde(default = "default_source")]
    pub source: String,
    pub components: Vec<CmdbComponent>,
    /// Optional caller-declared maturity. If set, applied to all
    /// components (including those just updated) after ingestion. The
    /// system does NOT pick a default — leaving this absent keeps the
    /// existing column values intact.
    #[serde(default)]
    pub default_knowledge_status: Option<String>,
    #[serde(default)]
    pub default_confidence_score: Option<f32>,
}

fn default_source() -> String {
    "cmdb".to_string()
}

/// One row of CMDB data describing a component candidate.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CmdbComponent {
    /// Component name — unique within the application.
    pub name: String,
    /// Component type label (free-form; matches the relaxed CHECK introduced in V031).
    #[serde(default = "default_type")]
    pub component_type: String,
    pub host: Option<String>,
    pub description: Option<String>,
    pub display_name: Option<String>,
    /// Arbitrary tags (technology, owner, lifecycle, etc.). Stored as JSONB.
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_type() -> String {
    "service".to_string()
}

/// Process a CMDB payload against the database.
///
/// For each component:
///   * if a component with the same name already exists in the application,
///     update its `host`, `description`, `display_name`, and `tags`;
///   * otherwise insert a new component with sensible defaults
///     (state=UNKNOWN, no commands, no agent).
///
/// Returns a structured report counting created/updated/skipped/errors.
pub async fn ingest(pool: &DbPool, payload: CmdbPayload) -> Result<IngestionReport, ApiError> {
    let mut report = IngestionReport::new(&payload.source);

    // Ensure the target application exists.
    application_exists(pool, payload.application_id)
        .await?
        .then_some(())
        .ok_or(ApiError::NotFound)?;

    for raw in payload.components {
        if raw.name.trim().is_empty() {
            report.record_error(None, "component name is required");
            continue;
        }

        let tags_value = Value::Array(
            raw.tags
                .iter()
                .map(|t| Value::String(t.clone()))
                .collect(),
        );

        match upsert_component(pool, payload.application_id, &raw, &tags_value).await {
            Ok(UpsertOutcome::Created) => report.created += 1,
            Ok(UpsertOutcome::Updated) => report.updated += 1,
            Ok(UpsertOutcome::Skipped) => report.skipped += 1,
            Err(e) => report.record_error(Some(raw.name.clone()), e.to_string()),
        }
    }

    super::apply_default_maturity(
        pool,
        payload.application_id,
        super::MaturityTarget::Components,
        payload.default_knowledge_status.as_deref(),
        payload.default_confidence_score,
    )
    .await?;

    Ok(report)
}

enum UpsertOutcome {
    Created,
    Updated,
    Skipped,
}

async fn upsert_component(
    pool: &DbPool,
    app_id: Uuid,
    row: &CmdbComponent,
    tags: &Value,
) -> Result<UpsertOutcome, ApiError> {
    let existing_id = find_component_by_name(pool, app_id, &row.name).await?;

    if let Some(_component_id) = existing_id {
        #[cfg(feature = "postgres")]
        let affected = sqlx::query(
            "UPDATE components
                SET component_type = $1,
                    host = COALESCE($2, host),
                    description = COALESCE($3, description),
                    display_name = COALESCE($4, display_name),
                    tags = $5,
                    updated_at = NOW()
              WHERE application_id = $6 AND name = $7",
        )
        .bind(&row.component_type)
        .bind(&row.host)
        .bind(&row.description)
        .bind(&row.display_name)
        .bind(tags)
        .bind(app_id)
        .bind(&row.name)
        .execute(pool)
        .await?
        .rows_affected();

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let affected = sqlx::query(
            "UPDATE components
                SET component_type = ?,
                    host = COALESCE(?, host),
                    description = COALESCE(?, description),
                    display_name = COALESCE(?, display_name),
                    tags = ?,
                    updated_at = CURRENT_TIMESTAMP
              WHERE application_id = ? AND name = ?",
        )
        .bind(&row.component_type)
        .bind(&row.host)
        .bind(&row.description)
        .bind(&row.display_name)
        .bind(crate::db::DbJson::from(tags.clone()))
        .bind(crate::db::DbUuid::from(app_id))
        .bind(&row.name)
        .execute(pool)
        .await?
        .rows_affected();

        Ok(if affected > 0 {
            UpsertOutcome::Updated
        } else {
            UpsertOutcome::Skipped
        })
    } else {
        let new_id = Uuid::new_v4();

        #[cfg(feature = "postgres")]
        sqlx::query(
            "INSERT INTO components
                (id, application_id, name, component_type, host, description, display_name, tags)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(new_id)
        .bind(app_id)
        .bind(&row.name)
        .bind(&row.component_type)
        .bind(&row.host)
        .bind(&row.description)
        .bind(&row.display_name)
        .bind(tags)
        .execute(pool)
        .await?;

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        sqlx::query(
            "INSERT INTO components
                (id, application_id, name, component_type, host, description, display_name, tags)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(crate::db::DbUuid::from(new_id))
        .bind(crate::db::DbUuid::from(app_id))
        .bind(&row.name)
        .bind(&row.component_type)
        .bind(&row.host)
        .bind(&row.description)
        .bind(&row.display_name)
        .bind(crate::db::DbJson::from(tags.clone()))
        .execute(pool)
        .await?;

        Ok(UpsertOutcome::Created)
    }
}

async fn application_exists(pool: &DbPool, app_id: Uuid) -> Result<bool, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM applications WHERE id = $1")
        .bind(app_id)
        .fetch_optional(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<(crate::db::DbUuid,)> =
        sqlx::query_as("SELECT id FROM applications WHERE id = ?")
            .bind(crate::db::DbUuid::from(app_id))
            .fetch_optional(pool)
            .await?;

    Ok(row.is_some())
}

async fn find_component_by_name(
    pool: &DbPool,
    app_id: Uuid,
    name: &str,
) -> Result<Option<Uuid>, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM components WHERE application_id = $1 AND name = $2",
    )
    .bind(app_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<(crate::db::DbUuid,)> = sqlx::query_as(
        "SELECT id FROM components WHERE application_id = ? AND name = ?",
    )
    .bind(crate::db::DbUuid::from(app_id))
    .bind(name)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id,)| {
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let id = id.into_inner();
        id
    }))
}
