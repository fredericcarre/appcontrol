//! XL Release / XL Deploy ingestion connector.
//!
//! XL Deploy ships deployment manifests describing the *deployable units*
//! of an application and their target environments. XL Release defines
//! the pipelines that orchestrate those deployments and the activity
//! dependencies between them.
//!
//! The connector accepts a payload combining both views and:
//!   * upserts components from XL Deploy `deployables`
//!   * inserts (best-effort) ordering dependencies between components from
//!     XL Release pipeline activities
//!
//! POST /api/v1/ingestion/xl
//! Body: {
//!   "application_id": "<uuid>",
//!   "source": "xl-release",
//!   "deployables": [ { "name": "billing-api", "host": "srv-12", "package": "billing-api/2.7.3" }, ... ],
//!   "pipeline_dependencies": [ { "from": "billing-db", "to": "billing-api" }, ... ]
//! }

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::ApiError;

use super::IngestionReport;

#[derive(Debug, Deserialize)]
pub struct XlPayload {
    pub application_id: Uuid,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub deployables: Vec<XlDeployable>,
    #[serde(default)]
    pub pipeline_dependencies: Vec<XlPipelineDep>,
}

fn default_source() -> String {
    "xl-release".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct XlDeployable {
    pub name: String,
    #[serde(default = "default_type")]
    pub component_type: String,
    pub host: Option<String>,
    pub package: Option<String>,
    pub environment: Option<String>,
}

fn default_type() -> String {
    "service".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct XlPipelineDep {
    pub from: String,
    pub to: String,
}

pub async fn ingest(pool: &DbPool, payload: XlPayload) -> Result<IngestionReport, ApiError> {
    let mut report = IngestionReport::new(&payload.source);

    // Reuse the CMDB upsert path for deployables — the data model is identical.
    let cmdb_payload = super::cmdb::CmdbPayload {
        application_id: payload.application_id,
        source: payload.source.clone(),
        components: payload
            .deployables
            .iter()
            .map(|d| {
                let mut tags = Vec::new();
                if let Some(p) = &d.package {
                    tags.push(format!("package:{}", p));
                }
                if let Some(e) = &d.environment {
                    tags.push(format!("env:{}", e));
                }
                super::cmdb::CmdbComponent {
                    name: d.name.clone(),
                    component_type: d.component_type.clone(),
                    host: d.host.clone(),
                    description: d.package.clone(),
                    display_name: None,
                    tags,
                }
            })
            .collect(),
    };

    let component_report = super::cmdb::ingest(pool, cmdb_payload).await?;
    report.created += component_report.created;
    report.updated += component_report.updated;
    report.skipped += component_report.skipped;
    report.errors.extend(component_report.errors);

    // Add dependencies from XL Release pipeline ordering.
    for dep in &payload.pipeline_dependencies {
        match upsert_dependency(pool, payload.application_id, &dep.from, &dep.to).await {
            Ok(DepOutcome::Created) => report.created += 1,
            Ok(DepOutcome::Skipped) => report.skipped += 1,
            Err(e) => report.record_error(
                Some(format!("{} → {}", dep.from, dep.to)),
                e.to_string(),
            ),
        }
    }

    Ok(report)
}

enum DepOutcome {
    Created,
    Skipped,
}

async fn upsert_dependency(
    pool: &DbPool,
    app_id: Uuid,
    from_name: &str,
    to_name: &str,
) -> Result<DepOutcome, ApiError> {
    let from_id = find_component_by_name(pool, app_id, from_name)
        .await?
        .ok_or_else(|| {
            ApiError::Validation(format!("source component '{}' not found", from_name))
        })?;
    let to_id = find_component_by_name(pool, app_id, to_name).await?.ok_or_else(
        || ApiError::Validation(format!("target component '{}' not found", to_name)),
    )?;

    // Dependencies have a unique (from, to) constraint.
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

    Ok(if inserted > 0 {
        DepOutcome::Created
    } else {
        DepOutcome::Skipped
    })
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

// Suppress unused-import lint when feature flags hide one path.
#[allow(dead_code)]
fn _unused(v: Value) -> Value {
    v
}
