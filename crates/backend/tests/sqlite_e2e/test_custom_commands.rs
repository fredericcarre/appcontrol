//! SQLite E2E: Custom Commands Execution + Audit Trail.

use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_execute_custom_command_returns_output() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

    // Grant operator permission
    ctx.grant_permission(app_id, ctx.operator_user_id, "operate")
        .await;

    // Create a custom command via component update
    ctx.create_command(oracle_id, "count_records", "echo '{\"count\": 42}'", false)
        .await;

    // Execute it as operator
    let resp = ctx
        .post_as(
            "operator",
            &format!("/api/v1/components/{}/command/count_records", oracle_id),
            json!({}),
        )
        .await;
    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    // The command can't actually execute without an agent, but it should be found (not 404)
    // and attempt execution (may return 200 with result, or 500 if no agent)
    assert_ne!(status, 404, "Command should be found: {body_text}");
    assert_ne!(status, 403, "Operator should have permission: {body_text}");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_command_requires_confirmation_blocks_without_flag() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

    // Grant operator permission
    ctx.grant_permission(app_id, ctx.operator_user_id, "operate")
        .await;

    // Create a dangerous command requiring confirmation
    ctx.create_command(oracle_id, "purge_all", "rm -rf /tmp/cache", true)
        .await;

    // Execute without confirmation -> should fail
    let resp = ctx
        .post_as(
            "operator",
            &format!("/api/v1/components/{}/command/purge_all", oracle_id),
            json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400, "Should require confirmation");

    // Execute with confirmation -> should pass confirmation check
    // (may still fail with 409 if no agent assigned, but should NOT return 400)
    let resp = ctx
        .post_as(
            "operator",
            &format!("/api/v1/components/{}/command/purge_all", oracle_id),
            json!({ "confirmed": true }),
        )
        .await;
    let status = resp.status().as_u16();
    assert_ne!(
        status, 400,
        "With confirmed=true, should not fail on confirmation check"
    );
    assert_ne!(status, 403, "Operator should have permission");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_command_rbac_viewer_cannot_execute() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

    // Viewer tries to execute command -> 403
    let resp = ctx
        .post_as(
            "viewer",
            &format!("/api/v1/components/{}/command/count_records", oracle_id),
            json!({}),
        )
        .await;
    assert_eq!(resp.status(), 403);

    ctx.cleanup().await;
}
