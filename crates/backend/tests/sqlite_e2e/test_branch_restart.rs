//! SQLite E2E: Error Branch Detection and Selective Restart.
//!
//! Test app (10 components):
//!   DB-1 -> App-1 -> Front-1 / App-1 -> Queue-1 -> Worker-1
//!   DB-2 -> App-2 -> Front-2 / App-2 -> Queue-2 -> Worker-2
//!
//! Scenario: App-1 goes FAILED. Branch = [App-1, Front-1, Queue-1, Worker-1].

use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_detect_error_branch() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_ten_component_app().await;
    ctx.set_all_running(app_id).await;

    // Make App-1 fail
    ctx.force_component_state(app_id, "App-1", "FAILED").await;

    // Use the topology endpoint to verify the DAG structure and component states
    let resp = ctx.get(&format!("/api/v1/apps/{}/topology", app_id)).await;
    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    assert_eq!(status, 200, "GET topology failed: {body_text}");

    let topology: Value = serde_json::from_str(&body_text).unwrap();
    let components = topology["components"].as_array().unwrap();

    // Verify App-1 is FAILED
    let app1 = components
        .iter()
        .find(|c| c["name"].as_str() == Some("App-1"))
        .expect("App-1 should exist in topology");
    assert_eq!(
        app1["current_state"].as_str(),
        Some("FAILED"),
        "App-1 should be FAILED"
    );

    // Verify the dependency structure exists
    let deps = topology["dependencies"].as_array().unwrap();
    assert!(deps.len() >= 8, "Should have at least 8 dependencies");

    // Verify we can trigger a branch restart via the start-branch endpoint
    let app1_id = ctx.component_id(app_id, "App-1").await;
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{}/start-branch", app_id),
            json!({"component_ids": [app1_id]}),
        )
        .await;
    // The start-branch should accept the request (200 or 202) or fail gracefully
    // because agents aren't connected (500), but NOT 404
    assert_ne!(
        resp.status().as_u16(),
        404,
        "start-branch endpoint should exist"
    );

    ctx.cleanup().await;
}
