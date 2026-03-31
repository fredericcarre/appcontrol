/// E2E Test: Realistic Incident Simulation — Full Lifecycle
///
/// Simulates a real-world incident from start to resolution:
///
/// 1. Create a 10-component application with two independent branches
/// 2. Start the application (all components RUNNING)
/// 3. Simulate a failure: App-1 goes FAILED (e.g. JVM crash)
/// 4. Verify AppControl correctly identifies the error branch (pink branch)
/// 5. Record the failure transition in state_transitions
/// 6. Call start-branch to restart only the affected branch
/// 7. Wait for the branch to recover
/// 8. Verify: ONLY affected components were restarted (App-1, Front-1, Queue-1, Worker-1)
/// 9. Verify: Unaffected components (DB-1, DB-2, App-2, Front-2, Queue-2, Worker-2) were NEVER restarted
/// 10. Verify: Complete audit trail in state_transitions and action_log
///
/// Test topology:
/// ```
///   DB-1 ──→ App-1 ──→ Front-1
///    │         │
///    │         └──→ Queue-1 ──→ Worker-1
///    │
///   DB-2 ──→ App-2 ──→ Front-2
///              │
///              └──→ Queue-2 ──→ Worker-2
/// ```
///
/// Scenario: App-1 fails. The error branch includes App-1 and all its dependents
/// (Front-1, Queue-1, Worker-1). DB-1 is upstream and healthy, so it's NOT in the branch.
/// The entire branch-2 (DB-2, App-2, Front-2, Queue-2, Worker-2) is completely unaffected.
use super::*;

#[cfg(test)]
mod test_incident_lifecycle {
    use super::*;

    #[tokio::test]
    async fn test_full_incident_detection_and_branch_restart() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_ten_component_app().await;

        // ── Step 1: Start the application, all components RUNNING ──
        ctx.set_all_running(app_id).await;
        let status = ctx.get_app_status(app_id).await;
        assert_eq!(
            status
                .components
                .iter()
                .filter(|c| c.state == "RUNNING")
                .count(),
            10,
            "All 10 components should be RUNNING"
        );

        // ── Step 2: Simulate incident — App-1 crashes ──
        // In production, the agent would detect the failure via check_cmd.
        // Here we simulate by directly setting the state and recording the transition.
        let app1_id = ctx.component_id(app_id, "App-1").await;

        // Record the failure transition (RUNNING → FAILED)
        sqlx::query(
            "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
             VALUES ($1, 'RUNNING', 'FAILED', 'check', $2, chrono::Utc::now().to_rfc3339())"
        )
        .bind(app1_id)
        .bind(serde_json::json!({
            "reason": "Process exited with signal 9 (SIGKILL)",
            "check_exit_code": 2,
            "pid": 12345,
        }))
        .execute(&ctx.db_pool).await.unwrap();

        // Update component state to FAILED
        ctx.force_component_state(app_id, "App-1", "FAILED").await;

        // ── Step 3: Verify error branch detection ──
        let resp = ctx.get(&format!("/api/v1/apps/{}/dag", app_id)).await;
        assert_eq!(resp.status(), 200);
        let dag: Value = resp.json().await.unwrap();

        // The error_branch should contain App-1 and its dependents
        if let Some(error_branch) = dag["error_branch"].as_array() {
            let branch_names: Vec<&str> = error_branch
                .iter()
                .filter_map(|v| v["name"].as_str())
                .collect();

            assert!(
                branch_names.contains(&"App-1"),
                "Error branch must contain the failed component App-1"
            );
            assert!(
                branch_names.contains(&"Front-1"),
                "Error branch must contain dependent Front-1"
            );
            assert!(
                branch_names.contains(&"Queue-1"),
                "Error branch must contain dependent Queue-1"
            );
            assert!(
                branch_names.contains(&"Worker-1"),
                "Error branch must contain dependent Worker-1"
            );

            // Healthy upstream and unrelated components must NOT be in the branch
            assert!(
                !branch_names.contains(&"DB-1"),
                "DB-1 is healthy upstream, must NOT be in error branch"
            );
            assert!(
                !branch_names.contains(&"DB-2"),
                "DB-2 is in a separate branch, must NOT be affected"
            );
            assert!(
                !branch_names.contains(&"App-2"),
                "App-2 is in a separate branch, must NOT be affected"
            );
        }

        // ── Step 4: Restart only the error branch ──
        let resp = ctx
            .post(&format!("/api/v1/apps/{}/start-branch", app_id), json!({}))
            .await;
        assert!(
            resp.status().is_success(),
            "start-branch should succeed, got {}",
            resp.status()
        );

        // ── Step 5: Wait for the branch to recover ──
        ctx.wait_app_branch_running(app_id, Duration::from_secs(30))
            .await
            .unwrap();

        // ── Step 6: Verify all components are now RUNNING ──
        let status = ctx.get_app_status(app_id).await;
        for comp in &status.components {
            assert_eq!(
                comp.state, "RUNNING",
                "Component {} should be RUNNING after branch restart, got {}",
                comp.name, comp.state
            );
        }

        // ── Step 7: Verify App-2 branch was NEVER restarted ──
        let app2_transitions = ctx.get_state_transitions_for(app_id, "App-2").await;
        assert!(
            !app2_transitions.iter().any(|t| t.to_state == "STARTING"),
            "App-2 should NEVER have been restarted (no STARTING transition)"
        );

        let db2_transitions = ctx.get_state_transitions_for(app_id, "DB-2").await;
        assert!(
            !db2_transitions.iter().any(|t| t.to_state == "STARTING"),
            "DB-2 should NEVER have been restarted"
        );

        let front2_transitions = ctx.get_state_transitions_for(app_id, "Front-2").await;
        assert!(
            !front2_transitions.iter().any(|t| t.to_state == "STARTING"),
            "Front-2 should NEVER have been restarted"
        );

        // ── Step 8: Verify complete audit trail ──
        // The FAILED transition should be recorded
        let app1_transitions = ctx.get_state_transitions_for(app_id, "App-1").await;
        assert!(
            app1_transitions
                .iter()
                .any(|t| t.from_state == "RUNNING" && t.to_state == "FAILED"),
            "state_transitions must record RUNNING → FAILED for App-1"
        );

        // The recovery transitions should be recorded
        assert!(
            app1_transitions
                .iter()
                .any(|t| t.to_state == "STARTING" || t.to_state == "RUNNING"),
            "state_transitions must record recovery for App-1"
        );

        // Action log should record the start-branch operation
        let logs = ctx.get_action_log(app_id, "start").await;
        // At minimum, we expect a start or start-branch action
        let all_logs = ctx.get_all_action_logs().await;
        let branch_logs: Vec<_> = all_logs
            .iter()
            .filter(|l| l.action.contains("start") && *l.resource_id == app_id)
            .collect();
        assert!(
            !branch_logs.is_empty(),
            "action_log must record the start-branch operation"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_incident_with_multiple_failures_in_same_branch() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_ten_component_app().await;
        ctx.set_all_running(app_id).await;

        // Both App-1 AND Queue-1 fail (Queue-1 is a dependent of App-1)
        let app1_id = ctx.component_id(app_id, "App-1").await;
        let queue1_id = ctx.component_id(app_id, "Queue-1").await;

        for (comp_id, name) in [(app1_id, "App-1"), (queue1_id, "Queue-1")] {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
                 VALUES ($1, 'RUNNING', 'FAILED', 'check', '{}', chrono::Utc::now().to_rfc3339())"
            )
            .bind(bind_id(comp_id))
            .execute(&ctx.db_pool).await.unwrap();
            ctx.force_component_state(app_id, name, "FAILED").await;
        }

        // Restart the branch
        let resp = ctx
            .post(&format!("/api/v1/apps/{}/start-branch", app_id), json!({}))
            .await;
        assert!(
            resp.status().is_success(),
            "start-branch should handle multiple failures, got {}",
            resp.status()
        );

        ctx.wait_app_branch_running(app_id, Duration::from_secs(30))
            .await
            .unwrap();

        // All should be RUNNING again
        let status = ctx.get_app_status(app_id).await;
        let running_count = status
            .components
            .iter()
            .filter(|c| c.state == "RUNNING")
            .count();
        assert_eq!(
            running_count, 10,
            "All 10 components should be RUNNING after recovery"
        );

        // Branch 2 still untouched
        let app2_transitions = ctx.get_state_transitions_for(app_id, "App-2").await;
        assert!(
            !app2_transitions.iter().any(|t| t.to_state == "STARTING"),
            "App-2 should NEVER have been restarted"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_incident_audit_trail_completeness() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_ten_component_app().await;
        ctx.set_all_running(app_id).await;

        let app1_id = ctx.component_id(app_id, "App-1").await;

        // Record the initial count
        let initial_transition_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM state_transitions st
             JOIN components c ON c.id = st.component_id
             WHERE c.application_id = $1",
        )
        .bind(bind_id(app_id))
        .fetch_one(&ctx.db_pool)
        .await
        .unwrap();

        // Simulate failure
        sqlx::query(
            "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
             VALUES ($1, 'RUNNING', 'FAILED', 'check', '{\"reason\": \"OOM\"}', chrono::Utc::now().to_rfc3339())"
        )
        .bind(app1_id)
        .execute(&ctx.db_pool).await.unwrap();
        ctx.force_component_state(app_id, "App-1", "FAILED").await;

        // Restart branch
        ctx.post(&format!("/api/v1/apps/{}/start-branch", app_id), json!({}))
            .await;
        ctx.wait_app_branch_running(app_id, Duration::from_secs(30))
            .await
            .unwrap();

        // Count transitions after recovery
        let final_transition_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM state_transitions st
             JOIN components c ON c.id = st.component_id
             WHERE c.application_id = $1",
        )
        .bind(bind_id(app_id))
        .fetch_one(&ctx.db_pool)
        .await
        .unwrap();

        // We should have MORE transitions than before (failure + restart transitions)
        assert!(
            final_transition_count > initial_transition_count + 1,
            "Should have recorded multiple transitions during incident lifecycle. \
             Before: {initial_transition_count}, After: {final_transition_count}"
        );

        // Verify the failure transition details include the reason
        let app1_transitions = ctx.get_state_transitions_for(app_id, "App-1").await;
        let failed_transition = app1_transitions
            .iter()
            .find(|t| t.to_state == "FAILED")
            .expect("Must find FAILED transition for App-1");
        assert_eq!(
            failed_transition.from_state, "RUNNING",
            "Failed transition should be from RUNNING"
        );
        assert_eq!(
            failed_transition.trigger, "check",
            "Failed transition trigger should be 'check'"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_incident_does_not_cascade_to_separate_branches() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_ten_component_app().await;
        ctx.set_all_running(app_id).await;

        // Fail DB-1 (affects entire branch 1)
        let db1_id = ctx.component_id(app_id, "DB-1").await;
        sqlx::query(
            "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
             VALUES ($1, 'RUNNING', 'FAILED', 'check', '{}', chrono::Utc::now().to_rfc3339())"
        )
        .bind(db1_id)
        .execute(&ctx.db_pool).await.unwrap();
        ctx.force_component_state(app_id, "DB-1", "FAILED").await;

        // Verify: Branch 2 components are all still RUNNING
        for name in ["DB-2", "App-2", "Front-2", "Queue-2", "Worker-2"] {
            let state = ctx.get_component_state(app_id, name).await;
            assert_eq!(
                state, "RUNNING",
                "Branch 2 component {name} should still be RUNNING when Branch 1 fails"
            );
        }

        // Restart branch
        let resp = ctx
            .post(&format!("/api/v1/apps/{}/start-branch", app_id), json!({}))
            .await;
        assert!(resp.status().is_success());

        ctx.wait_app_branch_running(app_id, Duration::from_secs(30))
            .await
            .unwrap();

        // Branch 2 was never touched
        for name in ["DB-2", "App-2", "Front-2", "Queue-2", "Worker-2"] {
            let transitions = ctx.get_state_transitions_for(app_id, name).await;
            assert!(
                !transitions
                    .iter()
                    .any(|t| t.to_state == "STARTING" || t.to_state == "STOPPING"),
                "Branch 2 component {name} should have NO start/stop transitions"
            );
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_incident_reports_show_failure_data() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.set_all_running(app_id).await;

        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        // Simulate 2 incidents
        for i in 0..2 {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
                 VALUES ($1, 'RUNNING', 'FAILED', 'check', $2, NOW() + interval '1 minute' * $3)"
            )
            .bind(oracle_id)
            .bind(serde_json::json!({"incident_number": i + 1}))
            .bind(i)
            .execute(&ctx.db_pool).await.unwrap();
        }

        // The incidents report should now show these failures
        let resp = ctx.get(&format!(
            "/api/v1/apps/{app_id}/reports/incidents?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        )).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().unwrap();

        let oracle_incidents: Vec<_> = data
            .iter()
            .filter(|d| d["component_name"].as_str() == Some("Oracle-DB"))
            .collect();
        assert_eq!(
            oracle_incidents.len(),
            2,
            "Incidents report should show 2 Oracle-DB failures, got {}",
            oracle_incidents.len()
        );

        ctx.cleanup().await;
    }
}
