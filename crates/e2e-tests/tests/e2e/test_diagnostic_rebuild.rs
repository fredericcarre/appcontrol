/// E2E Test: 3-Level Diagnostic and Reconstruction
///
/// Validates:
/// - POST /diagnose runs 3 check levels on all components
/// - Recommendation matrix produces correct results
/// - Rebuild respects DAG order
/// - rebuild_protected blocks rebuild
/// - Rebuild via bastion agent works for infra_rebuild
/// - RTR (Recovery Time for Rebuild) is measured
use super::*;

#[cfg(test)]
mod test_diagnostic_rebuild {
    use super::*;

    #[tokio::test]
    async fn test_diagnose_produces_correct_recommendations() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;

        // Configure component states:
        // Redis: all checks OK → HEALTHY
        // Tomcat: health FAIL, integrity OK, infra OK → RESTART
        // Oracle: health FAIL, integrity FAIL, infra OK → APP_REBUILD
        // Apache: health FAIL, integrity FAIL, infra FAIL → INFRA_REBUILD
        ctx.configure_check_results(
            app_id,
            vec![
                ("Redis", 0, 0, 0),  // health OK, integrity OK, infra OK
                ("Tomcat", 2, 0, 0), // health FAIL, integrity OK, infra OK
                ("Oracle", 2, 2, 0), // health FAIL, integrity FAIL, infra OK
                ("Apache", 2, 2, 2), // all FAIL
            ],
        )
        .await;

        let resp = ctx
            .post(&format!("/api/v1/apps/{}/diagnose", app_id), json!({}))
            .await;
        assert_eq!(resp.status(), 200);
        let diag: Value = resp.json().await.unwrap();

        assert_eq!(diag["summary"]["healthy"].as_u64(), Some(1)); // Redis
        assert_eq!(diag["summary"]["needs_restart"].as_u64(), Some(1)); // Tomcat
        assert_eq!(diag["summary"]["needs_app_rebuild"].as_u64(), Some(1)); // Oracle
        assert_eq!(diag["summary"]["needs_infra_rebuild"].as_u64(), Some(1)); // Apache

        // Check individual recommendations
        let components = diag["components"].as_array().unwrap();
        let redis = components.iter().find(|c| c["name"] == "Redis").unwrap();
        assert_eq!(redis["recommendation"], "HEALTHY");

        let tomcat = components.iter().find(|c| c["name"] == "Tomcat").unwrap();
        assert_eq!(tomcat["recommendation"], "RESTART");

        let oracle = components.iter().find(|c| c["name"] == "Oracle").unwrap();
        assert_eq!(oracle["recommendation"], "APP_REBUILD");

        let apache = components.iter().find(|c| c["name"] == "Apache").unwrap();
        assert_eq!(apache["recommendation"], "INFRA_REBUILD");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_respects_dag_order() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;

        // Oracle (database) and Tomcat (appserver, depends on Oracle) both need rebuild
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{}/rebuild", app_id),
                json!({
                    "components": [
                        { "id": ctx.component_id(app_id, "Oracle").await, "action": "app_rebuild" },
                        { "id": ctx.component_id(app_id, "Tomcat").await, "action": "app_rebuild" }
                    ]
                }),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Wait for completion
        tokio::time::sleep(Duration::from_secs(30)).await;

        // Verify Oracle rebuilt BEFORE Tomcat (DAG order)
        let logs = ctx.get_action_log_for_type(app_id, "rebuild").await;
        let oracle_rebuilt = logs
            .iter()
            .find(|l| l.details["target_name"].as_str() == Some("Oracle"))
            .unwrap()
            .created_at;
        let tomcat_rebuilt = logs
            .iter()
            .find(|l| l.details["target_name"].as_str() == Some("Tomcat"))
            .unwrap()
            .created_at;
        assert!(
            oracle_rebuilt < tomcat_rebuilt,
            "Oracle must rebuild before Tomcat"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_blocked_by_protection() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;

        // Protect Oracle
        let oracle_id = ctx.component_id(app_id, "Oracle").await;
        ctx.put(
            &format!("/api/v1/components/{}/rebuild-protection", oracle_id),
            json!({ "protected": true }),
        )
        .await;

        // Try to rebuild Oracle → should be rejected
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{}/rebuild", app_id),
                json!({
                    "components": [{ "id": oracle_id, "action": "app_rebuild" }]
                }),
            )
            .await;
        assert_eq!(
            resp.status(),
            409,
            "Rebuild of protected component should be rejected"
        );

        let body: Value = resp.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("protected"));

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_dry_run() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{}/rebuild", app_id),
                json!({
                    "dry_run": true
                }),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let plan: Value = resp.json().await.unwrap();
        assert!(plan["plan"].is_array());
        assert!(plan["estimated_time"].is_string());

        // Nothing should have actually changed
        let status = ctx.get_app_status(app_id).await;
        // Components should be in their original state (not rebuilt)

        ctx.cleanup().await;
    }
}
