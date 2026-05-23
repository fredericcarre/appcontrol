/// Real E2E Test: Metrics Extraction from Check stdout
///
/// Validates the full path from check script execution to metrics persistence:
/// 1. Agent runs a check script that emits Nagios-style perfdata.
/// 2. Agent parses the perfdata and includes it in the CheckResult.
/// 3. Gateway forwards the result to the backend.
/// 4. Backend inserts the metrics into check_events.metrics JSONB.
///
/// Covers the migration use case: existing Nagios/Icinga plugins drop in
/// unchanged and their perfdata becomes queryable AppControl metrics.
mod harness;

use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn test_check_emits_nagios_perfdata_metrics() {
    let h = harness::TestHarness::start().await;
    let site_id = h.default_site_id().await;

    let scripts = h.scripts_dir.to_str().unwrap();
    let pid_dir = h.pid_dir.to_str().unwrap();
    let proc_name = "perfdata-svc";

    // Create app with a single component using the perfdata check.
    let app = h
        .api_post(
            "/apps",
            serde_json::json!({
                "name": "Metrics-Extraction-E2E",
                "description": "Validates Nagios perfdata extraction end-to-end",
                "site_id": site_id,
            }),
        )
        .await;
    let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

    let comp = h
        .api_post(
            &format!("/apps/{app_id}/components"),
            serde_json::json!({
                "name": "Perfdata-Service",
                "component_type": "service",
                "hostname": "localhost",
                "check_cmd": format!("{scripts}/check_with_perfdata.sh {proc_name} {pid_dir}"),
                "start_cmd": format!("{scripts}/start_process.sh {proc_name} {pid_dir}"),
                "stop_cmd": format!("{scripts}/stop_process.sh {proc_name} {pid_dir}"),
            }),
        )
        .await;
    let comp_id: Uuid = comp["id"].as_str().unwrap().parse().unwrap();

    // Start the component and wait for RUNNING.
    h.api_post(&format!("/apps/{app_id}/start"), serde_json::json!({}))
        .await;
    assert!(
        h.wait_for_state(comp_id, "RUNNING", Duration::from_secs(90))
            .await,
        "component should reach RUNNING after start"
    );

    // Allow at least one healthy check cycle to land in check_events.
    // STALENESS_TIMEOUT_SECS in the scheduler is 5 minutes, but the first
    // result is sent immediately on exit_code change (Unknown → 0).
    let metrics = wait_for_metrics(&h, comp_id, Duration::from_secs(60))
        .await
        .expect("metrics should be persisted within 60s");

    let obj = metrics
        .as_object()
        .expect("metrics column should hold a JSON object");
    assert_eq!(
        obj.get("active_connections").and_then(|v| v.as_f64()),
        Some(42.0),
        "active_connections should be extracted from perfdata; got {:?}",
        obj
    );
    assert_eq!(
        obj.get("queue_depth").and_then(|v| v.as_f64()),
        Some(7.0),
        "queue_depth should be extracted from perfdata; got {:?}",
        obj
    );
    assert_eq!(
        obj.get("cpu_usage").and_then(|v| v.as_f64()),
        Some(23.5),
        "cpu_usage with % UOM should be extracted as 23.5; got {:?}",
        obj
    );

    h.cleanup().await;
}

/// Poll `check_events.metrics` for the most recent non-null payload.
async fn wait_for_metrics(
    h: &harness::TestHarness,
    component_id: Uuid,
    timeout: Duration,
) -> Option<serde_json::Value> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let row: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT metrics FROM check_events \
             WHERE component_id = $1 AND metrics IS NOT NULL \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(component_id)
        .fetch_optional(&h.db_pool)
        .await
        .ok()
        .flatten();

        if let Some(v) = row {
            return Some(v);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    None
}
