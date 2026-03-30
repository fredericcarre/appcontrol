/// SQLite E2E: Permission enforcement (mirrors test_permissions_sharing.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_viewer_cannot_start_app() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Grant viewer permission
    ctx.post(
        &format!("/api/v1/apps/{app_id}/permissions/users"),
        json!({"user_id": ctx.viewer_user_id, "permission_level": "view"}),
    )
    .await;

    // Viewer tries to start — should be 403
    let resp = ctx
        .post_as(
            "viewer",
            &format!("/api/v1/apps/{app_id}/start"),
            json!({"dry_run": true}),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "Viewer should NOT be able to start an app"
    );
}

#[tokio::test]
async fn test_operator_can_start_app() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Grant operator permission
    ctx.post(
        &format!("/api/v1/apps/{app_id}/permissions/users"),
        json!({"user_id": ctx.operator_user_id, "permission_level": "operate"}),
    )
    .await;

    // Operator tries to start dry-run — should succeed
    let resp = ctx
        .post_as(
            "operator",
            &format!("/api/v1/apps/{app_id}/start"),
            json!({"dry_run": true}),
        )
        .await;
    // 200 = success, 503 = no gateway (acceptable in test)
    assert!(
        resp.status() == 200 || resp.status() == 503,
        "Operator with 'operate' should be able to start: {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_viewer_cannot_delete_app() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    ctx.post(
        &format!("/api/v1/apps/{app_id}/permissions/users"),
        json!({"user_id": ctx.viewer_user_id, "permission_level": "view"}),
    )
    .await;

    let resp = ctx
        .delete_as("viewer", &format!("/api/v1/apps/{app_id}"))
        .await;
    assert_eq!(
        resp.status(),
        403,
        "Viewer should NOT be able to delete an app"
    );
}

#[tokio::test]
async fn test_effective_permission() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Admin is org admin → owner
    let resp = ctx
        .get(&format!("/api/v1/apps/{app_id}/permissions/effective"))
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["permission_level"], "owner");
}
