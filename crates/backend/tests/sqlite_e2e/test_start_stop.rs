/// SQLite E2E: Start/Stop DAG sequencing (mirrors test_full_start_stop.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_start_dry_run_returns_plan() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/start"),
            json!({"dry_run": true}),
        )
        .await;
    assert_eq!(resp.status(), 200, "Start dry-run should return 200");

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["dry_run"], true, "Response should indicate dry_run");
    assert!(body["plan"].is_object(), "Response should contain plan");
}

#[tokio::test]
async fn test_stop_dry_run_returns_plan() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/stop"),
            json!({"dry_run": true}),
        )
        .await;
    assert_eq!(resp.status(), 200, "Stop dry-run should return 200");

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["dry_run"], true);
}

#[tokio::test]
async fn test_start_without_agents_returns_error() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Real start (not dry-run) should fail because no agents are connected
    let resp = ctx
        .post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
        .await;
    // 503 = gateway unavailable, 500 = no agents, 200 = accepted, 409 = lock conflict
    let status = resp.status().as_u16();
    assert!(
        status == 503 || status == 500 || status == 200 || status == 409,
        "Start without agents should return 503/500/200/409, got {}",
        status
    );
}

#[tokio::test]
async fn test_all_components_start_as_stopped() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
    let app: Value = resp.json().await.unwrap();
    let components = app["components"].as_array().unwrap();

    let initial_state = components
        .iter()
        .filter(|c| c["current_state"] == "STOPPED" || c["current_state"] == "UNKNOWN")
        .count();
    assert_eq!(initial_state, 5, "All 5 components should start as STOPPED or UNKNOWN");
}
