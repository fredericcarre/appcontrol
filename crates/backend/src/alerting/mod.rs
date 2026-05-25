//! Alerting layer: watches FSM transitions, evaluates declarative
//! policies, opens / closes alert instances, and dispatches notifications
//! through pluggable channels.
//!
//! This lives **alongside** the existing `core::notifications` webhook
//! dispatch — webhooks remain available for raw event streams (every
//! state change fans out to subscribers), while the alerting layer is
//! for policy-driven, human-facing destinations with sustain, severity,
//! cooldown, and ack/resolve lifecycle.

pub mod channels;
pub mod engine;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlertingError {
    #[error("database error: {0}")]
    Database(String),
    #[error("channel dispatch error: {0}")]
    Dispatch(String),
    #[error("invalid configuration: {0}")]
    Config(String),
}

impl From<sqlx::Error> for AlertingError {
    fn from(e: sqlx::Error) -> Self {
        AlertingError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for AlertingError {
    fn from(e: serde_json::Error) -> Self {
        AlertingError::Config(e.to_string())
    }
}
