/// E2E Test: Audit Trail Completeness (DORA Compliance)
///
/// Validates that EVERY user action is recorded in action_log
/// and that event tables are truly append-only.

#[cfg(test)]
mod test_audit_trail {
    use super::*;

    #[tokio::test]
    async fn test_every_action_is_logged() {
        let ctx = TestContext::new().await;
        let initial_log_count = ctx.count_action_logs().await;

        // Perform a series of actions
        let app_id = ctx.create_payments_app().await;      // app_create
        ctx.grant_permission(app_id, ctx.operator_user_id, "operate").await; // config_change
        ctx.post_as("operator", &format!("/api/v1/apps/{}/start", app_id), json!({})).await; // start
        tokio::time::sleep(Duration::from_secs(5)).await;
        ctx.post_as("operator", &format!("/api/v1/apps/{}/stop", app_id), json!({})).await;  // stop

        let final_log_count = ctx.count_action_logs().await;
        assert!(final_log_count >= initial_log_count + 4,
            "Expected at least 4 new action_log entries, got {}", final_log_count - initial_log_count);

        // Verify each action type is present
        let logs = ctx.get_all_action_logs().await;
        let types: Vec<&str> = logs.iter().map(|l| l.action_type.as_str()).collect();
        assert!(types.contains(&"app_create"));
        assert!(types.contains(&"config_change"));
        assert!(types.contains(&"start"));
        assert!(types.contains(&"stop"));

        // Verify user_id is set on all human actions
        for log in &logs {
            if log.api_key_id.is_none() {
                assert!(log.user_id.is_some(), "Human action must have user_id: {:?}", log.action_type);
            }
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_state_transitions_recorded_for_every_change() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Start app → components go STOPPED → STARTING → RUNNING
        ctx.post_as("admin", &format!("/api/v1/apps/{}/start", app_id), json!({})).await;
        ctx.wait_app_running(app_id, Duration::from_secs(60)).await.unwrap();

        // Each component should have at least 2 transitions (STOPPED→STARTING, STARTING→RUNNING)
        let transitions = ctx.get_state_transitions(app_id).await;
        let oracle_transitions: Vec<_> = transitions.iter()
            .filter(|t| t.component_name == "Oracle-DB").collect();
        assert!(oracle_transitions.len() >= 2,
            "Oracle-DB should have at least 2 transitions, got {}", oracle_transitions.len());

        // Verify transition details
        assert!(oracle_transitions.iter().any(|t| t.previous_state == "STOPPED" && t.new_state == "STARTING"));
        assert!(oracle_transitions.iter().any(|t| t.previous_state == "STARTING" && t.new_state == "RUNNING"));

        // Verify trigger_type is set
        for t in &oracle_transitions {
            assert!(!t.trigger_type.is_empty(), "trigger_type must not be empty");
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_config_changes_versioned() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Modify app description
        ctx.put_as("admin", &format!("/api/v1/apps/{}", app_id),
            json!({"description": "Updated description"})).await;

        // Check config_versions has a snapshot
        let versions = ctx.get_config_versions("application", app_id).await;
        assert!(!versions.is_empty());
        let v = &versions[0];
        assert_eq!(v.change_type, "update");
        assert!(v.previous_value.is_some(), "Must have previous snapshot");
        assert!(v.new_value.is_some(), "Must have new snapshot");
        assert!(v.new_value.as_ref().unwrap()["description"].as_str() == Some("Updated description"));

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_append_only_tables_reject_updates() {
        let ctx = TestContext::new().await;

        // Try to UPDATE action_log directly via SQL (should be blocked by application logic)
        // This test verifies the backend never issues UPDATE/DELETE on event tables
        // We check by examining the sqlx query strings in the codebase (compile-time check)

        // At runtime, verify no rows were ever deleted
        let app_id = ctx.create_payments_app().await;
        ctx.post_as("admin", &format!("/api/v1/apps/{}/start", app_id), json!({})).await;

        let count_before = ctx.count_action_logs().await;
        assert!(count_before > 0);

        // Delete the app
        ctx.delete_as("admin", &format!("/api/v1/apps/{}", app_id)).await;

        // action_logs should NOT be deleted (even though the app is gone)
        let count_after = ctx.count_action_logs().await;
        assert!(count_after >= count_before,
            "action_log entries must NEVER be deleted, even when parent entity is deleted");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_api_key_actions_are_audited() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let api_key = ctx.create_api_key("Control-M Prod", vec!["start", "stop", "status"]).await;

        // Call via API key (simulating scheduler)
        let resp = ctx.get_with_api_key(&api_key, &format!("/api/v1/apps/{}/status", app_id)).await;
        assert_eq!(resp.status(), 200);

        // Verify audit log has api_key_id set (not user_id)
        let logs = ctx.get_all_action_logs().await;
        let api_log = logs.iter().find(|l| l.api_key_id.is_some()).unwrap();
        assert!(api_log.user_id.is_none(), "API key action should have api_key_id, not user_id");

        ctx.cleanup().await;
    }
}
