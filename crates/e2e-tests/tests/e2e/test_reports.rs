/// E2E Test: Reports — Availability, Incidents, Switchovers, Audit, Compliance, RTO
///
/// Validates all report endpoints produce correct data by:
/// 1. Seeding specific data into the event tables
/// 2. Calling report endpoints
/// 3. Asserting the computed values match expected results
///
/// This validates actual database CONTENT and computation, not just HTTP 200 status codes.
use super::*;

#[cfg(test)]
mod test_reports {
    use super::*;

    // ── Availability Report: seed state_transitions, refresh materialized view,
    //    validate running_seconds and availability_pct ──

    #[tokio::test]
    async fn test_availability_report_computes_correct_percentage() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat-App").await;

        let today = chrono::Utc::now().date_naive();

        // Seed state_transitions: Oracle-DB had 10 transitions to RUNNING today
        // Tomcat-App had 5 transitions to RUNNING today
        for i in 0..10 {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
                 VALUES ($1, 'STOPPED', 'RUNNING', 'check', '{}',
                         $2::date + interval '1 hour' * $3)"
            )
            .bind(oracle_id)
            .bind(today)
            .bind(i as i32)
            .execute(&ctx.db_pool).await.unwrap();
        }
        for i in 0..5 {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
                 VALUES ($1, 'STOPPED', 'RUNNING', 'check', '{}',
                         $2::date + interval '1 hour' * $3)"
            )
            .bind(tomcat_id)
            .bind(today)
            .bind(i as i32)
            .execute(&ctx.db_pool).await.unwrap();
        }

        // Refresh the materialized view so the report picks up our data
        sqlx::query("REFRESH MATERIALIZED VIEW component_daily_stats")
            .execute(&ctx.db_pool)
            .await
            .unwrap();

        // Call availability report
        let resp = ctx.get(&format!(
            "/api/v1/apps/{app_id}/reports/availability?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        )).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().unwrap();

        // Should have data for both components
        assert!(
            !data.is_empty(),
            "Availability report should contain data after seeding"
        );

        // Verify Oracle-DB entry: 10 transitions * 30s = 300 running_seconds
        let oracle_entries: Vec<_> = data
            .iter()
            .filter(|d| d["component_id"].as_str() == Some(&oracle_id.to_string()))
            .collect();
        assert!(
            !oracle_entries.is_empty(),
            "Should have Oracle-DB availability data"
        );
        let oracle_running = oracle_entries[0]["running_seconds"].as_i64().unwrap();
        assert_eq!(
            oracle_running, 300,
            "Oracle: 10 transitions * 30s = 300 running_seconds"
        );

        // Verify availability_pct is computed: 300/86400 * 100 ≈ 0.347%
        let oracle_pct = oracle_entries[0]["availability_pct"].as_f64().unwrap();
        assert!(
            oracle_pct > 0.0 && oracle_pct < 1.0,
            "Oracle availability should be ~0.347%, got {oracle_pct}"
        );

        // Verify Tomcat-App entry: 5 transitions * 30s = 150 running_seconds
        let tomcat_entries: Vec<_> = data
            .iter()
            .filter(|d| d["component_id"].as_str() == Some(&tomcat_id.to_string()))
            .collect();
        assert!(
            !tomcat_entries.is_empty(),
            "Should have Tomcat-App availability data"
        );
        let tomcat_running = tomcat_entries[0]["running_seconds"].as_i64().unwrap();
        assert_eq!(
            tomcat_running, 150,
            "Tomcat: 5 transitions * 30s = 150 running_seconds"
        );

        // total_seconds should be 86400 (one day)
        assert_eq!(oracle_entries[0]["total_seconds"].as_i64().unwrap(), 86400);

        ctx.cleanup().await;
    }

    // ── Incidents Report: seed FAILED transitions, validate component names and count ──

    #[tokio::test]
    async fn test_incidents_report_returns_seeded_failures() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;
        let oracle_id = ctx.component_id(app_id, "Oracle-DB").await;
        let tomcat_id = ctx.component_id(app_id, "Tomcat-App").await;

        // Seed 3 FAILED transitions for Oracle-DB and 1 for Tomcat-App
        for _ in 0..3 {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
                 VALUES ($1, 'RUNNING', 'FAILED', 'check', '{\"reason\": \"ORA-12541\"}', chrono::Utc::now().to_rfc3339())"
            )
            .bind(oracle_id)
            .execute(&ctx.db_pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details, created_at)
             VALUES ($1, 'RUNNING', 'FAILED', 'check', '{\"reason\": \"java.lang.OutOfMemoryError\"}', chrono::Utc::now().to_rfc3339())"
        )
        .bind(tomcat_id)
        .execute(&ctx.db_pool).await.unwrap();

        // Call incidents report
        let resp = ctx.get(&format!(
            "/api/v1/apps/{app_id}/reports/incidents?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        )).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().unwrap();

        // Should have exactly 4 incidents
        assert_eq!(
            data.len(),
            4,
            "Should have 4 incidents (3 Oracle + 1 Tomcat), got {}",
            data.len()
        );

        // All incidents should be FAILED state
        for incident in data {
            assert_eq!(incident["state"].as_str(), Some("FAILED"));
        }

        // Verify component names are present
        let oracle_incidents: Vec<_> = data
            .iter()
            .filter(|d| d["component_name"].as_str() == Some("Oracle-DB"))
            .collect();
        assert_eq!(
            oracle_incidents.len(),
            3,
            "Should have 3 Oracle-DB incidents"
        );

        let tomcat_incidents: Vec<_> = data
            .iter()
            .filter(|d| d["component_name"].as_str() == Some("Tomcat-App"))
            .collect();
        assert_eq!(
            tomcat_incidents.len(),
            1,
            "Should have 1 Tomcat-App incident"
        );

        // Verify each incident has a timestamp
        for incident in data {
            assert!(
                incident["at"].is_string(),
                "Incident should have a timestamp"
            );
        }

        ctx.cleanup().await;
    }

    // ── Switchovers Report: seed switchover_log phases, validate content ──

    #[tokio::test]
    async fn test_switchovers_report_returns_seeded_phases() {
        let ctx = TestContext::new().await;
        let (app_id, _site_a, _site_b) = ctx.create_app_with_dr_sites().await;
        let switchover_id = Uuid::new_v4();

        // Seed 4 switchover_log entries for a complete switchover
        let phases = [
            ("PREPARE", "completed"),
            ("VALIDATE", "completed"),
            ("STOP_SOURCE", "completed"),
            ("COMMIT", "completed"),
        ];
        for (i, (phase, status)) in phases.iter().enumerate() {
            sqlx::query(
                "INSERT INTO switchover_log (switchover_id, application_id, phase, status, details, created_at)
                 VALUES ($1, $2, $3, $4, $5, NOW() + interval '1 minute' * $6)"
            )
            .bind(bind_id(switchover_id))
            .bind(bind_id(app_id))
            .bind(phase)
            .bind(status)
            .bind(serde_json::json!({"source_site": "PRD", "target_site": "DR"}))
            .bind(i as i32)
            .execute(&ctx.db_pool).await.unwrap();
        }

        // Call switchovers report
        let resp = ctx
            .get(&format!("/api/v1/apps/{app_id}/reports/switchovers"))
            .await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().unwrap();

        // Should have 4 entries
        assert_eq!(
            data.len(),
            4,
            "Should have 4 switchover log entries, got {}",
            data.len()
        );

        // Verify phases are present
        let logged_phases: Vec<&str> = data.iter().map(|d| d["phase"].as_str().unwrap()).collect();
        assert!(logged_phases.contains(&"PREPARE"));
        assert!(logged_phases.contains(&"COMMIT"));

        // All entries should have completed status
        for entry in data {
            assert_eq!(entry["status"].as_str(), Some("completed"));
        }

        ctx.cleanup().await;
    }

    // ── Audit Report: seed action_log entries, validate user_id, action, date filtering ──

    #[tokio::test]
    async fn test_audit_report_validates_seeded_content() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Seed specific action_log entries with known values
        let actions = [
            ("start", "application"),
            ("stop", "application"),
            ("config_change", "application"),
            ("diagnose", "application"),
        ];
        for (action, resource_type) in &actions {
            sqlx::query(
                "INSERT INTO action_log (user_id, action, resource_type, resource_id, details, created_at)
                 VALUES ($1, $2, $3, $4, $5, chrono::Utc::now().to_rfc3339())"
            )
            .bind(ctx.admin_user_id)
            .bind(action)
            .bind(resource_type)
            .bind(bind_id(app_id))
            .bind(serde_json::json!({"source": "e2e_test"}))
            .execute(&ctx.db_pool).await.unwrap();
        }

        // Call audit report with wide date range
        let resp = ctx
            .get(&format!(
            "/api/v1/apps/{app_id}/reports/audit?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        ))
            .await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().unwrap();

        // Should have at least 4 entries (our seeded ones + any from create_payments_app)
        assert!(
            data.len() >= 4,
            "Audit report should have at least 4 entries, got {}",
            data.len()
        );

        // Verify the seeded actions are present
        let reported_actions: Vec<&str> =
            data.iter().map(|d| d["action"].as_str().unwrap()).collect();
        assert!(
            reported_actions.contains(&"start"),
            "Audit should contain 'start' action"
        );
        assert!(
            reported_actions.contains(&"stop"),
            "Audit should contain 'stop' action"
        );
        assert!(
            reported_actions.contains(&"config_change"),
            "Audit should contain 'config_change'"
        );
        assert!(
            reported_actions.contains(&"diagnose"),
            "Audit should contain 'diagnose'"
        );

        // Verify each entry has user_id and resource_type
        for entry in data {
            assert!(
                entry["user_id"].is_string(),
                "Audit entry should have user_id"
            );
            assert!(
                entry["resource_type"].is_string(),
                "Audit entry should have resource_type"
            );
        }

        // Verify user_id matches the admin who created the entries
        let admin_entries: Vec<_> = data
            .iter()
            .filter(|d| d["user_id"].as_str() == Some(&ctx.admin_user_id.to_string()))
            .collect();
        assert!(
            admin_entries.len() >= 4,
            "All seeded entries should be from admin user"
        );

        ctx.cleanup().await;
    }

    // ── Audit Report: date filtering works correctly ──

    #[tokio::test]
    async fn test_audit_report_date_filtering() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Seed an entry in the past
        sqlx::query(
            "INSERT INTO action_log (user_id, action, resource_type, resource_id, details, created_at)
             VALUES ($1, 'old_action', 'application', $2, '{}', '2023-01-15T12:00:00Z')"
        )
        .bind(ctx.admin_user_id)
        .bind(bind_id(app_id))
        .execute(&ctx.db_pool).await.unwrap();

        // Seed an entry now
        sqlx::query(
            "INSERT INTO action_log (user_id, action, resource_type, resource_id, details, created_at)
             VALUES ($1, 'recent_action', 'application', $2, '{}', chrono::Utc::now().to_rfc3339())"
        )
        .bind(ctx.admin_user_id)
        .bind(bind_id(app_id))
        .execute(&ctx.db_pool).await.unwrap();

        // Query only recent entries (2025+)
        let resp = ctx
            .get(&format!(
            "/api/v1/apps/{app_id}/reports/audit?from=2025-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        ))
            .await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().unwrap();

        // Should contain recent_action but NOT old_action
        let actions: Vec<&str> = data.iter().map(|d| d["action"].as_str().unwrap()).collect();
        assert!(
            actions.contains(&"recent_action"),
            "Should include recent_action"
        );
        assert!(
            !actions.contains(&"old_action"),
            "Should exclude old_action (outside date range)"
        );

        ctx.cleanup().await;
    }

    // ── Compliance Report: seed action_log entries, validate DORA count ──

    #[tokio::test]
    async fn test_compliance_report_counts_audit_entries() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // Seed exactly 7 action_log entries with resource_id = app_id
        for i in 0..7 {
            sqlx::query(
                "INSERT INTO action_log (user_id, action, resource_type, resource_id, details, created_at)
                 VALUES ($1, $2, 'application', $3, '{}', chrono::Utc::now().to_rfc3339())"
            )
            .bind(ctx.admin_user_id)
            .bind(format!("action_{i}"))
            .bind(bind_id(app_id))
            .execute(&ctx.db_pool).await.unwrap();
        }

        // Call compliance report
        let resp = ctx
            .get(&format!("/api/v1/apps/{app_id}/reports/compliance"))
            .await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();

        // Verify DORA compliance fields
        assert_eq!(report["report"].as_str(), Some("compliance"));
        assert_eq!(report["dora_compliant"].as_bool(), Some(true));
        assert_eq!(report["append_only_enforced"].as_bool(), Some(true));

        // audit_trail_entries should be >= 7 (our seeded entries + any from create_payments_app)
        let entry_count = report["audit_trail_entries"].as_i64().unwrap();
        assert!(
            entry_count >= 7,
            "Compliance report should count at least 7 audit entries, got {entry_count}"
        );

        ctx.cleanup().await;
    }

    // ── RTO Report: seed switchover_log PREPARE+COMMIT with known timestamps, validate computation ──

    #[tokio::test]
    async fn test_rto_report_computes_average_from_switchover_data() {
        let ctx = TestContext::new().await;
        let (app_id, _site_a, _site_b) = ctx.create_app_with_dr_sites().await;

        // Seed switchover #1: PREPARE at T, COMMIT at T+120s → RTO = 120s
        let sw1 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO switchover_log (switchover_id, application_id, phase, status, details, created_at)
             VALUES ($1, $2, 'PREPARE', 'completed', '{}', '2026-02-01T10:00:00Z')"
        ).bind(sw1).bind(bind_id(app_id)).execute(&ctx.db_pool).await.unwrap();
        sqlx::query(
            "INSERT INTO switchover_log (switchover_id, application_id, phase, status, details, created_at)
             VALUES ($1, $2, 'COMMIT', 'completed', '{}', '2026-02-01T10:02:00Z')"
        ).bind(sw1).bind(bind_id(app_id)).execute(&ctx.db_pool).await.unwrap();

        // Seed switchover #2: PREPARE at T, COMMIT at T+180s → RTO = 180s
        let sw2 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO switchover_log (switchover_id, application_id, phase, status, details, created_at)
             VALUES ($1, $2, 'PREPARE', 'completed', '{}', '2026-02-15T10:00:00Z')"
        ).bind(sw2).bind(bind_id(app_id)).execute(&ctx.db_pool).await.unwrap();
        sqlx::query(
            "INSERT INTO switchover_log (switchover_id, application_id, phase, status, details, created_at)
             VALUES ($1, $2, 'COMMIT', 'completed', '{}', '2026-02-15T10:03:00Z')"
        ).bind(sw2).bind(bind_id(app_id)).execute(&ctx.db_pool).await.unwrap();

        // Call RTO report
        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/reports/rto")).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();

        assert_eq!(report["report"].as_str(), Some("rto"));

        // Average RTO = (120 + 180) / 2 = 150 seconds
        let avg_rto = report["average_rto_seconds"].as_f64().unwrap();
        assert!(
            (avg_rto - 150.0).abs() < 1.0,
            "Average RTO should be ~150s (avg of 120s and 180s), got {avg_rto}"
        );

        ctx.cleanup().await;
    }

    // ── RTO Report: no switchover data returns null ──

    #[tokio::test]
    async fn test_rto_report_returns_null_with_no_data() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let resp = ctx.get(&format!("/api/v1/apps/{app_id}/reports/rto")).await;
        assert_eq!(resp.status(), 200);
        let report: Value = resp.json().await.unwrap();
        assert!(
            report["average_rto_seconds"].is_null(),
            "RTO should be null when no switchover data exists"
        );

        ctx.cleanup().await;
    }

    // ── Permission enforcement ──

    #[tokio::test]
    async fn test_report_requires_view_permission() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        // No permission → 403
        let resp = ctx
            .get_as(
                "viewer",
                &format!("/api/v1/apps/{app_id}/reports/availability"),
            )
            .await;
        assert_eq!(resp.status(), 403);

        // Grant view → 200
        ctx.grant_permission(app_id, ctx.viewer_user_id, "view")
            .await;
        let resp = ctx
            .get_as(
                "viewer",
                &format!("/api/v1/apps/{app_id}/reports/availability"),
            )
            .await;
        assert_eq!(resp.status(), 200);

        ctx.cleanup().await;
    }

    // ── All report endpoints respond correctly ──

    #[tokio::test]
    async fn test_all_six_report_endpoints_respond() {
        let ctx = TestContext::new().await;
        let app_id = ctx.create_payments_app().await;

        let endpoints = [
            "availability",
            "incidents",
            "switchovers",
            "audit",
            "compliance",
            "rto",
        ];

        for endpoint in &endpoints {
            let resp = ctx
                .get(&format!("/api/v1/apps/{app_id}/reports/{endpoint}"))
                .await;
            assert!(
                resp.status().is_success(),
                "Report endpoint '{}' should return success, got {}",
                endpoint,
                resp.status()
            );
        }

        ctx.cleanup().await;
    }
}
