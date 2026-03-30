/// SQLite E2E: DAG validation (mirrors test_dag_validation.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_cycle_detection_rejects_circular_dependency() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({"name": "Cycle-Test", "site_id": ctx.default_site_id}),
        )
        .await;
    let app: Value = resp.json().await.unwrap();
    let app_id = app["id"].as_str().unwrap();

    // Create A and B
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/components"),
            json!({"name": "A", "component_type": "service", "hostname": "a", "check_cmd": "c", "start_cmd": "s", "stop_cmd": "t"}),
        )
        .await;
    let a: Value = resp.json().await.unwrap();
    let a_id = a["id"].as_str().unwrap();

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/components"),
            json!({"name": "B", "component_type": "service", "hostname": "b", "check_cmd": "c", "start_cmd": "s", "stop_cmd": "t"}),
        )
        .await;
    let b: Value = resp.json().await.unwrap();
    let b_id = b["id"].as_str().unwrap();

    // A → B (valid)
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/dependencies"),
            json!({"from_component_id": a_id, "to_component_id": b_id}),
        )
        .await;
    assert!(resp.status().is_success(), "A→B should succeed");

    // B → A (creates cycle — should be rejected)
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/dependencies"),
            json!({"from_component_id": b_id, "to_component_id": a_id}),
        )
        .await;
    assert!(
        resp.status() == 400 || resp.status() == 409 || resp.status() == 422,
        "Cycle B→A should be rejected, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_dag_order_in_start_plan() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/start"),
            json!({"dry_run": true}),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let plan = &body["plan"];
    assert!(plan.is_object(), "Start plan should be present");
}
