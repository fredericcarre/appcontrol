/// E2E Test: DAG Validation — Cycle Detection, Dependency CRUD, Topological Ordering
///
/// Validates:
/// - Circular dependency creation is rejected (409)
/// - Self-dependency is rejected
/// - Dependency deletion works and recalculates DAG
/// - Orphan components (no dependencies) are in level 0
/// - Complex DAG topological sort correctness
use super::*;

#[cfg(test)]
mod test_dag_validation {
    use super::*;

    #[tokio::test]
    async fn test_cycle_detection_rejects_circular_dependency() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Try to create: Apache-Front → Oracle-DB (would create cycle)
        let apache_id = ctx.component_id(app_id, "Apache-Front").await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/dependencies"),
                json!({
                    "from_component_id": apache_id,
                    "to_component_id": oracle_id,
                }),
            )
            .await;
        assert_eq!(resp.status(), 409, "Circular dependency must be rejected");
        let body: Value = resp.json().await.unwrap();
        let error_text = format!(
            "{} {}",
            body["error"].as_str().unwrap_or(""),
            body["message"].as_str().unwrap_or("")
        );
        assert!(
            error_text.to_lowercase().contains("cycle"),
            "Error message should mention cycle, got: {error_text}"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_self_dependency_rejected() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/dependencies"),
                json!({
                    "from_component_id": oracle_id,
                    "to_component_id": oracle_id,
                }),
            )
            .await;
        assert!(
            resp.status() == 400 || resp.status() == 409,
            "Self-dependency must be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_duplicate_dependency_rejected() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat-App").await;

        // Oracle-DB → Tomcat-App already exists from create_payments_app
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/dependencies"),
                json!({
                    "from_component_id": oracle_id,
                    "to_component_id": tomcat_id,
                }),
            )
            .await;
        assert!(
            resp.status() == 409 || resp.status() == 400,
            "Duplicate dependency must be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_dependency_deletion_recalculates_dag() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // List dependencies
        let resp = ctx
            .get(&format!("/api/v1/apps/{app_id}/dependencies"))
            .await;
        let deps: Value = resp.json().await.unwrap();
        let deps_arr = deps
            .as_array()
            .or_else(|| deps["dependencies"].as_array())
            .expect("Response should contain dependencies array");
        let dep_count = deps_arr.len();
        assert_eq!(dep_count, 4, "Payments app should have 4 dependencies");

        // Delete one dependency (Tomcat-App → Apache-Front)
        let apache_id = ctx.component_id(app_id, "Apache-Front").await;
        let tomcat_to_apache = deps_arr
            .iter()
            .find(|d| {
                let to_id = d["to_component_id"].as_str().unwrap_or("");
                to_id == apache_id.to_string()
            })
            .expect("Should find dependency to Apache-Front");
        let dep_id = tomcat_to_apache["id"].as_str().unwrap();

        let resp = ctx
            .delete_as("admin", &format!("/api/v1/dependencies/{dep_id}"))
            .await;
        assert!(
            resp.status() == 200 || resp.status() == 204,
            "Delete should succeed, got {}",
            resp.status()
        );

        // Now Apache-Front should be independent (level 0 in its own plan)
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/start?dry_run=true"),
                json!({}),
            )
            .await;
        let plan: Value = resp.json().await.unwrap();
        let levels = plan["plan"]["levels"]
            .as_array()
            .or_else(|| plan["plan"].as_array())
            .expect("Plan should have levels");
        let level0 = &levels[0];
        let level0_names: Vec<&str> = level0
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert!(
            level0_names.contains(&"Apache-Front"),
            "Apache-Front should be in level 0 after dependency removal"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_dry_run_plan_has_correct_levels() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/start?dry_run=true"),
                json!({}),
            )
            .await;
        let plan: Value = resp.json().await.unwrap();
        let levels = plan["plan"]["levels"]
            .as_array()
            .or_else(|| plan["plan"].as_array())
            .expect("Plan should have levels");

        assert!(
            levels.len() >= 2,
            "Payments app should have at least 2 DAG levels, got {}",
            levels.len()
        );

        // Collect all component names from all levels
        let empty = Vec::new();
        let all_names: Vec<String> = levels
            .iter()
            .flat_map(|level| {
                level
                    .as_array()
                    .unwrap_or(&empty)
                    .iter()
                    .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
            })
            .collect();

        // Verify all 5 components are present across all levels
        assert!(
            all_names.contains(&"Oracle-DB".to_string()),
            "Should contain Oracle-DB"
        );
        assert!(
            all_names.contains(&"Tomcat-App".to_string()),
            "Should contain Tomcat-App"
        );
        assert!(
            all_names.contains(&"RabbitMQ".to_string()),
            "Should contain RabbitMQ"
        );
        assert!(
            all_names.contains(&"Apache-Front".to_string()),
            "Should contain Apache-Front"
        );
        assert!(
            all_names.contains(&"Batch-Processor".to_string()),
            "Should contain Batch-Processor"
        );
        assert_eq!(
            all_names.len(),
            5,
            "Should have exactly 5 components in the plan"
        );

        // Oracle-DB should be in a different level than Tomcat-App (they have a dependency)
        let oracle_level = levels
            .iter()
            .position(|l| {
                l.as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["name"].as_str() == Some("Oracle-DB"))
            })
            .unwrap();
        let tomcat_level = levels
            .iter()
            .position(|l| {
                l.as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["name"].as_str() == Some("Tomcat-App"))
            })
            .unwrap();
        assert_ne!(
            oracle_level, tomcat_level,
            "Oracle-DB and Tomcat-App should be on different levels"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_cross_app_dependency_rejected() {
        let ctx = TestContext::new().await;
        let app1_id = ctx.create_payments_app().await;

        // Create a second app
        let resp = ctx
            .post(
                "/api/v1/apps",
                json!({"name": "Other-App", "site_id": ctx.default_site_id}),
            )
            .await;
        let app2: Value = resp.json().await.unwrap();
        let app2_id: Uuid = app2["id"].as_str().unwrap().parse().unwrap();
        ctx.post(
            &format!("/api/v1/apps/{app2_id}/components"),
            json!({
                "name": "Other-Component",
                "component_type": "service",
                "hostname": "srv-other",
                "check_cmd": "check.sh",
            }),
        )
        .await;
        let other_id = ctx.component_id(app2_id, "Other-Component").await;
        let oracle_id = ctx.component_id(app1_id, "Oracle-DB").await;

        // Try cross-app dependency
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app1_id}/dependencies"),
                json!({
                    "from_component_id": oracle_id,
                    "to_component_id": other_id,
                }),
            )
            .await;
        assert!(
            resp.status() == 400 || resp.status() == 404,
            "Cross-app dependency must be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_indirect_cycle_detection() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Existing: Oracle-DB → Tomcat-App → Apache-Front
        // Try: Apache-Front → Tomcat-App (would create indirect cycle)
        let apache_id = ctx.component_id(app_id, "Apache-Front").await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat-App").await;

        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/dependencies"),
                json!({
                    "from_component_id": apache_id,
                    "to_component_id": tomcat_id,
                }),
            )
            .await;
        assert_eq!(resp.status(), 409, "Indirect cycle must be rejected");

        ctx.cleanup().await;
    }
}
