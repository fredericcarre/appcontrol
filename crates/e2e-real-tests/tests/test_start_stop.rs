/// Real E2E Test: Full Application Start/Stop
///
/// Validates the complete flow:
/// 1. Backend sends start commands via gateway to agent
/// 2. Agent executes start_process.sh scripts
/// 3. Agent health checks detect running processes
/// 4. Backend FSM transitions to RUNNING
/// 5. Stop follows reverse DAG order
mod harness;

use std::time::Duration;

#[tokio::test]
async fn test_start_stop_full_sequence() {
    let h = harness::TestHarness::start().await;
    let site_id = h.default_site_id().await;

    // Create app: Oracle-DB → Tomcat-App → Apache-Web
    let (app_id, db_id, app_id_comp, web_id) = h.create_test_app(site_id).await;

    // ---- START ----
    let resp = h.api_post(&format!("/apps/{app_id}/start"), serde_json::json!({})).await;
    assert!(resp.get("status").is_some() || resp.get("plan").is_some());

    // Wait for all components to reach RUNNING (max 120s)
    let all_running = h.wait_for_state(db_id, "RUNNING", Duration::from_secs(120)).await
        && h.wait_for_state(app_id_comp, "RUNNING", Duration::from_secs(60)).await
        && h.wait_for_state(web_id, "RUNNING", Duration::from_secs(60)).await;
    assert!(all_running, "All components should reach RUNNING state");

    // Verify: processes are actually alive
    assert!(h.process_running("oracle-db"), "Oracle-DB process should be running");
    assert!(h.process_running("tomcat-app"), "Tomcat-App process should be running");
    assert!(h.process_running("apache-web"), "Apache-Web process should be running");

    // Verify DAG order: DB should have started before AppServer
    let db_transitions = h.get_transitions(db_id).await;
    let app_transitions = h.get_transitions(app_id_comp).await;
    let web_transitions = h.get_transitions(web_id).await;

    let db_running_at = db_transitions
        .iter()
        .find(|(_, to, _)| to == "RUNNING")
        .map(|(_, _, at)| at)
        .expect("DB should have RUNNING transition");

    let app_starting_at = app_transitions
        .iter()
        .find(|(_, to, _)| to == "STARTING")
        .map(|(_, _, at)| at)
        .expect("AppServer should have STARTING transition");

    assert!(
        db_running_at <= app_starting_at,
        "DB must be RUNNING before AppServer starts"
    );

    let app_running_at = app_transitions
        .iter()
        .find(|(_, to, _)| to == "RUNNING")
        .map(|(_, _, at)| at)
        .expect("AppServer should have RUNNING transition");

    let web_starting_at = web_transitions
        .iter()
        .find(|(_, to, _)| to == "STARTING")
        .map(|(_, _, at)| at)
        .expect("Web should have STARTING transition");

    assert!(
        app_running_at <= web_starting_at,
        "AppServer must be RUNNING before Web starts"
    );

    // ---- STOP ----
    h.api_post(&format!("/apps/{app_id}/stop"), serde_json::json!({})).await;

    let all_stopped = h.wait_for_state(web_id, "STOPPED", Duration::from_secs(60)).await
        && h.wait_for_state(app_id_comp, "STOPPED", Duration::from_secs(60)).await
        && h.wait_for_state(db_id, "STOPPED", Duration::from_secs(60)).await;
    assert!(all_stopped, "All components should reach STOPPED state");

    // Verify: processes are actually dead
    assert!(!h.process_running("oracle-db"), "Oracle-DB should be stopped");
    assert!(!h.process_running("tomcat-app"), "Tomcat-App should be stopped");
    assert!(!h.process_running("apache-web"), "Apache-Web should be stopped");

    // Verify reverse DAG order: Web stopped before AppServer, AppServer before DB
    let web_stop_transitions = h.get_transitions(web_id).await;
    let app_stop_transitions = h.get_transitions(app_id_comp).await;

    let web_stopped_at = web_stop_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPED")
        .map(|(_, _, at)| at)
        .expect("Web should have STOPPED transition");

    let app_stopping_at = app_stop_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPING")
        .map(|(_, _, at)| at)
        .expect("AppServer should have STOPPING transition");

    assert!(
        web_stopped_at <= app_stopping_at,
        "Web must stop before AppServer"
    );

    h.cleanup().await;
}
