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
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("cycle"),
            "Error message should mention cycle"
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
        let dep_count = deps.as_array().unwrap().len();
        assert_eq!(dep_count, 4, "Payments app should have 4 dependencies");

        // Delete one dependency (Tomcat-App → Apache-Front)
        let tomcat_to_apache = deps
            .as_array()
            .unwrap()
            .iter()
            .find(|d| {
                let to_name = d["to_component_name"].as_str().unwrap_or("");
                to_name == "Apache-Front"
            })
            .unwrap();
        let dep_id = tomcat_to_apache["id"].as_str().unwrap();

        let resp = ctx
            .delete_as("admin", &format!("/api/v1/dependencies/{dep_id}"))
            .await;
        assert_eq!(resp.status(), 200);

        // Now Apache-Front should be independent (level 0 in its own plan)
        let resp = ctx
            .post(
                &format!("/api/v1/apps/{app_id}/start?dry_run=true"),
                json!({}),
            )
            .await;
        let plan: Value = resp.json().await.unwrap();
        let level0 = &plan["plan"][0];
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
        let levels = plan["plan"].as_array().unwrap();

        assert_eq!(levels.len(), 3, "Payments app should have 3 DAG levels");

        // Level 0: Oracle-DB only
        let l0: Vec<&str> = levels[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert!(l0.contains(&"Oracle-DB"));
        assert_eq!(l0.len(), 1);

        // Level 1: Tomcat-App + RabbitMQ (parallel)
        let l1: Vec<&str> = levels[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert!(l1.contains(&"Tomcat-App"));
        assert!(l1.contains(&"RabbitMQ"));
        assert_eq!(l1.len(), 2);

        // Level 2: Apache-Front + Batch-Processor (parallel)
        let l2: Vec<&str> = levels[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert!(l2.contains(&"Apache-Front"));
        assert!(l2.contains(&"Batch-Processor"));
        assert_eq!(l2.len(), 2);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_cross_app_dependency_rejected() {
        let ctx = TestContext::new().await;
        let app1_id = ctx.create_payments_app().await;

        // Create a second app
        let resp = ctx.post("/api/v1/apps", json!({"name": "Other-App"})).await;
        let app2: Value = resp.json().await.unwrap();
        let app2_id: Uuid = app2["id"].as_str().unwrap().parse().unwrap();
        ctx.post(
            &format!("/api/v1/apps/{app2_id}/components"),
            json!({
                "name": "Other-Component",
                "component_type": "generic",
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
