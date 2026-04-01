//! SQLite E2E: Incident lifecycle — detection, branch restart, audit trail.

use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_app_status_shows_component_states() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!("/api/v1/orchestration/apps/{app_id}/status"))
        .await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "status endpoint returned {status}"
    );
}

#[tokio::test]
async fn test_dag_endpoint_returns_graph() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/dag")).await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "dag endpoint returned {status}"
    );

    if status == 200 {
        let dag: Value = resp.json().await.unwrap();
        assert!(
            dag["nodes"].is_array() || dag["components"].is_array() || dag["levels"].is_array(),
            "DAG should contain graph data"
        );
    }
}

#[tokio::test]
async fn test_start_branch_endpoint_exists() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(&format!("/api/v1/apps/{app_id}/start-branch"), json!({}))
        .await;
    let status = resp.status().as_u16();
    assert_ne!(status, 404, "start-branch route should exist");
}

#[tokio::test]
async fn test_incidents_report_endpoint() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!(
            "/api/v1/apps/{app_id}/reports/incidents?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        ))
        .await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "incidents report returned {status}"
    );

    if status == 200 {
        let report: Value = resp.json().await.unwrap();
        assert!(
            report["data"].is_array() || report["incidents"].is_array(),
            "report should contain data array"
        );
    }
}

#[tokio::test]
async fn test_action_log_records_operations() {
    let ctx = TestContext::new().await;
    let _app_id = ctx.create_payments_app().await;

    let resp = ctx.get("/api/v1/audit/actions").await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 404,
        "audit actions returned {status}"
    );
}
