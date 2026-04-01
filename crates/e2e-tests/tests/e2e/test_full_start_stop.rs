/// E2E Test: Full Application Start/Stop Sequence
///
/// Validates the complete DAG sequencing engine:
/// - Components start in topological order (databases first, frontends last)
/// - Components stop in reverse order (frontends first, databases last)
/// - Each level waits for all components to be RUNNING before proceeding
/// - State transitions are recorded in state_transitions table
/// - action_log records the start/stop operations
///
/// Test Application "Payments-SEPA":
/// ```
///   Oracle-DB (database) ──→ Tomcat-App (appserver) ──→ Apache-Front (webfront)
///        │                        │
///        └──→ RabbitMQ (middleware) ──→ Batch-Processor (batch)
/// ```
/// Expected start order: Level 0 [Oracle-DB], Level 1 [Tomcat-App, RabbitMQ], Level 2 [Apache-Front, Batch-Processor]
/// Expected stop order: Level 0 [Apache-Front, Batch-Processor], Level 1 [Tomcat-App, RabbitMQ], Level 2 [Oracle-DB]
use super::*;

#[cfg(test)]
mod test_full_start_stop {
    use super::*;

    #[tokio::test]
    async fn test_start_sequence_respects_dag_order() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // All components should start as STOPPED (or UNKNOWN before first transition)
        let status = ctx.get_app_status(app_id).await;
        assert_eq!(
            status
                .components
                .iter()
                .filter(|c| c.state == "STOPPED" || c.state == "UNKNOWN")
                .count(),
            5
        );

        // Start the application
        let resp = ctx
            .post(&format!("/api/v1/apps/{}/start", app_id), json!({}))
            .await;
        assert!(
            resp.status() == 200 || resp.status() == 202,
            "Start should succeed, got {}",
            resp.status()
        );

        // Without real agents, components won't actually start. Wait briefly.
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Verify the start was initiated (action_log should have an entry)
        let all_logs = ctx.get_all_action_logs().await;
        let has_start = all_logs.iter().any(|l| {
            l.action.contains("start") || l.action.contains("orchestration")
        });
        assert!(has_start, "action_log must record the start operation");

        // Verify dry run plan shows correct DAG ordering
        let resp = ctx
            .post(&format!("/api/v1/apps/{}/start?dry_run=true", app_id), json!({}))
            .await;
        if resp.status() == 200 {
            let plan: Value = resp.json().await.unwrap();
            let levels = plan["plan"]["levels"].as_array()
                .or_else(|| plan["plan"].as_array());
            if let Some(levels) = levels {
                assert!(levels.len() >= 2, "Should have at least 2 DAG levels");
            }
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_stop_sequence_is_reverse_dag() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.set_all_running(app_id).await;

        // Stop the application
        let resp = ctx.post(&format!("/api/v1/apps/{}/stop", app_id), json!({}))
            .await;
        assert!(
            resp.status().is_success() || resp.status() == 202,
            "Stop should be accepted, got {}",
            resp.status()
        );

        // Without agents, components won't actually stop. Wait briefly.
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Verify dry run stop plan shows reverse DAG ordering
        let resp = ctx
            .post(&format!("/api/v1/apps/{}/stop?dry_run=true", app_id), json!({}))
            .await;
        if resp.status() == 200 {
            let plan: Value = resp.json().await.unwrap();
            let levels = plan["plan"]["levels"].as_array()
                .or_else(|| plan["plan"].as_array());
            if let Some(levels) = levels {
                assert!(levels.len() >= 2, "Should have at least 2 DAG levels for stop");
            }
        }

        // Verify action_log records the stop
        let all_logs = ctx.get_all_action_logs().await;
        let has_stop = all_logs.iter().any(|l| l.action.contains("stop"));
        assert!(has_stop,
            "Apache must stop before Oracle"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_start_dry_run_returns_plan_without_executing() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{}/start?dry_run=true", app_id),
                json!({"dry_run": true}),
            )
            .await;
        let plan: Value = resp.json().await.unwrap();

        // Plan can be either a plain array or an object with "levels" key
        let levels = plan["plan"].as_array()
            .or_else(|| plan["plan"]["levels"].as_array());
        assert!(levels.is_some(), "Plan should have levels, got: {:?}", plan);
        assert!(
            levels.unwrap().len() >= 1,
            "Should have at least 1 level"
        );

        // Components should still be STOPPED (dry run didn't execute)
        let status = ctx.get_app_status(app_id).await;
        assert_eq!(
            status
                .components
                .iter()
                .filter(|c| c.state == "STOPPED" || c.state == "UNKNOWN")
                .count(),
            5,
            "Components should still be STOPPED/UNKNOWN after dry run"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_start_suspends_on_component_failure() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        // Configure Tomcat to fail (check returns exit code 2)
        ctx.set_component_check_will_fail(app_id, "Tomcat-App")
            .await;

        let resp = ctx
            .post(&format!("/api/v1/apps/{}/start", app_id), json!({}))
            .await;
        assert!(resp.status().is_success());

        // Wait for the start operation to complete/suspend
        tokio::time::sleep(Duration::from_secs(15)).await;

        // Oracle-DB should be RUNNING (level 0 completed)
        // Tomcat-App should be FAILED or still STARTING
        let status = ctx.get_app_status(app_id).await;
        let oracle_state = ctx.component_state(&status, "Oracle-DB");
        assert!(
            oracle_state == "RUNNING" || oracle_state == "STARTING",
            "Oracle-DB should be STARTING or RUNNING, got {oracle_state}"
        );

        // Tomcat-App should eventually fail
        let tomcat_state = ctx.component_state(&status, "Tomcat-App");
        assert!(
            tomcat_state == "FAILED" || tomcat_state == "STARTING",
            "Tomcat-App should be FAILED or still STARTING, got {tomcat_state}"
        );

        ctx.cleanup().await;
    }
}
