//! SQLite E2E: Share link expiry, max_uses, revocation, listing, permissions.

use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_share_link_with_expiry() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({
                "permission_level": "view",
                "label": "Expired Link",
                "expires_at": "2020-01-01T00:00:00Z",
            }),
        )
        .await;
    assert!(
        resp.status().is_success(),
        "create share link: {}",
        resp.status()
    );
    let link: Value = resp.json().await.unwrap();
    let token = link["token"].as_str().unwrap();

    let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["valid"], false, "Expired share link should be invalid");
}

#[tokio::test]
async fn test_share_link_with_max_uses() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({
                "permission_level": "view",
                "label": "Limited Link",
                "max_uses": 2,
                "expires_at": "2030-12-31T00:00:00Z",
            }),
        )
        .await;
    assert!(resp.status().is_success());
    let link: Value = resp.json().await.unwrap();
    let token = link["token"].as_str().unwrap();

    let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["valid"], true, "Link should be valid initially");
}

#[tokio::test]
async fn test_share_link_revocation() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({
                "permission_level": "view",
                "label": "Revocable Link",
                "expires_at": "2030-12-31T00:00:00Z",
            }),
        )
        .await;
    let link: Value = resp.json().await.unwrap();
    let token = link["token"].as_str().unwrap();
    let link_id = link["id"].as_str().unwrap();

    let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx
        .delete_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links/{link_id}"),
        )
        .await;
    assert!(resp.status().is_success(), "revoke: {}", resp.status());

    let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
    let status = resp.status().as_u16();
    assert!(
        status == 404 || status == 401 || status == 403,
        "Revoked share link should be rejected, got {status}"
    );
}

#[tokio::test]
async fn test_list_share_links() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    for label in ["Link 1", "Link 2"] {
        ctx.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({
                "permission_level": "view",
                "label": label,
                "expires_at": "2030-12-31T00:00:00Z",
            }),
        )
        .await;
    }

    let resp = ctx
        .get_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let links = body["share_links"]
        .as_array()
        .or_else(|| body.as_array())
        .expect("should return links");
    assert!(links.len() >= 2, "should have at least 2 links");
}

#[tokio::test]
async fn test_share_link_requires_manage_permission() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    ctx.grant_permission(app_id, ctx.editor_user_id, "edit")
        .await;

    let resp = ctx
        .post_as(
            "editor",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({
                "permission_level": "view",
                "label": "Test",
                "expires_at": "2030-12-31T00:00:00Z",
            }),
        )
        .await;
    assert_eq!(resp.status(), 403, "share links require manage permission");
}
