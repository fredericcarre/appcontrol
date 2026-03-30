/// SQLite E2E: Variables and Groups CRUD (mirrors test_variables_groups.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_create_and_list_variables() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/variables"),
            json!({"name": "DB_HOST", "value": "localhost", "scope": "application"}),
        )
        .await;
    assert!(
        resp.status().is_success(),
        "Create variable: {}",
        resp.status()
    );

    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/variables")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let vars = body["variables"].as_array().expect("Should have variables array");
    assert!(!vars.is_empty(), "Should have at least 1 variable");
}

#[tokio::test]
async fn test_create_and_list_groups() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/groups"),
            json!({"name": "Backend", "description": "Backend services"}),
        )
        .await;
    assert!(
        resp.status().is_success(),
        "Create group: {}",
        resp.status()
    );

    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/groups")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let groups = body["groups"].as_array().expect("Should have groups array");
    assert!(!groups.is_empty());
}

#[tokio::test]
async fn test_update_variable() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/variables"),
            json!({"name": "PORT", "value": "8080", "scope": "application"}),
        )
        .await;
    let var: Value = resp.json().await.unwrap();
    let var_id = var["id"].as_str().unwrap();

    let resp = ctx
        .put(
            &format!("/api/v1/apps/{app_id}/variables/{var_id}"),
            json!({"value": "9090"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "Update variable: {}", resp.status());
}
