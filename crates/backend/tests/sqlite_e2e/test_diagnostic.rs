//! SQLite E2E: Diagnostic and rebuild tests.
//! Merged from test_diagnostic_advanced.rs and test_diagnostic_rebuild.rs.

use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_diagnose_endpoint_returns_success() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app_with_checks().await;

    let resp = ctx
        .post(&format!("/api/v1/apps/{app_id}/diagnose"), json!({}))
        .await;
    assert!(
        resp.status().is_success(),
        "Diagnose should succeed, got {}",
        resp.status()
    );
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_diagnose_returns_component_list() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app_with_checks().await;

    let resp = ctx
        .post(&format!("/api/v1/apps/{app_id}/diagnose"), json!({}))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    // The response should contain diagnosis data
    let diagnosis = &body["diagnosis"];
    assert!(
        diagnosis.is_array(),
        "Diagnosis should be an array, got: {body}"
    );
    let components = diagnosis.as_array().unwrap();
    assert_eq!(components.len(), 4, "Should have 4 components");

    for comp in components {
        assert!(comp["component_name"].is_string());
        assert!(comp["recommendation"].is_string());
    }
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_diagnose_requires_operate_permission() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app_with_checks().await;

    ctx.grant_permission(app_id, ctx.viewer_user_id, "view")
        .await;

    let resp = ctx
        .post_as(
            "viewer",
            &format!("/api/v1/apps/{app_id}/diagnose"),
            json!({}),
        )
        .await;
    assert_eq!(resp.status(), 403, "Diagnose requires operate permission");
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_rebuild_endpoint_exists() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app_with_checks().await;

    // Rebuild with dry_run to avoid needing real agents
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/rebuild"),
            json!({"dry_run": true}),
        )
        .await;
    // Should not be 404 — endpoint exists
    assert_ne!(resp.status().as_u16(), 404, "Rebuild endpoint should exist");
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_rebuild_requires_manage_permission() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app_with_checks().await;
    let tomcat_id = ctx.component_id(app_id, "Tomcat").await;

    ctx.grant_permission(app_id, ctx.operator_user_id, "operate")
        .await;

    let resp = ctx
        .post_as(
            "operator",
            &format!("/api/v1/apps/{app_id}/rebuild"),
            json!({
                "components": [{ "id": tomcat_id, "action": "app_rebuild" }]
            }),
        )
        .await;
    assert_eq!(resp.status(), 403, "Rebuild requires manage permission");
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_diagnose_audit_trail() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app_with_checks().await;

    ctx.post(&format!("/api/v1/apps/{app_id}/diagnose"), json!({}))
        .await;

    let logs = ctx.get_action_log(app_id, "diagnose").await;
    assert!(!logs.is_empty(), "Diagnose should be logged in action_log");
    ctx.cleanup().await;
}
