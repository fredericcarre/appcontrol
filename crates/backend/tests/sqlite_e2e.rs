//! SQLite E2E test suite — mirrors ALL PostgreSQL E2E tests for feature parity.
//!
//! Run with:
//!   CARGO_TARGET_DIR=$PWD/target-sqlite cargo test -p appcontrol-backend \
//!       --no-default-features --features sqlite --test sqlite_e2e

#![cfg(all(feature = "sqlite", not(feature = "postgres")))]
#![allow(dead_code)]

#[path = "sqlite_e2e/common.rs"]
mod common;

// --- Core CRUD ---
#[path = "sqlite_e2e/test_health.rs"]
mod test_health;
#[path = "sqlite_e2e/test_app_crud.rs"]
mod test_app_crud;
#[path = "sqlite_e2e/test_components.rs"]
mod test_components;
#[path = "sqlite_e2e/test_teams.rs"]
mod test_teams;
#[path = "sqlite_e2e/test_variables.rs"]
mod test_variables;

// --- DAG & Sequencing ---
#[path = "sqlite_e2e/test_dag.rs"]
mod test_dag;
#[path = "sqlite_e2e/test_start_stop.rs"]
mod test_start_stop;
#[path = "sqlite_e2e/test_branch_restart.rs"]
mod test_branch_restart;

// --- Permissions & Auth ---
#[path = "sqlite_e2e/test_permissions.rs"]
mod test_permissions;
#[path = "sqlite_e2e/test_org_isolation.rs"]
mod test_org_isolation;
#[path = "sqlite_e2e/test_share_links.rs"]
mod test_share_links;
#[path = "sqlite_e2e/test_saml_auth.rs"]
mod test_saml_auth;

// --- Audit & History ---
#[path = "sqlite_e2e/test_audit.rs"]
mod test_audit;
#[path = "sqlite_e2e/test_config_snapshots.rs"]
mod test_config_snapshots;

// --- Import & Export ---
#[path = "sqlite_e2e/test_yaml_import.rs"]
mod test_yaml_import;

// --- Advanced Operations ---
#[path = "sqlite_e2e/test_custom_commands.rs"]
mod test_custom_commands;
#[path = "sqlite_e2e/test_orchestration.rs"]
mod test_orchestration;
#[path = "sqlite_e2e/test_diagnostic.rs"]
mod test_diagnostic;
#[path = "sqlite_e2e/test_incident_lifecycle.rs"]
mod test_incident_lifecycle;
#[path = "sqlite_e2e/test_switchover.rs"]
mod test_switchover;
#[path = "sqlite_e2e/test_reports.rs"]
mod test_reports;

// --- Agents & WebSocket ---
#[path = "sqlite_e2e/test_agent_management.rs"]
mod test_agent_management;
#[path = "sqlite_e2e/test_agent_scheduler.rs"]
mod test_agent_scheduler;
#[path = "sqlite_e2e/test_websocket_events.rs"]
mod test_websocket_events;
#[path = "sqlite_e2e/test_app_type_components.rs"]
mod test_app_type_components;
