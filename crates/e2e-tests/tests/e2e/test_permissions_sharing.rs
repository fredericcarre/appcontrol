/// E2E Test: Permissions, Team Sharing, Share Links
use super::*;

#[cfg(test)]
mod test_permissions_sharing {
    use super::*;

    #[tokio::test]
    async fn test_permission_levels_enforce_correctly() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Grant view to viewer user
        ctx.grant_permission(app_id, ctx.viewer_user_id, "view")
            .await;

        // Viewer can GET app
        let resp = ctx
            .get_as("viewer", &format!("/api/v1/apps/{}", app_id))
            .await;
        assert_eq!(resp.status(), 200);

        // Viewer CANNOT start app
        let resp = ctx
            .post_as(
                "viewer",
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 403);

        // Grant operate to operator user
        ctx.grant_permission(app_id, ctx.operator_user_id, "operate")
            .await;

        // Operator CAN start app
        let resp = ctx
            .post_as(
                "operator",
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert!(resp.status() == 200 || resp.status() == 202);

        // Operator CANNOT modify config
        let resp = ctx
            .put_as(
                "operator",
                &format!("/api/v1/apps/{}", app_id),
                json!({"name": "hacked"}),
            )
            .await;
        assert_eq!(resp.status(), 403);

        // Grant edit to editor user
        ctx.grant_permission(app_id, ctx.editor_user_id, "edit")
            .await;

        // Editor CAN modify config
        let resp = ctx
            .put_as(
                "editor",
                &format!("/api/v1/apps/{}", app_id),
                json!({"description": "updated"}),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Editor CANNOT start (edit != operate)
        let resp = ctx
            .post_as(
                "editor",
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 403);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_team_permission_grants_access_to_all_members() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create team with 2 members
        let team_id = ctx
            .create_team("Prod-Team", vec![ctx.operator_user_id, ctx.viewer_user_id])
            .await;

        // Grant operate to team
        ctx.grant_team_permission(app_id, team_id, "operate").await;

        // Both members can start
        let resp = ctx
            .post_as(
                "operator",
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert!(resp.status() == 200 || resp.status() == 202);

        // Viewer user (who is team member) now has operate via team
        let resp = ctx
            .get_as(
                "viewer",
                &format!("/api/v1/apps/{}/permissions/effective", app_id),
            )
            .await;
        let eff: Value = resp.json().await.unwrap();
        assert_eq!(eff["permission_level"], "operate");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_effective_permission_is_max_of_direct_and_team() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Direct: view
        ctx.grant_permission(app_id, ctx.operator_user_id, "view")
            .await;

        // Team: operate
        let team_id = ctx.create_team("Team", vec![ctx.operator_user_id]).await;
        ctx.grant_team_permission(app_id, team_id, "operate").await;

        // Effective should be MAX = operate
        let resp = ctx
            .get_as(
                "operator",
                &format!("/api/v1/apps/{}/permissions/effective", app_id),
            )
            .await;
        let eff: Value = resp.json().await.unwrap();
        assert_eq!(eff["permission_level"], "operate");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_expired_permission_ignored() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Grant operate with expiry in the past
        ctx.grant_permission_with_expiry(
            app_id,
            ctx.operator_user_id,
            "operate",
            chrono::Utc::now() - chrono::Duration::hours(1),
        )
        .await;

        // Should be denied (expired)
        let resp = ctx
            .post_as(
                "operator",
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 403);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_share_link_grants_access() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create share link (view level)
        let resp = ctx
            .post_as(
                "admin",
                &format!("/api/v1/apps/{}/share-links", app_id),
                json!({
                    "permission_level": "view",
                    "label": "For COMEX",
                    "expires_at": "2026-12-31T00:00:00Z"
                }),
            )
            .await;
        let link: Value = resp.json().await.unwrap();
        let token = link["token"].as_str().unwrap();

        // Access via share link (no auth needed)
        let resp = ctx.get_anonymous(&format!("/api/v1/share/{}", token)).await;
        assert_eq!(resp.status(), 200);

        let app: Value = resp.json().await.unwrap();
        assert_eq!(app["name"], "Paiements-SEPA");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_org_admin_has_implicit_owner() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        // Admin has no explicit permission on this app

        // But can do everything (implicit owner)
        let resp = ctx
            .delete_as("admin", &format!("/api/v1/apps/{}", app_id))
            .await;
        assert_eq!(
            resp.status(),
            200,
            "Org admin should have implicit owner access"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_permission_changes_are_audited() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        ctx.grant_permission(app_id, ctx.viewer_user_id, "operate")
            .await;

        let logs = ctx.get_action_log_for_type(app_id, "config_change").await;
        assert!(
            logs.iter()
                .any(|l| l.details["permission_level"].as_str() == Some("operate")),
            "Permission grant must be audited"
        );

        ctx.cleanup().await;
    }
}
