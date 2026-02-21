/// E2E Test: Application CRUD, Tags, Search, Pagination
use super::*;

#[cfg(test)]
mod test_app_crud {
    use super::*;

    #[tokio::test]
    async fn test_create_app() {
        let ctx = TestContext::new().await;

        let resp = ctx
            .post(
                "/api/v1/apps",
                json!({
                    "name": "New-App",
                    "description": "A new application",
                    "tags": ["production", "tier-1"],
                    "site_id": ctx.default_site_id,
                }),
            )
            .await;
        assert!(resp.status() == 201 || resp.status() == 200);
        let app: Value = resp.json().await.unwrap();
        assert_eq!(app["name"], "New-App");
        assert!(app["id"].as_str().is_some());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_create_app_grants_owner_permission() {
        let ctx = TestContext::new().await;

        let resp = ctx
            .post("/api/v1/apps", json!({"name": "Owner-Test", "site_id": ctx.default_site_id}))
            .await;
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        // Creator should have owner permission
        let resp = ctx
            .get(&format!("/api/v1/apps/{app_id}/permissions/effective"))
            .await;
        let eff: Value = resp.json().await.unwrap();
        assert_eq!(
            eff["permission_level"], "owner",
            "App creator should automatically get owner permission"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_list_apps() {
        let ctx = TestContext::new().await;
        ctx.create_payments_app().await;

        let resp = ctx.get("/api/v1/apps").await;
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let apps = body["apps"].as_array().unwrap();
        assert!(!apps.is_empty());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_list_apps_with_search() {
        let ctx = TestContext::new().await;
        ctx.create_payments_app().await;
        ctx.post("/api/v1/apps", json!({"name": "Other-App", "site_id": ctx.default_site_id})).await;

        // Search by name
        let resp = ctx.get("/api/v1/apps?search=Paiements").await;
        let body: Value = resp.json().await.unwrap();
        let names: Vec<&str> = body["apps"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["name"].as_str())
            .collect();
        assert!(names.iter().any(|n| n.contains("Paiements")));
        assert!(
            !names.iter().any(|n| n.contains("Other")),
            "Search should filter results"
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_list_apps_with_pagination() {
        let ctx = TestContext::new().await;

        // Create 5 apps
        for i in 0..5 {
            ctx.post("/api/v1/apps", json!({"name": format!("App-{i}"), "site_id": ctx.default_site_id}))
                .await;
        }

        // Page 1: limit=2, offset=0
        let resp = ctx.get("/api/v1/apps?limit=2&offset=0").await;
        let page1: Value = resp.json().await.unwrap();
        assert_eq!(page1["apps"].as_array().unwrap().len(), 2);

        // Page 2: limit=2, offset=2
        let resp = ctx.get("/api/v1/apps?limit=2&offset=2").await;
        let page2: Value = resp.json().await.unwrap();
        assert_eq!(page2["apps"].as_array().unwrap().len(), 2);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_update_app() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx
            .put_as(
                "admin",
                &format!("/api/v1/apps/{app_id}"),
                json!({
                    "description": "Updated description",
                    "tags": ["payments", "critical", "updated"],
                }),
            )
            .await;
        assert_eq!(resp.status(), 200);

        // Verify update
        let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
        let app: Value = resp.json().await.unwrap();
        assert_eq!(app["description"], "Updated description");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_delete_app_requires_owner() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Editor cannot delete
        ctx.grant_permission(app_id, ctx.editor_user_id, "edit")
            .await;
        let resp = ctx
            .delete_as("editor", &format!("/api/v1/apps/{app_id}"))
            .await;
        assert_eq!(resp.status(), 403);

        // Admin (org admin = implicit owner) can delete
        let resp = ctx
            .delete_as("admin", &format!("/api/v1/apps/{app_id}"))
            .await;
        assert_eq!(resp.status(), 200);

        // Verify deleted
        let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
        assert_eq!(resp.status(), 404);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_get_nonexistent_app_returns_404() {
        let ctx = TestContext::new().await;
        let fake_id = Uuid::new_v4();

        let resp = ctx.get(&format!("/api/v1/apps/{fake_id}")).await;
        assert_eq!(resp.status(), 404);

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_create_app_with_duplicate_name_rejected() {
        let ctx = TestContext::new().await;
        ctx.create_payments_app().await;

        // Try to create another app with the same name
        let resp = ctx
            .post(
                "/api/v1/apps",
                json!({
                    "name": "Paiements-SEPA",
                    "site_id": ctx.default_site_id,
                }),
            )
            .await;
        assert!(
            resp.status() == 409 || resp.status() == 400,
            "Duplicate name should be rejected, got {}",
            resp.status()
        );

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_app_delete_preserves_audit_logs() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Generate some audit data
        ctx.post(&format!("/api/v1/apps/{app_id}/start"), json!({}))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;

        let logs_before = ctx.count_action_logs().await;
        assert!(logs_before > 0);

        // Delete the app
        ctx.delete_as("admin", &format!("/api/v1/apps/{app_id}"))
            .await;

        // Audit logs should NOT be deleted (append-only)
        let logs_after = ctx.count_action_logs().await;
        assert!(
            logs_after >= logs_before,
            "Deleting an app must not delete its audit logs"
        );

        ctx.cleanup().await;
    }
}
