/// E2E Test: Organization Isolation
///
/// Validates:
/// - Apps from Org A are invisible to Org B
/// - Cross-org API access returns 404 (not 403, to avoid leaking existence)
/// - Users can only see their own org's teams
/// - Agents are scoped to organization
use super::*;

#[cfg(test)]
mod test_org_isolation {
    use super::*;

    #[tokio::test]
    async fn test_apps_invisible_across_orgs() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create second org
        let (_, _, org2_token) = ctx.create_second_org().await;

        // Org2 admin tries to list apps → should not see Org1's app
        let resp = ctx.get_with_token(&org2_token, "/api/v1/apps").await;
        assert_eq!(resp.status(), 200);
        let apps: Value = resp.json().await.unwrap();
        let app_ids: Vec<&str> = apps
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["id"].as_str())
            .collect();
        assert!(
            !app_ids.contains(&app_id.to_string().as_str()),
            "Org2 should not see Org1's apps"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_cross_org_app_access_returns_404() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let (_, _, org2_token) = ctx.create_second_org().await;

        // Org2 tries to access Org1's app directly → 404
        let resp = ctx
            .get_with_token(&org2_token, &format!("/api/v1/apps/{app_id}"))
            .await;
        assert_eq!(
            resp.status(),
            404,
            "Cross-org app access should return 404 (not 403) to avoid leaking existence"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_cross_org_start_returns_404() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let (_, _, org2_token) = ctx.create_second_org().await;

        // Org2 tries to start Org1's app → 404
        let resp = ctx
            .post_with_token(
                &org2_token,
                &format!("/api/v1/apps/{app_id}/start"),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 404, "Cross-org start should return 404");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_cross_org_component_access_denied() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        let (_, _, org2_token) = ctx.create_second_org().await;

        // Org2 tries to access Org1's component → 404
        let resp = ctx
            .get_with_token(&org2_token, &format!("/api/v1/components/{oracle_id}"))
            .await;
        assert_eq!(resp.status(), 404);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_teams_scoped_to_org() {
        let ctx = TestContext::new().await;
        let _team_id = ctx
            .create_team("Org1-Team", vec![ctx.operator_user_id])
            .await;

        let (_, _, org2_token) = ctx.create_second_org().await;

        // Org2 lists teams → should not see Org1's team
        let resp = ctx.get_with_token(&org2_token, "/api/v1/teams").await;
        let teams: Value = resp.json().await.unwrap();
        let team_names: Vec<&str> = teams
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(
            !team_names.contains(&"Org1-Team"),
            "Org2 should not see Org1's teams"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_each_org_has_independent_apps() {
        let ctx = TestContext::new().await;

        // Org1 creates app
        let resp = ctx
            .post(
                "/api/v1/apps",
                json!({"name": "Org1-App", "site_id": ctx.default_site_id}),
            )
            .await;
        assert!(resp.status().is_success());

        // Org2 creates app with same name → should succeed (different org)
        let (org2_id, _, org2_token) = ctx.create_second_org().await;
        // Create a site for org2
        let org2_site_id = Uuid::new_v4();
        sqlx::query("INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Org2-Default', 'O2D')")
            .bind(org2_site_id)
            .bind(bind_id(org2_id))
            .execute(&ctx.db_pool)
            .await
            .unwrap();
        let resp = ctx
            .post_with_token(
                &org2_token,
                "/api/v1/apps",
                json!({"name": "Org1-App", "site_id": org2_site_id}),
            )
            .await;
        assert!(
            resp.status().is_success(),
            "Different orgs should be able to have apps with the same name"
        );

        ctx.cleanup().await;
    }
}
