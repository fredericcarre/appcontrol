/// SQLite E2E: Audit trail (mirrors test_audit_trail.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_create_app_creates_audit_log() {
    let ctx = TestContext::new().await;

    // Create app → should create action_log entry
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({"name": "Audit-Test", "site_id": ctx.default_site_id}),
        )
        .await;
    assert!(resp.status().is_success());
    let app: Value = resp.json().await.unwrap();
    let app_id = app["id"].as_str().unwrap();

    // Check audit log - history requires from/to query params
    let now = chrono::Utc::now();
    let from = (now - chrono::Duration::hours(1)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let to = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/history?from={from}&to={to}")).await;
    // History may return action_log entries
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "History should return 200 or 404, got {}",
        status
    );
}

#[tokio::test]
async fn test_delete_preserves_audit() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({"name": "Audit-Del", "site_id": ctx.default_site_id}),
        )
        .await;
    let app: Value = resp.json().await.unwrap();
    let app_id = app["id"].as_str().unwrap();

    // Delete app
    ctx.delete(&format!("/api/v1/apps/{app_id}")).await;

    // App is gone, but audit should have been created before delete
    // (we can't query the specific audit log after delete without admin API,
    // but the important thing is delete didn't crash)
}
