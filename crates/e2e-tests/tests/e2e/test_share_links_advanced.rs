/// E2E Test: Share Links — Expiry, Revocation, Max Uses, Listing
use super::*;

#[cfg(test)]
mod test_share_links_advanced {
    use super::*;

    #[tokio::test]
    async fn test_list_share_links() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create 2 share links
        ctx.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({"permission_level": "view", "label": "Link-1"}),
        )
        .await;
        ctx.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/share-links"),
            json!({"permission_level": "operate", "label": "Link-2"}),
        )
        .await;

        let resp = ctx
            .get(&format!(
                "/api/v1/apps/{app_id}/permissions/share-links"
            ))
            .await;
        assert_eq!(resp.status(), 200);
        let links: Value = resp.json().await.unwrap();
        let links_arr = links.as_array()
            .or_else(|| links["links"].as_array())
            .or_else(|| links["share_links"].as_array())
            .expect("Response should contain links array");
        assert!(links_arr.len() >= 2, "Should have at least 2 share links");

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

        // Use the consume endpoint with different users each time
        // First use
        let user1_token = ctx.viewer_token.clone();
        let resp = ctx
            .post_with_token(
                &user1_token,
                "/api/v1/share-links/consume",
                json!({"token": token}),
            )
            .await;
        assert!(
            resp.status().is_success(),
            "First use should succeed, got {}",
            resp.status()
        );

        // Second use
        let user2_token = ctx.operator_token.clone();
        let resp = ctx
            .post_with_token(
                &user2_token,
                "/api/v1/share-links/consume",
                json!({"token": token}),
            )
            .await;
        assert!(
            resp.status().is_success(),
            "Second use should succeed, got {}",
            resp.status()
        );

        // Third use: should be denied (max_uses exhausted)
        let user3_token = ctx.editor_token.clone();
        let resp = ctx
            .post_with_token(
                &user3_token,
                "/api/v1/share-links/consume",
                json!({"token": token}),
            )
            .await;
        assert!(
            resp.status() == 400 || resp.status() == 403 || resp.status() == 410 || resp.status() == 409,
            "Third use should be rejected (max_uses=2), got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_share_link_with_expiry() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create share link that already expired
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
        let link: Value = resp.json().await.unwrap();
        let token = link["token"].as_str().unwrap();

        // Try to consume expired link
        let resp = ctx
            .post_with_token(
                &ctx.viewer_token,
                "/api/v1/share-links/consume",
                json!({"token": token}),
            )
            .await;
        assert!(
            resp.status() == 400 || resp.status() == 403 || resp.status() == 410,
            "Expired share link should be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_share_link_revocation() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create share link
        let resp = ctx
            .post_as(
                "admin",
                &format!("/api/v1/apps/{app_id}/permissions/share-links"),
                json!({"permission_level": "view", "label": "To Revoke"}),
            )
            .await;
        let link: Value = resp.json().await.unwrap();
        let link_id = link["id"].as_str().unwrap();
        let token = link["token"].as_str().unwrap();

        // Revoke it
        let resp = ctx
            .delete_as(
                "admin",
                &format!("/api/v1/apps/{app_id}/permissions/share-links/{link_id}"),
            )
            .await;
        assert!(
            resp.status() == 200 || resp.status() == 204,
            "Revocation should succeed"
        );

        // Try to consume revoked link
        let resp = ctx
            .post_with_token(
                &ctx.viewer_token,
                "/api/v1/share-links/consume",
                json!({"token": token}),
            )
            .await;
        assert!(
            resp.status() == 400 || resp.status() == 404,
            "Revoked share link should be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }
}
