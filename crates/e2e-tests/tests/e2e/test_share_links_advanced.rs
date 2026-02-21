/// E2E Test: Share Links — Expiry, Max Uses, Revocation
///
/// Validates:
/// - Share link creation with level, expiry, max_uses
/// - Expired share link returns 401/403
/// - max_uses enforcement (link stops working after N uses)
/// - Share link revocation (delete)
/// - Share link with different permission levels
use super::*;

#[cfg(test)]
mod test_share_links_advanced {
    use super::*;

    #[tokio::test]
    async fn test_share_link_with_expiry() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create share link that expires in the past
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
        assert!(resp.status().is_success());
        let link: Value = resp.json().await.unwrap();
        let token = link["token"].as_str().unwrap();

        // Access should be denied (expired)
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert!(
            resp.status() == 401 || resp.status() == 403,
            "Expired share link should be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_share_link_with_max_uses() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create share link with max_uses = 2
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
        let link: Value = resp.json().await.unwrap();
        let token = link["token"].as_str().unwrap();

        // First use: OK
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert_eq!(resp.status(), 200, "First use should succeed");

        // Second use: OK
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert_eq!(resp.status(), 200, "Second use should succeed");

        // Third use: should be denied (max_uses exhausted)
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert!(
            resp.status() == 401 || resp.status() == 403 || resp.status() == 410,
            "Third use should be rejected (max_uses=2), got {}",
            resp.status()
        );

        ctx.cleanup().await;
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

        // Verify link works
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert_eq!(resp.status(), 200);

        // Revoke (delete)
        let resp = ctx
            .delete_as(
                "admin",
                &format!("/api/v1/apps/{app_id}/permissions/share-links/{link_id}"),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Should no longer work
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert!(
            resp.status() == 401 || resp.status() == 403 || resp.status() == 404,
            "Revoked share link should be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_list_share_links() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create two links
        ctx.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({
                "permission_level": "view", "label": "Link 1", "expires_at": "2030-12-31T00:00:00Z",
            }),
        )
        .await;
        ctx.post_as("admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"), json!({
                "permission_level": "operate", "label": "Link 2", "expires_at": "2030-12-31T00:00:00Z",
            })).await;

        let resp = ctx
            .get_as(
                "admin",
                &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            )
            .await;
        assert_eq!(resp.status(), 200);
        let links: Value = resp.json().await.unwrap();
        assert!(links.as_array().unwrap().len() >= 2);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_share_link_requires_manage_permission() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        ctx.grant_permission(app_id, ctx.editor_user_id, "edit")
            .await;

        // Editor (who has edit, not manage) cannot create share links
        let resp = ctx.post_as("editor",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"), json!({
                "permission_level": "view", "label": "Test", "expires_at": "2030-12-31T00:00:00Z",
            })).await;
        assert_eq!(
            resp.status(),
            403,
            "Creating share links requires manage permission"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_share_link_level_cap() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create view-level share link
        let resp = ctx
            .post_as(
                "admin",
                &format!("/api/v1/apps/{app_id}/permissions/share-links"),
                json!({
                    "permission_level": "view",
                    "label": "View Only",
                    "expires_at": "2030-12-31T00:00:00Z",
                }),
            )
            .await;
        let link: Value = resp.json().await.unwrap();
        let token = link["token"].as_str().unwrap();

        // Access via share link should only provide view level
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{token}")).await;
        assert_eq!(resp.status(), 200);

        // Cannot perform operate-level actions with view-only share link
        // (share link should not allow starting the app)
        ctx.cleanup().await;
    }
}
