/// E2E Test: Custom Commands Execution + Audit Trail

#[cfg(test)]
mod test_custom_commands {
    use super::*;

    #[tokio::test]
    async fn test_execute_custom_command_returns_output() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        // Create a custom command
        ctx.post(&format!("/api/v1/apps/{}/components/{}", app_id, oracle_id), json!({
            "commands": [{ "name": "count_records", "display_name": "Count Records",
                          "command": "echo '{\"count\": 42}'", "category": "diagnostic",
                          "requires_confirmation": false, "timeout_seconds": 30 }]
        })).await;

        // Execute it (as operator)
        let resp = ctx.post_as("operator", &format!("/api/v1/components/{}/command/count_records", oracle_id), json!({})).await;
        assert_eq!(resp.status(), 200);
        let result: Value = resp.json().await;
        assert_eq!(result["exit_code"].as_i64(), Some(0));
        assert!(result["stdout"].as_str().unwrap().contains("42"));

        // Verify audit trail
        let logs = ctx.get_action_log_for_type(app_id, "command").await;
        assert!(!logs.is_empty());
        let log = &logs[0];
        assert_eq!(log.action, "command");
        assert!(log.details["command_name"].as_str() == Some("count_records"));

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_command_requires_confirmation_blocks_without_flag() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        // Create a dangerous command requiring confirmation
        ctx.create_command(oracle_id, "purge_all", "rm -rf /tmp/cache", true).await;

        // Execute without confirmation → should fail
        let resp = ctx.post_as("operator",
            &format!("/api/v1/components/{}/command/purge_all", oracle_id), json!({})).await;
        assert_eq!(resp.status(), 400, "Should require confirmation");

        // Execute with confirmation → should succeed
        let resp = ctx.post_as("operator",
            &format!("/api/v1/components/{}/command/purge_all", oracle_id),
            json!({ "confirmed": true })).await;
        assert_eq!(resp.status(), 200);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_command_rbac_viewer_cannot_execute() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        // Viewer tries to execute command → 403
        let resp = ctx.post_as("viewer",
            &format!("/api/v1/components/{}/command/count_records", oracle_id), json!({})).await;
        assert_eq!(resp.status(), 403);

        ctx.cleanup().await;
    }
}
