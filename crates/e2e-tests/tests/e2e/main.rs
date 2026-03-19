// E2E Integration Tests for AppControl v4
//
// Each test module validates a complete scenario.
// Tests require a PostgreSQL 16 instance (see tests/CLAUDE.md).
//
// Run: cargo test --test e2e

// Shared test utilities — re-export for all test modules
mod common;
pub use common::*;

// Re-export dependencies used by test modules via `use super::*;`
pub use serde_json::{json, Value};
pub use std::time::Duration;
pub use uuid::Uuid;

// ---- Original test modules ----
mod test_agent_and_scheduler;
mod test_audit_trail;
mod test_branch_restart;
mod test_custom_commands;
mod test_diagnostic_rebuild;
mod test_full_start_stop;
mod test_permissions_sharing;
mod test_switchover;

// ---- New comprehensive test modules ----
mod test_agent_management;
mod test_app_crud;
mod test_component_operations;
mod test_config_snapshots;
mod test_dag_validation;
mod test_diagnostic_advanced;
mod test_health_endpoints;
mod test_incident_lifecycle;
mod test_orchestration_advanced;
mod test_org_isolation;
mod test_reports;
mod test_saml_auth;
mod test_share_links_advanced;
mod test_switchover_advanced;
mod test_teams_crud;
mod test_variables_groups;
mod test_websocket_events;
mod test_yaml_import;

// ---- Application-type components (referenced apps) ----
mod test_app_type_components;
