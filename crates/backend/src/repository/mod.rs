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

pub mod apps;

use async_trait::async_trait;
use uuid::Uuid;

/// Database-agnostic pool type.
pub use crate::db::DbPool;
