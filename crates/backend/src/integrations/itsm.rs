//! ITSM / incident ingestion connector.
//!
//! Ingests incident tickets from ServiceNow, Jira Service Management,
//! PagerDuty or any other ITSM tool. Each incident is identified by
//! `(source, external_id)` and upserted into the `incidents` table. The
//! optional `impacted_components` list is resolved against components of
//! the target application and stored as a JSON array of UUIDs.
//!
//! POST /api/v1/ingestion/incidents
//! Body: { "application_id": "<uuid>", "source": "servicenow",
//!         "incidents": [ {...}, ... ] }

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::ApiError;

use super::IngestionReport;

#[derive(Debug, Deserialize)]
pub struct ItsmPayload {
    pub application_id: Option<Uuid>,
    pub organization_id: Uuid,
    #[serde(default = "default_source")]
    pub source: String,
    pub incidents: Vec<ItsmIncident>,
}

fn default_source() -> String {
    "itsm".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ItsmIncident {
    pub external_id: String,
    pub title: String,
    pub description: Option<String>,
    /// Free-form severity ("P1", "P2", "high", "low", ...). Stored as-is.
    pub severity: Option<String>,
    pub status: Option<String>,
    pub opened_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub root_cause: Option<String>,
    /// List of component names impacted by the incident (resolved against
    /// the target application). Names that don't match any component are
    /// silently dropped; they remain in `metadata.unresolved_components`
    /// for later inspection.
    #[serde(default)]
    pub impacted_component_names: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
}

pub async fn ingest(pool: &DbPool, payload: ItsmPayload) -> Result<IngestionReport, ApiError> {
    let mut report = IngestionReport::new(&payload.source);

    for incident in &payload.incidents {
        if incident.external_id.trim().is_empty() {
            report.record_error(None, "external_id is required");
            continue;
        }

        let (resolved_ids, unresolved_names) = match payload.application_id {
            Some(app_id) => resolve_component_names(pool, app_id, &incident.impacted_component_names).await?,
            None => (Vec::new(), incident.impacted_component_names.clone()),
        };

        let mut metadata = incident.metadata.clone();
        if !unresolved_names.is_empty() {
            if !metadata.is_object() {
                metadata = serde_json::json!({});
            }
            metadata["unresolved_components"] =
                serde_json::Value::Array(unresolved_names.iter().cloned().map(Value::String).collect());
        }

        let impacted_value = Value::Array(
            resolved_ids
                .iter()
                .map(|id| Value::String(id.to_string()))
                .collect(),
        );

        match upsert_incident(
            pool,
            payload.organization_id,
            payload.application_id,
            &payload.source,
            incident,
            &impacted_value,
            &metadata,
        )
        .await
        {
            Ok(UpsertOutcome::Created) => report.created += 1,
            Ok(UpsertOutcome::Updated) => report.updated += 1,
            Ok(UpsertOutcome::Skipped) => report.skipped += 1,
            Err(e) => report.record_error(Some(incident.external_id.clone()), e.to_string()),
        }
    }

    Ok(report)
}

enum UpsertOutcome {
    Created,
    Updated,
    #[allow(dead_code)] // reserved for future "no-op" detection
    Skipped,
}

#[allow(clippy::too_many_arguments)]
async fn upsert_incident(
    pool: &DbPool,
    org_id: Uuid,
    app_id: Option<Uuid>,
    source: &str,
    incident: &ItsmIncident,
    impacted: &Value,
    metadata: &Value,
) -> Result<UpsertOutcome, ApiError> {
    #[cfg(feature = "postgres")]
    let result = sqlx::query(
        "INSERT INTO incidents
            (id, organization_id, application_id, external_id, source, title,
             description, severity, status, opened_at, resolved_at, root_cause,
             impacted_components, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT (source, external_id) DO UPDATE
            SET title = EXCLUDED.title,
                description = EXCLUDED.description,
                severity = EXCLUDED.severity,
                status = EXCLUDED.status,
                resolved_at = EXCLUDED.resolved_at,
                root_cause = EXCLUDED.root_cause,
                impacted_components = EXCLUDED.impacted_components,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
         RETURNING (xmax = 0) AS inserted",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(app_id)
    .bind(&incident.external_id)
    .bind(source)
    .bind(&incident.title)
    .bind(&incident.description)
    .bind(&incident.severity)
    .bind(&incident.status)
    .bind(incident.opened_at)
    .bind(incident.resolved_at)
    .bind(&incident.root_cause)
    .bind(impacted)
    .bind(metadata)
    .fetch_one(pool)
    .await?;

    #[cfg(feature = "postgres")]
    {
        use sqlx::Row;
        let inserted: bool = result.try_get("inserted").unwrap_or(false);
        Ok(if inserted {
            UpsertOutcome::Created
        } else {
            UpsertOutcome::Updated
        })
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        // SQLite does not support RETURNING xmax — emulate via SELECT-then-INSERT/UPDATE.
        let existing: Option<(crate::db::DbUuid,)> = sqlx::query_as(
            "SELECT id FROM incidents WHERE source = ? AND external_id = ?",
        )
        .bind(source)
        .bind(&incident.external_id)
        .fetch_optional(pool)
        .await?;

        if existing.is_some() {
            sqlx::query(
                "UPDATE incidents
                    SET title = ?, description = ?, severity = ?, status = ?,
                        resolved_at = ?, root_cause = ?, impacted_components = ?,
                        metadata = ?, updated_at = CURRENT_TIMESTAMP
                  WHERE source = ? AND external_id = ?",
            )
            .bind(&incident.title)
            .bind(&incident.description)
            .bind(&incident.severity)
            .bind(&incident.status)
            .bind(incident.resolved_at)
            .bind(&incident.root_cause)
            .bind(crate::db::DbJson::from(impacted.clone()))
            .bind(crate::db::DbJson::from(metadata.clone()))
            .bind(source)
            .bind(&incident.external_id)
            .execute(pool)
            .await?;
            return Ok(UpsertOutcome::Updated);
        }

        sqlx::query(
            "INSERT INTO incidents
                (id, organization_id, application_id, external_id, source, title,
                 description, severity, status, opened_at, resolved_at, root_cause,
                 impacted_components, metadata)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(crate::db::DbUuid::from(Uuid::new_v4()))
        .bind(crate::db::DbUuid::from(org_id))
        .bind(app_id.map(crate::db::DbUuid::from))
        .bind(&incident.external_id)
        .bind(source)
        .bind(&incident.title)
        .bind(&incident.description)
        .bind(&incident.severity)
        .bind(&incident.status)
        .bind(incident.opened_at)
        .bind(incident.resolved_at)
        .bind(&incident.root_cause)
        .bind(crate::db::DbJson::from(impacted.clone()))
        .bind(crate::db::DbJson::from(metadata.clone()))
        .execute(pool)
        .await?;
        Ok(UpsertOutcome::Created)
    }
}

async fn resolve_component_names(
    pool: &DbPool,
    app_id: Uuid,
    names: &[String],
) -> Result<(Vec<Uuid>, Vec<String>), ApiError> {
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();

    for name in names {
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

        match row {
            Some((id,)) => {
                #[cfg(feature = "postgres")]
                resolved.push(id);
                #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
                resolved.push(id.into_inner());
            }
            None => unresolved.push(name.clone()),
        }
    }

    Ok((resolved, unresolved))
}
