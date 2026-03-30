//! SQLite E2E test suite — mirrors the PostgreSQL E2E tests for feature parity.
//!
//! Run with:
//!   cargo test --package appcontrol-backend --test sqlite_e2e --features sqlite --no-default-features

#![cfg(all(feature = "sqlite", not(feature = "postgres")))]
#![allow(dead_code)]

#[path = "sqlite_e2e/common.rs"]
mod common;
#[path = "sqlite_e2e/test_app_crud.rs"]
mod test_app_crud;
#[path = "sqlite_e2e/test_audit.rs"]
mod test_audit;
#[path = "sqlite_e2e/test_components.rs"]
mod test_components;
#[path = "sqlite_e2e/test_dag.rs"]
mod test_dag;
#[path = "sqlite_e2e/test_health.rs"]
mod test_health;
#[path = "sqlite_e2e/test_permissions.rs"]
mod test_permissions;
#[path = "sqlite_e2e/test_start_stop.rs"]
mod test_start_stop;
#[path = "sqlite_e2e/test_teams.rs"]
mod test_teams;
#[path = "sqlite_e2e/test_variables.rs"]
mod test_variables;
