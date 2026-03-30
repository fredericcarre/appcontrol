//! SQLite E2E: Error Branch Detection and Selective Restart.
//!
//! Test app (10 components):
//!   DB-1 -> App-1 -> Front-1 / App-1 -> Queue-1 -> Worker-1
//!   DB-2 -> App-2 -> Front-2 / App-2 -> Queue-2 -> Worker-2
//!
//! Scenario: App-1 goes FAILED. Branch = [App-1, Front-1, Queue-1, Worker-1].

use super::common::TestContext;
use serde_json::Value;

#[tokio::test]
async fn test_detect_error_branch() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_ten_component_app().await;
    ctx.set_all_running(app_id).await;

    // Make App-1 fail
    ctx.force_component_state(app_id, "App-1", "FAILED").await;

    // Get DAG with branch info
    let resp = ctx
        .get(&format!("/api/v1/apps/{}/dag", app_id))
        .await;
    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    assert_eq!(status, 200, "GET dag failed: {body_text}");

    let dag: Value = serde_json::from_str(&body_text).unwrap();
    let error_branch: Vec<String> = dag["error_branch"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect();

    assert!(
        error_branch.contains(&"App-1".to_string()),
        "App-1 should be in error branch"
    );
    assert!(
        error_branch.contains(&"Front-1".to_string()),
        "Front-1 (dependent) should be in error branch"
    );
    assert!(
        error_branch.contains(&"Queue-1".to_string()),
        "Queue-1 (dependent) should be in error branch"
    );
    assert!(
        error_branch.contains(&"Worker-1".to_string()),
        "Worker-1 (dependent) should be in error branch"
    );
    assert!(
        !error_branch.contains(&"DB-1".to_string()),
        "DB-1 is healthy, should not be in branch"
    );
    assert!(
        !error_branch.contains(&"App-2".to_string()),
        "App-2 is in a different branch"
    );

    ctx.cleanup().await;
}
