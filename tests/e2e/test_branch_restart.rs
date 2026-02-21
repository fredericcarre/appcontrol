/// E2E Test: Error Branch Detection and Selective Restart
///
/// Validates:
/// - Branch detection finds FAILED components + their dependents
/// - Selective restart only affects the error branch
/// - Healthy components are NOT restarted
/// - "Pink branch" state is correctly computed
///
/// Test app (10 components):
/// ```
///   DB-1 ──→ App-1 ──→ Front-1
///    │         │
///    │         └──→ Queue-1 ──→ Worker-1
///    │
///   DB-2 ──→ App-2 ──→ Front-2
///              │
///              └──→ Queue-2 ──→ Worker-2
/// ```
/// Scenario: App-1 goes FAILED. Branch = [App-1, Front-1, Queue-1, Worker-1].
/// DB-1 is NOT in the branch (it's healthy). DB-2/App-2/Front-2/Queue-2/Worker-2 untouched.

#[cfg(test)]
mod test_branch_restart {
    use super::*;

    #[tokio::test]
    async fn test_detect_error_branch() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_ten_component_app().await;
        ctx.set_all_running(app_id).await;

        // Make App-1 fail
        ctx.force_component_state(app_id, "App-1", "FAILED").await;

        // Get DAG with branch info
        let dag = ctx.get(&format!("/api/v1/apps/{}/dag", app_id)).await.json::<Value>().await;
        let error_branch: Vec<String> = dag["error_branch"].as_array().unwrap()
            .iter().map(|v| v["name"].as_str().unwrap().to_string()).collect();

        assert!(error_branch.contains(&"App-1".to_string()));
        assert!(error_branch.contains(&"Front-1".to_string()));
        assert!(error_branch.contains(&"Queue-1".to_string()));
        assert!(error_branch.contains(&"Worker-1".to_string()));
        assert!(!error_branch.contains(&"DB-1".to_string()), "DB-1 is healthy, should not be in branch");
        assert!(!error_branch.contains(&"App-2".to_string()), "App-2 is in a different branch");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_restart_branch_only_restarts_affected() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_ten_component_app().await;
        ctx.set_all_running(app_id).await;
        ctx.force_component_state(app_id, "App-1", "FAILED").await;

        // Restart the error branch
        let resp = ctx.post(&format!("/api/v1/apps/{}/start-branch", app_id), json!({})).await;
        assert_eq!(resp.status(), 200);

        ctx.wait_app_branch_running(app_id, Duration::from_secs(30)).await.unwrap();

        // Verify: App-1 went through STARTING → RUNNING
        let transitions = ctx.get_state_transitions_for(app_id, "App-1").await;
        assert!(transitions.iter().any(|t| t.to_state == "STARTING"));
        assert!(transitions.iter().any(|t| t.to_state == "RUNNING"));

        // Verify: App-2 was NEVER restarted (no STARTING transition)
        let app2_transitions = ctx.get_state_transitions_for(app_id, "App-2").await;
        assert!(!app2_transitions.iter().any(|t| t.to_state == "STARTING"),
            "App-2 should not have been restarted");

        ctx.cleanup().await;
    }
}
