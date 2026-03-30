//! SQLite E2E: Reports endpoint tests.
//! Tests all 6 report endpoints respond correctly via HTTP API.

use super::common::TestContext;
use serde_json::{json, Value};

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
        let url = if *endpoint == "availability" || *endpoint == "incidents" || *endpoint == "audit"
        {
            format!(
                "/api/v1/apps/{app_id}/reports/{endpoint}?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
            )
        } else {
            format!("/api/v1/apps/{app_id}/reports/{endpoint}")
        };
        let resp = ctx.get(&url).await;
        assert!(
            resp.status().is_success(),
            "Report '{}' should succeed, got {}",
            endpoint,
            resp.status()
        );
    }
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_incidents_report_returns_empty_initially() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!(
            "/api/v1/apps/{app_id}/reports/incidents?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        ))
        .await;
    assert_eq!(resp.status(), 200);
    let report: Value = resp.json().await.unwrap();
    let data = report["data"].as_array().unwrap();
    assert!(data.is_empty(), "No incidents expected initially");
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_compliance_report_structure() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!("/api/v1/apps/{app_id}/reports/compliance"))
        .await;
    assert_eq!(resp.status(), 200);
    let report: Value = resp.json().await.unwrap();
    assert_eq!(report["report"].as_str(), Some("compliance"));
    assert!(report["dora_compliant"].is_boolean());
    assert!(report["append_only_enforced"].is_boolean());
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_rto_report_returns_null_with_no_data() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!("/api/v1/apps/{app_id}/reports/rto"))
        .await;
    assert_eq!(resp.status(), 200);
    let report: Value = resp.json().await.unwrap();
    assert!(
        report["average_rto_seconds"].is_null(),
        "RTO should be null with no switchover data"
    );
    ctx.cleanup().await;
}

#[tokio::test]
async fn test_report_requires_view_permission() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get_as(
            "viewer",
            &format!("/api/v1/apps/{app_id}/reports/availability"),
        )
        .await;
    assert_eq!(resp.status(), 403);

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

#[tokio::test]
async fn test_audit_report_returns_data() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    let resp = ctx
        .get(&format!(
            "/api/v1/apps/{app_id}/reports/audit?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
        ))
        .await;
    assert_eq!(resp.status(), 200);
    let report: Value = resp.json().await.unwrap();
    let data = report["data"].as_array().unwrap();
    assert!(
        !data.is_empty(),
        "Audit report should have entries from app creation"
    );
    ctx.cleanup().await;
}
