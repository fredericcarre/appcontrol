//! Repository pattern for database abstraction.
//!
//! All database queries go through Repository traits. Handlers never
//! write SQL directly. Each trait has PostgreSQL and SQLite implementations.
//!
//! Benefits:
//! - No `bind_id()` needed in handlers — the repo handles encoding
//! - Adding a new database backend = implementing the trait (1 file)
//! - Handlers are testable with mock repositories
//! - SQL is centralized, not scattered across 50+ handler files

pub mod agents;
pub mod apps;
pub mod auth_queries;
pub mod components;
pub mod core_queries;
pub mod discovery_queries;
pub mod enrollment;
pub mod gateway_queries;
pub mod gateways;
pub mod import_queries;
pub mod misc_queries;
pub mod org_queries;
pub mod permissions;
pub mod queries;
pub mod report_queries;
pub mod schedule_queries;
pub mod sites;
pub mod startup_queries;
pub mod switchover_queries;
pub mod teams;
pub mod websocket_queries;

/// Database-agnostic pool type.
pub use crate::db::DbPool;
