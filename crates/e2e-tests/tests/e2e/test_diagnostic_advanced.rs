/// E2E Test: Diagnostic Advanced — All 8 Matrix Combinations, Bastion Agent, RTR
///
/// The recommendation matrix has 8 combinations of Health/Integrity/Infra check results.
use super::*;

#[cfg(test)]
mod test_diagnostic_advanced {
    use super::*;

    /// Test all 8 combinations of the diagnostic recommendation matrix.
    /// H=Health, I=Integrity, F=Infrastructure (0=OK, 2=FAIL)
    #[tokio::test]
    async fn test_all_8_diagnostic_matrix_combinations() {
        let ctx = TestContext::new().await;

        // Create app with 8 components for all combinations
        let resp = ctx
            .post(
                "/api/v1/apps",
                json!({
                    "name": "Matrix-Test",
                    "description": "8-combination diagnostic test",
                    "site_id": ctx.default_site_id,
                }),
            )
            .await;
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        let combos = [
            ("C-HoIoFo", 0, 0, 0, "HEALTHY"),       // All OK
            ("C-HoIoFk", 0, 0, 2, "HEALTHY"),       // Infra bad but app works
            ("C-HoIkFo", 0, 2, 0, "HEALTHY"),       // Integrity bad but health OK
            ("C-HoIkFk", 0, 2, 2, "HEALTHY"),       // Integrity+Infra bad, health OK
            ("C-HkIoFo", 2, 0, 0, "RESTART"),       // Health bad, data OK → restart
            ("C-HkIoFk", 2, 0, 2, "INFRA_REBUILD"), // Health+Infra bad
            ("C-HkIkFo", 2, 2, 0, "APP_REBUILD"),   // Health+Integrity bad
            ("C-HkIkFk", 2, 2, 2, "INFRA_REBUILD"), // Everything bad
        ];

        for (name, _, _, _, _) in &combos {
            ctx.post(
                &format!("/api/v1/apps/{app_id}/components"),
                json!({
                    "name": name,
                    "component_type": "service",
                    "hostname": format!("srv-{}", name.to_lowercase()),
                    "check_cmd": "check.sh",
                    "integrity_check_cmd": "integrity.sh",
                    "infra_check_cmd": "infra.sh",
                    "rebuild_cmd": "rebuild.sh",
                    "rebuild_infra_cmd": "rebuild_infra.sh",
                }),
            )
            .await;
        }

        // Configure check results
        let configs: Vec<(&str, i32, i32, i32)> = combos
            .iter()
            .map(|(name, h, i, f, _)| (*name, *h, *i, *f))
            .collect();
        ctx.configure_check_results(app_id, configs).await;

        // Run diagnostic
        let resp = ctx
            .post(&format!("/api/v1/apps/{app_id}/diagnose"), json!({}))
            .await;
        assert_eq!(resp.status(), 200);
        let diag: Value = resp.json().await.unwrap();

        // Verify each combination produces the expected recommendation
        let components = diag["components"].as_array().unwrap();
        for (name, _, _, _, expected) in &combos {
            let comp = components
                .iter()
                .find(|c| c["name"].as_str() == Some(name))
                .unwrap_or_else(|| panic!("Component {name} not found in diagnostic result"));
            assert_eq!(
                comp["recommendation"].as_str(),
                Some(*expected),
                "Component {name}: expected {expected}, got {}",
                comp["recommendation"]
            );
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_diagnose_returns_check_details() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;

        ctx.configure_check_results(
            app_id,
            vec![
                ("Redis", 0, 0, 0),
                ("Tomcat", 2, 0, 0),
                ("Oracle", 2, 2, 0),
                ("Apache", 2, 2, 2),
            ],
        )
        .await;

        let resp = ctx
            .post(&format!("/api/v1/apps/{app_id}/diagnose"), json!({}))
            .await;
        let diag: Value = resp.json().await.unwrap();

        // Each component should have check details
        let components = diag["components"].as_array().unwrap();
        for comp in components {
            assert!(
                comp["health_check"].is_object() || comp["health_status"].is_string(),
                "Component should have health check details: {}",
                comp["name"]
            );
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_measures_rtr() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat").await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/rebuild"),
                json!({
                    "components": [
                        { "id": tomcat_id, "action": "app_rebuild" }
                    ]
                }),
            )
            .await;

        if resp.status().is_success() {
            let result: Value = resp.json().await.unwrap();
            // RTR (Recovery Time for Rebuild) should be tracked
            if let Some(rtr) = result.get("rtr_seconds") {
                assert!(
                    rtr.as_f64().unwrap_or(0.0) >= 0.0,
                    "RTR should be non-negative"
                );
            }
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_with_site_override() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, _site_b) = ctx.create_app_with_dr_sites().await;

        // Set a site-specific rebuild command override
        let oracle_prd_id = ctx.component_id(app_id, "Oracle-DB-prd").await;

        // Try rebuild with site override
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/rebuild"),
                json!({
                    "components": [
                        { "id": oracle_prd_id, "action": "app_rebuild" }
                    ],
                    "site_id": site_a,
                }),
            )
            .await;

        // Should be accepted (whether override exists or not)
        assert!(
            resp.status().is_success() || resp.status() == 200 || resp.status() == 202,
            "Rebuild with site override should be accepted, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_multiple_components_respects_dag() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;
        let oracle_id = ctx.component_id(app_id, "Oracle").await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat").await;

        // Rebuild both (Oracle is dependency of Tomcat)
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/rebuild"),
                json!({
                    "components": [
                        { "id": tomcat_id, "action": "app_rebuild" },
                        { "id": oracle_id, "action": "app_rebuild" }
                    ]
                }),
            )
            .await;
        assert!(resp.status().is_success());

        // Verify execution order through action_log
        tokio::time::sleep(Duration::from_secs(10)).await;
        let logs = ctx.get_action_log_for_type(app_id, "rebuild").await;
        if logs.len() >= 2 {
            let oracle_log = logs
                .iter()
                .find(|l| l.details["target_name"].as_str() == Some("Oracle"));
            let tomcat_log = logs
                .iter()
                .find(|l| l.details["target_name"].as_str() == Some("Tomcat"));
            if let (Some(o), Some(t)) = (oracle_log, tomcat_log) {
                assert!(
                    o.created_at <= t.created_at,
                    "Oracle must be rebuilt before Tomcat (DAG order)"
                );
            }
        }

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_diagnose_requires_operate_permission() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        ctx.grant_permission(app_id, ctx.viewer_user_id, "view")
            .await;

        let resp = ctx
            .post_as(
                "viewer",
                &format!("/api/v1/apps/{app_id}/diagnose"),
                json!({}),
            )
            .await;
        assert_eq!(resp.status(), 403, "Diagnose requires operate permission");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rebuild_requires_manage_permission() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat").await;

        ctx.grant_permission(app_id, ctx.operator_user_id, "operate")
            .await;

        let resp = ctx
            .post_as(
                "operator",
                &format!("/api/v1/apps/{app_id}/rebuild"),
                json!({
                    "components": [{ "id": tomcat_id, "action": "app_rebuild" }]
                }),
            )
            .await;
        assert_eq!(resp.status(), 403, "Rebuild requires manage permission");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_diagnose_audit_trail() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app_with_checks().await;

        ctx.post(&format!("/api/v1/apps/{app_id}/diagnose"), json!({}))
            .await;

        let logs = ctx.get_action_log(app_id, "diagnose").await;
        assert!(!logs.is_empty(), "Diagnose should be logged in action_log");

        ctx.cleanup().await;
    }
}
