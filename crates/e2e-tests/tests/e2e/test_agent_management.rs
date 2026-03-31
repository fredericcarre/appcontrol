/// E2E Test: Agent Management — Listing, Labels, Heartbeat Status
use super::*;

#[cfg(test)]
mod test_agent_management {
    use super::*;

    #[tokio::test]
    async fn test_list_agents() {
        let ctx = TestContext::new().await;

        let resp = ctx.get("/api/v1/agents").await;
        assert_eq!(resp.status(), 200);
        let agents: Value = resp.json().await.unwrap();
        assert!(agents.is_array());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_get_nonexistent_agent_returns_404() {
        let ctx = TestContext::new().await;
        let fake_id = Uuid::new_v4();

        let resp = ctx.get(&format!("/api/v1/agents/{fake_id}")).await;
        assert_eq!(resp.status(), 404);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_agent_heartbeat_tracking() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Simulate agent registration by inserting directly
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, hostname, labels, version, last_heartbeat_at)
             VALUES ($1, $2, 'srv-oracle', '{\"role\": \"database\", \"env\": \"prod\"}', '0.1.0', chrono::Utc::now().to_rfc3339())"
        )
        .bind(agent_id)
        .bind(ctx.organization_id)
        .execute(&ctx.db_pool)
        .await
        .unwrap();

        // Get agent
        let resp = ctx.get(&format!("/api/v1/agents/{agent_id}")).await;
        assert_eq!(resp.status(), 200);
        let agent: Value = resp.json().await.unwrap();
        assert_eq!(agent["hostname"], "srv-oracle");
        assert_eq!(agent["labels"]["role"], "database");
        assert_eq!(agent["labels"]["env"], "prod");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_agent_list_filters_by_org() {
        let ctx = TestContext::new().await;

        // Insert agent for org1
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, hostname, labels, version, last_heartbeat_at)
             VALUES ($1, $2, 'srv-org1', '{}', '0.1.0', chrono::Utc::now().to_rfc3339())",
        )
        .bind(agent_id)
        .bind(ctx.organization_id)
        .execute(&ctx.db_pool)
        .await
        .unwrap();

        // Org2 should not see org1's agent
        let (org2_id, _, org2_token) = ctx.create_second_org().await;
        let resp = ctx.get_with_token(&org2_token, "/api/v1/agents").await;
        let agents: Value = resp.json().await.unwrap();
        let hostnames: Vec<&str> = agents
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["hostname"].as_str())
            .collect();
        assert!(
            !hostnames.contains(&"srv-org1"),
            "Org2 should not see Org1's agents"
        );

        ctx.cleanup().await;
    }
}
