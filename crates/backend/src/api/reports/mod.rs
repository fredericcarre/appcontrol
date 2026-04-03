//! Report API endpoints.

pub mod audit;
pub mod availability;
pub mod dora;
pub mod export;
pub mod incidents;

// Re-export all handler functions
pub use audit::{activity_feed, audit, global_audit, ActivityQuery, GlobalAuditQuery};
pub use availability::{availability, health_summary};
pub use dora::{compliance, mttr, rto};
pub use export::export_pdf;
pub use incidents::{drp_report, incidents, switchovers};

// Shared types
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub format: Option<String>,
}
