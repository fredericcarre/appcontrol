/// Real E2E Test: Failure Detection
///
/// Validates that when a process is killed:
/// 1. Agent's health check detects the failure
/// 2. Backend FSM transitions the component to FAILED
/// 3. State transitions are recorded
mod harness;

use std::time::Duration;

#[tokio::test]
async fn test_failure_detection() {
    let h = harness::TestHarness::start().await;
    let site_id = h.default_site_id().await;

    // Create and start app
    let (app_id, db_id, app_id_comp, web_id) = h.create_test_app(site_id).await;

    h.api_post(&format!("/apps/{app_id}/start"), serde_json::json!({})).await;

    // Wait for all RUNNING
    assert!(
        h.wait_for_state(db_id, "RUNNING", Duration::from_secs(120)).await
            && h.wait_for_state(app_id_comp, "RUNNING", Duration::from_secs(60)).await
            && h.wait_for_state(web_id, "RUNNING", Duration::from_secs(60)).await,
        "All components should reach RUNNING"
    );

    // Kill the AppServer process (simulate crash)
    h.kill_process("tomcat-app");
    assert!(!h.process_running("tomcat-app"), "Tomcat should be dead after kill");

    // Wait for agent's health check to detect the failure
    // Check interval is ~30s in the scheduler, so allow up to 90s
    let detected = h.wait_for_state(app_id_comp, "FAILED", Duration::from_secs(90)).await;
    assert!(detected, "Agent should detect Tomcat crash and transition to FAILED");

    // Verify state transitions recorded
    let transitions = h.get_transitions(app_id_comp).await;
    let has_running_to_failed = transitions
        .iter()
        .any(|(from, to, _)| from == "RUNNING" && to == "FAILED");
    assert!(
        has_running_to_failed,
        "Should have RUNNING→FAILED transition for crashed component"
    );

    // DB should still be RUNNING (unaffected)
    let db_state = h.get_component_state(db_id).await;
    assert_eq!(db_state, "RUNNING", "DB should still be running");
    assert!(h.process_running("oracle-db"), "DB process should still be alive");

    h.cleanup().await;
}
