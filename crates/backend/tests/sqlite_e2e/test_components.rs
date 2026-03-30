/// SQLite E2E: Component CRUD + Dependencies (mirrors test_component_operations.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_create_and_list_components() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
    assert_eq!(resp.status(), 200);
    let app: Value = resp.json().await.unwrap();
    let components = app["components"]
        .as_array()
        .expect("Should have components");
    assert_eq!(components.len(), 5, "Payments app should have 5 components");
}

#[tokio::test]
async fn test_create_and_list_dependencies() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
    let app: Value = resp.json().await.unwrap();
    let deps = app["dependencies"]
        .as_array()
        .expect("Should have dependencies");
    assert_eq!(deps.len(), 4, "Payments app should have 4 dependencies");
}

#[tokio::test]
async fn test_update_component() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({"name": "Comp-Test", "site_id": ctx.default_site_id}),
        )
        .await;
    let app: Value = resp.json().await.unwrap();
    let app_id = app["id"].as_str().unwrap();

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/components"),
            json!({
                "name": "TestComp",
                "component_type": "service",
                "hostname": "srv-test",
                "check_cmd": "check.sh",
                "start_cmd": "start.sh",
                "stop_cmd": "stop.sh",
            }),
        )
        .await;
    assert!(resp.status().is_success());
    let comp: Value = resp.json().await.unwrap();
    let comp_id = comp["id"].as_str().unwrap();

    let resp = ctx
        .put(
            &format!("/api/v1/apps/{app_id}/components/{comp_id}"),
            json!({"name": "Updated-Comp", "description": "Updated"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "Update component should return 200");
}

#[tokio::test]
async fn test_delete_component() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post(
            "/api/v1/apps",
            json!({"name": "Del-Comp-Test", "site_id": ctx.default_site_id}),
        )
        .await;
    let app: Value = resp.json().await.unwrap();
    let app_id = app["id"].as_str().unwrap();

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/components"),
            json!({"name": "ToDelete", "component_type": "service", "hostname": "srv", "check_cmd": "c.sh", "start_cmd": "s.sh", "stop_cmd": "t.sh"}),
        )
        .await;
    let comp: Value = resp.json().await.unwrap();
    let comp_id = comp["id"].as_str().unwrap();

    let resp = ctx
        .delete(&format!("/api/v1/apps/{app_id}/components/{comp_id}"))
        .await;
    assert!(
        resp.status().is_success(),
        "Delete component should succeed"
    );
}
