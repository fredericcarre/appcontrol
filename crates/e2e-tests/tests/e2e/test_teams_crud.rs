/// E2E Test: Team CRUD, Members, Leader Privileges
use super::*;

#[cfg(test)]
mod test_teams_crud {
    use super::*;

    #[tokio::test]
    async fn test_create_team() {
        let ctx = TestContext::new().await;

        let resp = ctx
            .post(
                "/api/v1/teams",
                json!({
                    "name": "Prod-Team",
                    "description": "Production operations team",
                }),
            )
            .await;
        assert!(resp.status().is_success());
        let team: Value = resp.json().await.unwrap();
        assert_eq!(team["name"], "Prod-Team");
        assert!(team["id"].as_str().is_some());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_list_teams() {
        let ctx = TestContext::new().await;
        ctx.create_team("Team-A", vec![]).await;
        ctx.create_team("Team-B", vec![]).await;

        let resp = ctx.get("/api/v1/teams").await;
        assert_eq!(resp.status(), 200);
        let teams: Value = resp.json().await.unwrap();
        assert!(teams.as_array().unwrap().len() >= 2);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_team_crud_lifecycle() {
        let ctx = TestContext::new().await;

        // Create
        let resp = ctx
            .post(
                "/api/v1/teams",
                json!({
                    "name": "Lifecycle-Team",
                    "description": "To be updated",
                }),
            )
            .await;
        let team: Value = resp.json().await.unwrap();
        let team_id: Uuid = team["id"].as_str().unwrap().parse().unwrap();

        // Read
        let resp = ctx.get(&format!("/api/v1/teams/{team_id}")).await;
        assert_eq!(resp.status(), 200);
        let team: Value = resp.json().await.unwrap();
        assert_eq!(team["name"], "Lifecycle-Team");

        // Update
        let resp = ctx
            .put(
                &format!("/api/v1/teams/{team_id}"),
                json!({
                    "name": "Updated-Team",
                    "description": "Updated description",
                }),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Verify update
        let resp = ctx.get(&format!("/api/v1/teams/{team_id}")).await;
        let team: Value = resp.json().await.unwrap();
        assert_eq!(team["name"], "Updated-Team");

        // Delete
        let resp = ctx
            .delete_as("admin", &format!("/api/v1/teams/{team_id}"))
            .await;
        assert_eq!(resp.status(), 200);

        // Verify deleted
        let resp = ctx.get(&format!("/api/v1/teams/{team_id}")).await;
        assert_eq!(resp.status(), 404);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_add_and_remove_team_members() {
        let ctx = TestContext::new().await;
        let team_id = ctx.create_team("Member-Team", vec![]).await;

        // Add member
        let resp = ctx
            .post(
                &format!("/api/v1/teams/{team_id}/members"),
                json!({
                    "user_id": ctx.operator_user_id,
                    "role": "member",
                }),
            )
            .await;
        assert!(resp.status().is_success());

        // List members
        let resp = ctx.get(&format!("/api/v1/teams/{team_id}/members")).await;
        assert_eq!(resp.status(), 200);
        let members: Value = resp.json().await.unwrap();
        let member_ids: Vec<&str> = members
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["user_id"].as_str().unwrap())
            .collect();
        assert!(member_ids.contains(&ctx.operator_user_id.to_string().as_str()));

        // Remove member
        let resp = ctx
            .delete_as(
                "admin",
                &format!("/api/v1/teams/{team_id}/members/{}", ctx.operator_user_id),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Verify removed
        let resp = ctx.get(&format!("/api/v1/teams/{team_id}/members")).await;
        let members: Value = resp.json().await.unwrap();
        let member_ids: Vec<&str> = members
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["user_id"].as_str().unwrap())
            .collect();
        assert!(!member_ids.contains(&ctx.operator_user_id.to_string().as_str()));

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_team_permission_revoked_on_member_removal() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Create team, add operator, grant team permission
        let team_id = ctx
            .create_team("Revoke-Team", vec![ctx.operator_user_id])
            .await;
        ctx.grant_team_permission(app_id, team_id, "operate").await;

        // Operator can start (via team)
        let resp = ctx
            .get_as(
                "operator",
                &format!("/api/v1/apps/{app_id}/permissions/effective"),
            )
            .await;
        let eff: Value = resp.json().await.unwrap();
        assert_eq!(eff["permission_level"], "operate");

        // Remove operator from team
        ctx.delete_as(
            "admin",
            &format!("/api/v1/teams/{team_id}/members/{}", ctx.operator_user_id),
        )
        .await;

        // Operator should lose team permission
        let resp = ctx
            .get_as(
                "operator",
                &format!("/api/v1/apps/{app_id}/permissions/effective"),
            )
            .await;
        let eff: Value = resp.json().await.unwrap();
        assert_ne!(
            eff["permission_level"], "operate",
            "Operator should lose access after team removal"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_team_with_expiry() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let team_id = ctx
            .create_team("Expiry-Team", vec![ctx.operator_user_id])
            .await;

        // Grant team permission with expiry in the past
        ctx.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/teams"),
            json!({
                "team_id": team_id,
                "permission_level": "operate",
                "expires_at": "2020-01-01T00:00:00Z",
            }),
        )
        .await;

        // Should be denied (expired)
        let resp = ctx
            .post_as(
                "operator",
                &format!("/api/v1/apps/{app_id}/start"),
                json!({}),
            )
            .await;
        assert_eq!(
            resp.status(),
            403,
            "Expired team permission should be denied"
        );

        ctx.cleanup().await;
    }
}
