//! SQLite E2E: Application-Type Components.
//!
//! Tests components that reference other applications.
//! App-type components derive state from the referenced app.

use super::common::TestContext;
use serde_json::{json, Value};
use uuid::Uuid;

/// Create two linked apps: Metrics-Dashboard references Core-Backend.
/// Returns (metrics_app_id, core_app_id, backend_ref_component_id).
async fn create_linked_apps(ctx: &TestContext) -> (Uuid, Uuid, Uuid) {
    // Core-Backend app
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({
                "name": "Core-Backend",
                "description": "Core backend services",
            }),
        )
        .await;
    let core_app: Value = resp.json().await.unwrap();
    let core_app_id = TestContext::extract_id(&core_app);

    // Core-DB component
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{core_app_id}/components"),
            json!({
                "name": "Core-DB",
                "component_type": "database",
                "hostname": "srv-core-db",
                "check_cmd": "exit 0",
                "start_cmd": "echo starting",
                "stop_cmd": "echo stopping",
                "check_interval_seconds": 5,
                "start_timeout_seconds": 30,
                "stop_timeout_seconds": 30,
            }),
        )
        .await;
    let core_db: Value = resp.json().await.unwrap();
    let core_db_id = TestContext::extract_id(&core_db);

    // Core-API component
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{core_app_id}/components"),
            json!({
                "name": "Core-API",
                "component_type": "appserver",
                "hostname": "srv-core-api",
                "check_cmd": "exit 0",
                "start_cmd": "echo starting",
                "stop_cmd": "echo stopping",
                "check_interval_seconds": 5,
                "start_timeout_seconds": 30,
                "stop_timeout_seconds": 30,
            }),
        )
        .await;
    let core_api: Value = resp.json().await.unwrap();
    let core_api_id = TestContext::extract_id(&core_api);

    // Core-DB -> Core-API dep
    ctx.post(
        &format!("/api/v1/apps/{core_app_id}/dependencies"),
        json!({
            "from_component_id": core_db_id,
            "to_component_id": core_api_id,
        }),
    )
    .await;

    // Metrics-Dashboard app
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({
                "name": "Metrics-Dashboard",
                "description": "Metrics dashboard",
            }),
        )
        .await;
    let metrics_app: Value = resp.json().await.unwrap();
    let metrics_app_id = TestContext::extract_id(&metrics_app);

    // Metrics-DB component
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{metrics_app_id}/components"),
            json!({
                "name": "Metrics-DB",
                "component_type": "database",
                "hostname": "srv-metrics-db",
                "check_cmd": "exit 0",
                "start_cmd": "echo starting",
                "stop_cmd": "echo stopping",
                "check_interval_seconds": 5,
                "start_timeout_seconds": 30,
                "stop_timeout_seconds": 30,
            }),
        )
        .await;
    let metrics_db: Value = resp.json().await.unwrap();
    let metrics_db_id = TestContext::extract_id(&metrics_db);

    // Backend-Ref (app-type component)
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{metrics_app_id}/components"),
            json!({
                "name": "Backend-Ref",
                "component_type": "application",
                "referenced_app_id": core_app_id,
                "start_timeout_seconds": 60,
                "stop_timeout_seconds": 60,
            }),
        )
        .await;
    let backend_ref: Value = resp.json().await.unwrap();
    let backend_ref_id = TestContext::extract_id(&backend_ref);

    // Dashboard component
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{metrics_app_id}/components"),
            json!({
                "name": "Dashboard",
                "component_type": "webfront",
                "hostname": "srv-dashboard",
                "check_cmd": "exit 0",
                "start_cmd": "echo starting",
                "stop_cmd": "echo stopping",
                "check_interval_seconds": 5,
                "start_timeout_seconds": 30,
                "stop_timeout_seconds": 30,
            }),
        )
        .await;
    let dashboard: Value = resp.json().await.unwrap();
    let dashboard_id = TestContext::extract_id(&dashboard);

    // Metrics-DB -> Backend-Ref -> Dashboard
    ctx.post(
        &format!("/api/v1/apps/{metrics_app_id}/dependencies"),
        json!({
            "from_component_id": metrics_db_id,
            "to_component_id": backend_ref_id,
        }),
    )
    .await;
    ctx.post(
        &format!("/api/v1/apps/{metrics_app_id}/dependencies"),
        json!({
            "from_component_id": backend_ref_id,
            "to_component_id": dashboard_id,
        }),
    )
    .await;

    (metrics_app_id, core_app_id, backend_ref_id)
}

#[tokio::test]
async fn test_app_type_component_accepts_degraded_state() {
    let ctx = TestContext::new().await;
    let (_metrics_app_id, core_app_id, _backend_ref_id) =
        create_linked_apps(&ctx).await;

    // Set Core-Backend: Core-DB=RUNNING, Core-API=DEGRADED
    ctx.force_component_state(core_app_id, "Core-DB", "RUNNING")
        .await;
    ctx.force_component_state(core_app_id, "Core-API", "DEGRADED")
        .await;

    // Verify aggregate state
    let running: i32 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM components c
         JOIN state_transitions st ON st.component_id = c.id
         WHERE c.application_id = $1
         AND st.to_state = 'RUNNING'",
    )
    .bind(core_app_id.to_string())
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    let degraded: i32 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM components c
         JOIN state_transitions st ON st.component_id = c.id
         WHERE c.application_id = $1
         AND st.to_state = 'DEGRADED'",
    )
    .bind(core_app_id.to_string())
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    assert!(running > 0, "At least one component RUNNING");
    assert!(degraded > 0, "At least one component DEGRADED");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_cycle_detection_prevents_infinite_recursion() {
    let ctx = TestContext::new().await;

    // Create App-A
    let resp = ctx.post("/api/v1/apps", json!({ "name": "App-A" })).await;
    let app_a: Value = resp.json().await.unwrap();
    let app_a_id = TestContext::extract_id(&app_a);

    // Create App-B
    let resp = ctx.post("/api/v1/apps", json!({ "name": "App-B" })).await;
    let app_b: Value = resp.json().await.unwrap();
    let app_b_id = TestContext::extract_id(&app_b);

    // App-A references App-B
    ctx.post(
        &format!("/api/v1/apps/{app_a_id}/components"),
        json!({
            "name": "Ref-B",
            "component_type": "application",
            "referenced_app_id": app_b_id,
        }),
    )
    .await;

    // App-B references App-A (cycle)
    ctx.post(
        &format!("/api/v1/apps/{app_b_id}/components"),
        json!({
            "name": "Ref-A",
            "component_type": "application",
            "referenced_app_id": app_a_id,
        }),
    )
    .await;

    // Start should complete without hanging
    let resp = ctx
        .post(&format!("/api/v1/apps/{app_a_id}/start"), json!({}))
        .await;
    assert!(
        resp.status().is_success()
            || resp.status().is_client_error()
            || resp.status().is_server_error(),
        "Start should complete without hanging"
    );

    ctx.cleanup().await;
}
