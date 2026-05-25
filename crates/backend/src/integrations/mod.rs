//! External-source ingestion connectors.
//!
//! Each submodule implements a connector for one family of upstream
//! referentials, materialising Phase 1 of the methodology (multi-source
//! captation). The HTTP layer (`api/ingestion.rs`) is a thin handler that
//! delegates business logic to these modules.
//!
//! Common pattern: each connector accepts a JSON payload referencing an
//! `application_id`, upserts the contained entities into the relevant
//! tables, and returns an `IngestionReport` describing what changed.
//!
//! Connectors:
//!
//! * `cmdb` — generic Configuration Management Database import
//!   (ServiceNow CMDB, BMC Atrium, custom). Maps rows to `components`.
//! * `xl` — XL Release / XL Deploy import. Pipelines become
//!   dependencies, deployables become components.
//! * `flow` — Network flow referential. Each authorised flow becomes a
//!   dependency between the matching components.
//! * `itsm` — Incident / Service Management import. Stores incidents and
//!   links them to the affected components.

pub mod cmdb;
pub mod flow;
pub mod git;
pub mod itsm;
pub mod jira_sm;
pub mod servicenow;
pub mod xl;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What an ingestion run should apply maturity to. Components-only for
/// CMDB-style scrapes, Dependencies-only for flow-ref imports, Both for
/// XL Release / Deploy (which produces both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaturityTarget {
    Components,
    Dependencies,
    Both,
}

/// Apply caller-declared maturity (knowledge_status + confidence_score)
/// to the rows that were just produced by an ingestion run.
///
/// Permissive on unknown statuses — silently drops them so a stray
/// value can't abort an otherwise successful ingestion.
pub async fn apply_default_maturity(
    pool: &crate::db::DbPool,
    app_id: Uuid,
    target: MaturityTarget,
    status: Option<&str>,
    confidence: Option<f32>,
) -> Result<(), crate::error::ApiError> {
    const ALLOWED: &[&str] = &["candidate", "draft", "reviewed", "validated", "deprecated"];
    let status = status.filter(|s| ALLOWED.contains(s));
    if status.is_none() && confidence.is_none() {
        return Ok(());
    }

    if matches!(target, MaturityTarget::Components | MaturityTarget::Both) {
        #[cfg(feature = "postgres")]
        sqlx::query(
            "UPDATE components SET
                knowledge_status = COALESCE($1, knowledge_status),
                confidence_score = COALESCE($2, confidence_score),
                updated_at = NOW()
              WHERE application_id = $3",
        )
        .bind(status)
        .bind(confidence)
        .bind(app_id)
        .execute(pool)
        .await?;
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        sqlx::query(
            "UPDATE components SET
                knowledge_status = COALESCE(?, knowledge_status),
                confidence_score = COALESCE(?, confidence_score),
                updated_at = CURRENT_TIMESTAMP
              WHERE application_id = ?",
        )
        .bind(status)
        .bind(confidence)
        .bind(crate::db::DbUuid::from(app_id))
        .execute(pool)
        .await?;
    }

    if matches!(target, MaturityTarget::Dependencies | MaturityTarget::Both) {
        #[cfg(feature = "postgres")]
        sqlx::query(
            "UPDATE dependencies SET
                knowledge_status = COALESCE($1, knowledge_status),
                confidence_score = COALESCE($2, confidence_score)
              WHERE from_component_id IN (
                SELECT id FROM components WHERE application_id = $3
              )",
        )
        .bind(status)
        .bind(confidence)
        .bind(app_id)
        .execute(pool)
        .await?;
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        sqlx::query(
            "UPDATE dependencies SET
                knowledge_status = COALESCE(?, knowledge_status),
                confidence_score = COALESCE(?, confidence_score)
              WHERE from_component_id IN (
                SELECT id FROM components WHERE application_id = ?
              )",
        )
        .bind(status)
        .bind(confidence)
        .bind(crate::db::DbUuid::from(app_id))
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Structured report returned by every ingestion connector.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct IngestionReport {
    /// Logical name of the source that produced the payload.
    pub source: String,
    /// Number of new entities created.
    pub created: usize,
    /// Number of existing entities updated.
    pub updated: usize,
    /// Number of entities skipped (already aligned, no change).
    pub skipped: usize,
    /// Per-row errors that did not abort the whole ingestion.
    pub errors: Vec<IngestionError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IngestionError {
    /// Optional identifier of the offending row (component name, flow id, etc.).
    pub item: Option<String>,
    pub message: String,
}

impl IngestionReport {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            ..Default::default()
        }
    }

    pub fn record_error(&mut self, item: Option<String>, message: impl Into<String>) {
        self.errors.push(IngestionError {
            item,
            message: message.into(),
        });
    }
}
