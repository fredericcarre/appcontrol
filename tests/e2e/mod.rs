// E2E Integration Tests for AppControl v4
//
// Each test module validates a complete scenario.
// Tests require a PostgreSQL 16 instance (see tests/CLAUDE.md).
//
// Run: cargo test --test e2e

mod test_full_start_stop;
mod test_branch_restart;
mod test_switchover;
mod test_diagnostic_rebuild;
mod test_custom_commands;
mod test_permissions_sharing;
mod test_audit_trail;
mod test_agent_and_scheduler;

// Shared test utilities
mod common;
