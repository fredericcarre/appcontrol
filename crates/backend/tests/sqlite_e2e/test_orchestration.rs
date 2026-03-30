//! SQLite E2E: Orchestration / scheduler integration tests.

use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_status_endpoint_structure() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!("/api/v1/orchestration/apps/{app_id}/status"))
        .await;
    assert_eq!(resp.status(), 200);

    let status: Value = resp.json().await.unwrap();
    assert!(
        status["components"].is_array(),
        "Status should have components array"
    );
    let components = status["components"].as_array().unwrap();
    assert_eq!(components.len(), 5, "Payments app has 5 components");

    for comp in components {
        assert!(comp["name"].is_string());
        assert!(comp["state"].is_string());
    }
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_wait_running_returns_timeout() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!(
            "/api/v1/orchestration/apps/{app_id}/wait-running?timeout=2"
        ))
        .await;

    let status_code = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["status"].as_str() == Some("TIMEOUT")
            || body["status"].as_str() == Some("STOPPED")
            || status_code == 408,
        "Should indicate timeout, got: {:?}",
        body
    );
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_wait_running_returns_failed_on_failure() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    ctx.force_component_state(app_id, "Oracle-DB", "FAILED")
        .await;

    let resp = ctx
        .get(&format!(
            "/api/v1/orchestration/apps/{app_id}/wait-running?timeout=2"
        ))
        .await;

    let body: Value = resp.json().await.unwrap();
    assert!(
        body["status"].as_str() == Some("FAILED")
            || body["all_running"].as_bool() == Some(false),
        "Should indicate failure when component is FAILED"
    );
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_api_key_crud() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/v1/api-keys",
            json!({
                "name": "Test-Key",
                "allowed_actions": ["start", "stop", "status"],
            }),
        )
        .await;
    assert!(resp.status().is_success());
    let key: Value = resp.json().await.unwrap();
    assert!(key["key"].is_string(), "Should return API key string");
    assert!(key["id"].is_string(), "Should return key ID");

    let resp = ctx.get("/api/v1/api-keys").await;
    assert_eq!(resp.status(), 200);
    let keys: Value = resp.json().await.unwrap();
    assert!(!keys.as_array().unwrap().is_empty());

    let key_id = key["id"].as_str().unwrap();
    let resp = ctx
        .delete_as("admin", &format!("/api/v1/api-keys/{key_id}"))
        .await;
    assert_eq!(resp.status(), 200);
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_orchestration_endpoints_via_api_key() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let api_key = ctx
        .create_api_key("Orchestrator", vec!["start", "stop", "status"])
        .await;

    let resp = ctx
        .get_with_api_key(
            &api_key,
            &format!("/api/v1/orchestration/apps/{app_id}/status"),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let resp = ctx
        .post_with_api_key(
            &api_key,
            &format!("/api/v1/orchestration/apps/{app_id}/start"),
            json!({}),
        )
        .await;
    assert!(resp.status().is_success() || resp.status() == 202);
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_concurrent_start_idempotent() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp1 = ctx
        .post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
        .await;
    let resp2 = ctx
        .post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
        .await;

    assert!(resp1.status().is_success());
    assert!(
        resp2.status().is_success() || resp2.status() == 409,
        "Concurrent start should be idempotent or rejected, got {}",
        resp2.status()
    );
    ctx.cleanup().await;
}
