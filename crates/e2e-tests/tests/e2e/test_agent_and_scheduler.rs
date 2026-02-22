/// E2E Test: Agent Offline Buffer + Replay
use super::*;

#[cfg(test)]
mod test_agent_offline {
    use super::*;

    #[tokio::test]
    async fn test_agent_buffers_during_disconnect_and_replays() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Simulate agent disconnect
        ctx.disconnect_agent("srv-oracle-01").await;

        // Component should become UNREACHABLE after heartbeat timeout
        tokio::time::sleep(Duration::from_secs(95)).await; // 3 cycles * 30s + margin
        let status = ctx.get_component_state(app_id, "Oracle-DB").await;
        assert_eq!(status, "UNREACHABLE");

        // Reconnect agent
        ctx.reconnect_agent("srv-oracle-01").await;
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Agent should have replayed buffered checks
        // Component should return to its real state
        let status = ctx.get_component_state(app_id, "Oracle-DB").await;
        assert_ne!(
            status, "UNREACHABLE",
            "Should have recovered from UNREACHABLE"
        );

        // Verify state_transitions recorded: * → UNREACHABLE → (previous state)
        let transitions = ctx.get_state_transitions_for(app_id, "Oracle-DB").await;
        assert!(transitions.iter().any(|t| t.to_state == "UNREACHABLE"));

        ctx.cleanup().await;
    }
}

/// E2E Test: Scheduler Integration (appctl / API key)

#[cfg(test)]
mod test_scheduler_integration {
    use super::*;

    #[tokio::test]
    async fn test_api_key_start_wait() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let api_key = ctx
            .create_api_key("Control-M", vec!["start", "stop", "status"])
            .await;

        // Start via API key (simulating Control-M)
        let resp = ctx
            .post_with_api_key(
                &api_key,
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Wait-running (long poll, simulating appctl --wait)
        let resp = ctx
            .get_with_api_key_timeout(
                &api_key,
                &format!("/api/v1/apps/{}/wait-running", app_id),
                Duration::from_secs(60),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "RUNNING");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_api_key_permissions_enforced() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // API key with only "status" permission
        let api_key = ctx.create_api_key("ReadOnly", vec!["status"]).await;

        // Status OK
        let resp = ctx
            .get_with_api_key(&api_key, &format!("/api/v1/apps/{}/status", app_id))
            .await;
        assert_eq!(resp.status(), 200);

        // Start should be denied
        let resp = ctx
            .post_with_api_key(
                &api_key,
                &format!("/api/v1/apps/{}/start", app_id),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 403);

        ctx.cleanup().await;
    }
}
