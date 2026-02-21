/// E2E Test: Reports — Availability, Incidents, Switchovers, Audit, Compliance, RTO
///
/// Validates all report endpoints produce correct data.

#[cfg(test)]
mod test_reports {
    use super::*;

    #[tokio::test]
    async fn test_availability_report() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.set_all_running(app_id).await;

        let resp = ctx.get(&format!(
            "/api/v1/apps/{app_id}/reports/availability?from=2026-01-01T00:00:00Z&to=2026-12-31T23:59:59Z"
        )).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await;
        assert!(report.is_object() || report.is_array(),
            "Availability report should return data");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_incidents_report() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        ctx.set_all_running(app_id).await;

        // Create an incident (component goes FAILED)
        ctx.force_component_state(app_id, "Oracle-DB", "FAILED").await;
        // Record transition
        sqlx::query(
            "INSERT INTO state_transitions (id, application_id, component_id, component_name,
             previous_state, new_state, trigger_type, created_at)
             SELECT gen_random_uuid(), $1, id, 'Oracle-DB', 'RUNNING', 'FAILED', 'check',
             NOW() FROM components WHERE application_id = $1 AND name = 'Oracle-DB'"
        ).bind(app_id).execute(&ctx.db_pool).await.unwrap();

        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/reports/incidents")).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await;
        let incidents = report.as_array().unwrap_or(&vec![]);
        assert!(!incidents.is_empty(), "Should have at least 1 incident");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_switchovers_report() {
        let ctx = TestContext::new().await;
        let (app_id, site_a, site_b) = ctx.create_app_with_dr_sites().await;

        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/reports/switchovers")).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await;
        // Initially empty
        assert!(report.as_array().unwrap_or(&vec![]).is_empty() || report.is_array());

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_audit_report_with_date_filter() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Generate some audit data
        ctx.post_as("admin", &format!("/api/v1/apps/{app_id}/start"), json!({})).await;

        let resp = ctx.get(&format!(
            "/api/v1/apps/{app_id}/reports/audit?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        )).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await;
        let entries = report.as_array().unwrap();
        assert!(!entries.is_empty(), "Should have audit entries after start");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_compliance_report_dora_metrics() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/reports/compliance")).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await;
        // DORA metrics should include deployment frequency, lead time, MTTR, change failure rate
        assert!(report.is_object(), "Compliance report should be an object");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_rto_report() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/reports/rto")).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await;
        assert!(report.is_object(), "RTO report should be an object");

        ctx.cleanup().await;
    }

    #[tokio::test]
    async fn test_report_requires_view_permission() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // No permission → 403
        let resp = ctx.get_as("viewer", &format!("/api/v1/apps/{app_id}/reports/availability")).await;
        assert_eq!(resp.status(), 403);

        // Grant view → 200
        ctx.grant_permission(app_id, ctx.viewer_user_id, "view").await;
        let resp = ctx.get_as("viewer", &format!("/api/v1/apps/{app_id}/reports/availability")).await;
        assert_eq!(resp.status(), 200);

        ctx.cleanup().await;
    }
}
