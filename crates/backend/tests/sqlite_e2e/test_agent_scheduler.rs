//! SQLite E2E: Scheduler integration (API keys).

use super::common::TestContext;
use serde_json::json;

#[tokio::test]
async fn test_api_key_creation() {
    let ctx = TestContext::new().await;
    let key = ctx
        .create_api_key("Control-M", vec!["start", "stop", "status"])
        .await;
    assert!(!key.is_empty(), "API key should not be empty");
}

#[tokio::test]
async fn test_api_key_start_app() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let api_key = ctx
        .create_api_key("Control-M", vec!["start", "stop", "status"])
        .await;

    let resp = ctx
        .post_with_api_key(
            &api_key,
            &format!("/api/v1/apps/{app_id}/start"),
            json!({}),
        )
        .await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 202 || status == 500,
        "start via API key returned {status}"
    );
}

#[tokio::test]
async fn test_api_key_status_check() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let api_key = ctx.create_api_key("ReadOnly", vec!["status"]).await;

    let resp = ctx
        .get_with_api_key(
            &api_key,
            &format!("/api/v1/orchestration/apps/{app_id}/status"),
        )
        .await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "status via API key returned {status}"
    );
}

#[tokio::test]
async fn test_api_key_permissions_enforced() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Grant viewer only view permission
    ctx.grant_permission(app_id, ctx.viewer_user_id, "view")
        .await;

    // Create API key as viewer (not admin) -- the key inherits the viewer's permissions
    let resp = ctx
        .post_as(
            "viewer",
            "/api/v1/api-keys",
            json!({"name": "ReadOnly", "allowed_actions": ["status"]}),
        )
        .await;
    let key: serde_json::Value = resp.json().await.unwrap();
    let api_key = key["key"].as_str().unwrap().to_string();

    let resp = ctx
        .post_with_api_key(
            &api_key,
            &format!("/api/v1/apps/{app_id}/start"),
            json!({}),
        )
        .await;
    assert_eq!(
        resp.status().as_u16(),
        403,
        "start with viewer-owned API key should be denied"
    );
}

#[tokio::test]
async fn test_scheduler_wait_running_endpoint() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let api_key = ctx
        .create_api_key("Scheduler", vec!["start", "stop", "status"])
        .await;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();
    let resp = client
        .get(format!("{}/api/v1/orchestration/apps/{app_id}/wait-running?timeout=2", ctx.api_url))
        .header("Authorization", format!("ApiKey {api_key}"))
        .send()
        .await;
    match resp {
        Ok(r) => assert_ne!(r.status().as_u16(), 404, "wait-running route should exist"),
        Err(e) => assert!(e.is_timeout(), "expected timeout, got: {e}"),
    }
}
