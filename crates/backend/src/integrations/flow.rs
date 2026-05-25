//! Network flow referential ingestion.
//!
//! Each authorised network flow describes a directional dependency
//! between two components, identified by `(host, port)` or by component
//! name. The connector resolves both endpoints against the application's
//! existing components and inserts a dependency edge for each matched
//! pair. Unmatched endpoints are reported but do not abort ingestion.
//!
//! POST /api/v1/ingestion/flows
//! Body: {
//!   "application_id": "<uuid>",
//!   "source": "flux-ref",
//!   "flows": [ { "from": "billing-api", "to": "billing-db", "port": 5432, "protocol": "tcp" }, ... ]
//! }

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::ApiError;

use super::IngestionReport;

#[derive(Debug, Deserialize)]
pub struct FlowPayload {
    pub application_id: Uuid,
    #[serde(default = "default_source")]
    pub source: String,
    pub flows: Vec<FlowEntry>,
}

fn default_source() -> String {
    "flux-ref".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FlowEntry {
    /// Source component name or `host:port` selector.
    pub from: String,
    /// Destination component name or `host:port` selector.
    pub to: String,
    pub port: Option<i32>,
    pub protocol: Option<String>,
}

pub async fn ingest(pool: &DbPool, payload: FlowPayload) -> Result<IngestionReport, ApiError> {
    let mut report = IngestionReport::new(&payload.source);

    for flow in &payload.flows {
        let from_id = resolve_endpoint(pool, payload.application_id, &flow.from).await?;
        let to_id = resolve_endpoint(pool, payload.application_id, &flow.to).await?;

        let (from_id, to_id) = match (from_id, to_id) {
            (Some(f), Some(t)) => (f, t),
            _ => {
                report.record_error(
                    Some(format!("{} → {}", flow.from, flow.to)),
                    "one or both endpoints not found in this application",
                );
                continue;
            }
        };

        if from_id == to_id {
            report.record_error(
                Some(format!("{} → {}", flow.from, flow.to)),
                "self-dependency rejected",
            );
            continue;
        }

        #[cfg(feature = "postgres")]
        let inserted = sqlx::query(
            "INSERT INTO dependencies (id, from_component_id, to_component_id)
                  VALUES ($1, $2, $3)
                  ON CONFLICT (from_component_id, to_component_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(from_id)
        .bind(to_id)
        .execute(pool)
        .await?
        .rows_affected();

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO dependencies (id, from_component_id, to_component_id)
                  VALUES (?, ?, ?)",
        )
        .bind(crate::db::DbUuid::from(Uuid::new_v4()))
        .bind(crate::db::DbUuid::from(from_id))
        .bind(crate::db::DbUuid::from(to_id))
        .execute(pool)
        .await?
        .rows_affected();

        if inserted > 0 {
            report.created += 1;
        } else {
            report.skipped += 1;
        }
    }

    Ok(report)
}

/// Resolve an endpoint that may be either a plain component name
/// ("billing-api") or a `host:port` selector ("srv-12.prod:5432").
async fn resolve_endpoint(
    pool: &DbPool,
    app_id: Uuid,
    selector: &str,
) -> Result<Option<Uuid>, ApiError> {
    // Try name-first match — most common and cheaper.
    if let Some(id) = find_component_by_name(pool, app_id, selector).await? {
        return Ok(Some(id));
    }

    // Fall back to host[:port] match.
    let host = selector.split(':').next().unwrap_or(selector);

    #[cfg(feature = "postgres")]
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM components
              WHERE application_id = $1 AND host = $2
              LIMIT 1",
    )
    .bind(app_id)
    .bind(host)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<(crate::db::DbUuid,)> = sqlx::query_as(
        "SELECT id FROM components
              WHERE application_id = ? AND host = ?
              LIMIT 1",
    )
    .bind(crate::db::DbUuid::from(app_id))
    .bind(host)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id,)| {
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let id = id.into_inner();
        id
    }))
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
