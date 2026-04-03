//! Report API endpoints.

pub mod audit;
pub mod availability;
pub mod incidents;
pub mod dora;
pub mod export;

// Re-export all handler functions
pub use audit::{global_audit, audit, activity_feed, GlobalAuditQuery, ActivityQuery};
pub use availability::{availability, health_summary};
pub use incidents::{incidents, switchovers, drp_report};
pub use dora::{compliance, rto, mttr};
pub use export::export_pdf;

// Shared types
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub format: Option<String>,
}
