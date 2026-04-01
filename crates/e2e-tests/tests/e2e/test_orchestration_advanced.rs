/// E2E Test: Orchestration / Scheduler Integration — Advanced
///
/// Validates:
/// - wait-running with timeout
/// - wait-running returns FAILED if component fails
/// - Status endpoint returns component list with all_running flag
/// - Concurrent start on same app is handled gracefully
/// - Stop dry run
/// - API key CRUD
use super::*;

#[cfg(test)]
mod test_orchestration_advanced {
    use super::*;

    #[tokio::test]
    async fn test_status_endpoint_structure() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx
            .get(&format!("/api/v1/orchestration/apps/{app_id}/status"))
            .await;
        assert_eq!(resp.status(), 200);

        let status: Value = resp.json().await.unwrap();
        assert!(
            status["components"].is_array(),
            "Status should have components array"
        );

        let components = status["components"].as_array().unwrap();
        assert_eq!(components.len(), 5, "Payments app has 5 components");

        // Each component should have name and state
        for comp in components {
            assert!(comp["name"].is_string());
            assert!(comp["state"].is_string());
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_wait_running_returns_timeout() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Don't start the app, just wait → should timeout
        let resp = ctx
            .get(&format!(
                "/api/v1/orchestration/apps/{app_id}/wait-running?timeout=2"
            ))
            .await;

        // Should return timeout status or 408
        let status_code = resp.status();
        let body: Value = resp.json().await.unwrap();
        let status_str = body["status"].as_str().unwrap_or("");
        assert!(
            status_str == "TIMEOUT" || status_str == "timeout"
                || status_str == "STOPPED" || status_str == "stopped"
                || status_code == 408,
            "Should indicate timeout, got: {:?}",
            body
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_wait_running_returns_failed_on_failure() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.force_component_state(app_id, "Oracle-DB", "FAILED")
            .await;

        let resp = ctx
            .get(&format!(
                "/api/v1/orchestration/apps/{app_id}/wait-running?timeout=2"
            ))
            .await;

        let body: Value = resp.json().await.unwrap();
        let status_str = body["status"].as_str().unwrap_or("");
        assert!(
            status_str == "FAILED" || status_str == "failed"
                || body["all_running"].as_bool() == Some(false),
            "Should indicate failure when component is FAILED, got: {:?}",
            body
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_stop_dry_run() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.set_all_running(app_id).await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/stop?dry_run=true"),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let plan: Value = resp.json().await.unwrap();
        assert!(plan["plan"]["levels"].is_array() || plan["plan"].is_array(), "Stop dry run should return a plan, got: {:?}", plan);

        // Components should still be RUNNING
        let status = ctx.get_app_status(app_id).await;
        assert!(
            status.components.iter().all(|c| c.state == "RUNNING"),
            "Dry run should not change component states"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_api_key_crud() {
        let ctx = TestContext::new().await;

        // Create API key
        let resp = ctx
            .post(
                "/api/v1/api-keys",
                json!({
                    "name": "Test-Key",
                    "allowed_actions": ["start", "stop", "status"],
                }),
            )
            .await;
        assert!(resp.status().is_success());
        let key: Value = resp.json().await.unwrap();
        assert!(key["key"].is_string(), "Should return the API key string");
        assert!(key["id"].is_string(), "Should return the key ID");

        // List API keys
        let resp = ctx.get("/api/v1/api-keys").await;
        assert_eq!(resp.status(), 200);
        let keys: Value = resp.json().await.unwrap();
        assert!(!keys.as_array().unwrap().is_empty());

        // Delete API key
        let key_id = key["id"].as_str().unwrap();
        let resp = ctx
            .delete_as("admin", &format!("/api/v1/api-keys/{key_id}"))
            .await;
        assert!(resp.status() == 200 || resp.status() == 204, "Delete should succeed, got {}", resp.status());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_api_key_stop() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.set_all_running(app_id).await;

        let api_key = ctx
            .create_api_key("Scheduler", vec!["start", "stop", "status"])
            .await;

        // Stop via API key
        let resp = ctx
            .post_with_api_key(&api_key, &format!("/api/v1/apps/{app_id}/stop"), json!({}))
            .await;
        assert!(
            resp.status().is_success(),
            "API key with 'stop' action should be able to stop, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_orchestration_endpoints_via_api_key() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let api_key = ctx
            .create_api_key("Orchestrator", vec!["start", "stop", "status"])
            .await;

        // Orchestration status
        let resp = ctx
            .get_with_api_key(
                &api_key,
                &format!("/api/v1/orchestration/apps/{app_id}/status"),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Orchestration start
        let resp = ctx
            .post_with_api_key(
                &api_key,
                &format!("/api/v1/orchestration/apps/{app_id}/start"),
                json!({}),
            )
            .await;
        assert!(resp.status().is_success() || resp.status() == 202);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_concurrent_start_idempotent() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Start twice rapidly
        let resp1 = ctx
            .post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
            .await;
        let resp2 = ctx
            .post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
            .await;

        // First should succeed, second should either succeed (idempotent)
        // or return 409 (already running)
        assert!(resp1.status().is_success());
        assert!(
            resp2.status().is_success() || resp2.status() == 409,
            "Concurrent start should be idempotent or rejected, got {}",
            resp2.status()
        );

        ctx.cleanup().await;
    }
}
