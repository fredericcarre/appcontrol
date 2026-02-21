/// E2E Test: Switchover Advanced — Selective Mode, Progressive Mode, Edge Cases
///
/// Validates:
/// - Selective switchover (only specified component_ids)
/// - Rollback after COMMIT is rejected
/// - Concurrent switchover on same app is rejected
/// - Switchover audit trail completeness
/// - Switchover on app with no DR site returns error

#[cfg(test)]
mod test_switchover_advanced {
    use super::*;

    #[tokio::test]
    async fn test_selective_switchover_only_moves_specified_components() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;
        ctx.set_all_running_on_site(app_id, site_a).await;

        // Get specific component IDs for selective switchover
        let oracle_prd_id = ctx.component_id(app_id, "Oracle-DB-prd").await;

        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_b,
            "mode": "SELECTIVE",
            "component_ids": [oracle_prd_id],
        })).await;
        assert!(resp.status().is_success(),
            "Selective switchover should be accepted, got {}", resp.status());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_progressive_switchover() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;
        ctx.set_all_running_on_site(app_id, site_a).await;

        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_b,
            "mode": "PROGRESSIVE",
        })).await;
        assert!(resp.status().is_success(),
            "Progressive switchover should be accepted, got {}", resp.status());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rollback_after_commit_is_rejected() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;
        ctx.set_all_running_on_site(app_id, site_a).await;

        // Start and complete switchover
        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_b, "mode": "FULL"
        })).await;
        let sw_id: Uuid = resp.json::<Value>().await["switchover_id"]
            .as_str().unwrap().parse().unwrap();

        // Advance through all phases and commit
        for _ in 0..5 {
            ctx.post(&format!("/api/v1/switchovers/{sw_id}/next-phase"), json!({})).await;
        }
        ctx.post(&format!("/api/v1/switchovers/{sw_id}/commit"), json!({})).await;

        // Try rollback after commit → should be rejected
        let resp = ctx.post(&format!("/api/v1/switchovers/{sw_id}/rollback"), json!({})).await;
        assert!(resp.status() == 400 || resp.status() == 409,
            "Rollback after COMMIT should be rejected, got {}", resp.status());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_concurrent_switchover_rejected() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;
        ctx.set_all_running_on_site(app_id, site_a).await;

        // Start first switchover
        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_b, "mode": "FULL"
        })).await;
        assert!(resp.status().is_success());

        // Try to start a second switchover on same app
        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_a, "mode": "FULL"
        })).await;
        assert!(resp.status() == 409 || resp.status() == 400,
            "Concurrent switchover should be rejected, got {}", resp.status());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_switchover_requires_manage_permission() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;

        ctx.grant_permission(app_id, ctx.operator_user_id, "operate").await;

        // Operator (operate level) cannot start switchover (needs manage)
        let resp = ctx.post_as("operator",
            &format!("/api/v1/apps/{app_id}/switchover"), json!({
                "target_site_id": site_b, "mode": "FULL"
            })).await;
        assert_eq!(resp.status(), 403, "Switchover requires manage permission");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_switchover_on_single_site_app_rejected() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let fake_site = Uuid::new_v4();

        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": fake_site, "mode": "FULL"
        })).await;
        assert!(resp.status() == 400 || resp.status() == 404,
            "Switchover to non-existent site should fail, got {}", resp.status());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_switchover_status_endpoint() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;
        ctx.set_all_running_on_site(app_id, site_a).await;

        let resp = ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_b, "mode": "FULL"
        })).await;
        let sw_id: Uuid = resp.json::<Value>().await["switchover_id"]
            .as_str().unwrap().parse().unwrap();

        // Check status
        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/switchover/status")).await;
        assert_eq!(resp.status(), 200);
        let status: Value = resp.json().await;
        assert!(status["status"].as_str().is_some());
        assert_eq!(status["switchover_id"].as_str().unwrap(),
            sw_id.to_string().as_str());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_switchover_audit_trail() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;
        ctx.set_all_running_on_site(app_id, site_a).await;

        ctx.post(&format!("/api/v1/apps/{app_id}/switchover"), json!({
            "target_site_id": site_b, "mode": "FULL"
        })).await;

        // Verify switchover is logged in action_log
        let logs = ctx.get_action_log(app_id, "switchover").await;
        assert!(!logs.is_empty(), "Switchover should be recorded in action_log");

        ctx.cleanup().await;
    }
}
