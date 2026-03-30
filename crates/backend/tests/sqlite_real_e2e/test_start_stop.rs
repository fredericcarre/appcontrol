/// Real E2E Test: Full Application Start/Stop (SQLite version)
///
/// Validates the complete flow:
/// 1. Backend sends start commands via gateway to agent
/// 2. Agent executes start_process.sh scripts
/// 3. Agent health checks detect running processes
/// 4. Backend FSM transitions to RUNNING
/// 5. Stop follows reverse DAG order
use std::time::Duration;

#[tokio::test]
async fn test_start_stop_full_sequence() {
    let h = super::harness::TestHarness::start().await;
    let site_id = h.default_site_id().await;

    // Create app: Oracle-DB -> Tomcat-App -> Apache-Web
    let (app_id, db_id, app_srv_id, web_id) = h.create_test_app(site_id).await;

    // ---- START ----
    eprintln!("[test] Starting application {app_id}...");
    let resp = h
        .api_post(&format!("/apps/{app_id}/start"), serde_json::json!({}))
        .await;
    assert!(resp.get("status").is_some() || resp.get("plan").is_some());

    // Wait for all components to reach RUNNING (max 120s)
    let db_running = h
        .wait_for_state(db_id, "RUNNING", Duration::from_secs(120))
        .await;
    assert!(db_running, "Oracle-DB should reach RUNNING state");

    let app_running = h
        .wait_for_state(app_srv_id, "RUNNING", Duration::from_secs(60))
        .await;
    assert!(app_running, "Tomcat-App should reach RUNNING state");

    let web_running = h
        .wait_for_state(web_id, "RUNNING", Duration::from_secs(60))
        .await;
    assert!(web_running, "Apache-Web should reach RUNNING state");

    // Verify: processes are actually alive
    assert!(
        h.process_running("oracle-db"),
        "Oracle-DB process should be running"
    );
    assert!(
        h.process_running("tomcat-app"),
        "Tomcat-App process should be running"
    );
    assert!(
        h.process_running("apache-web"),
        "Apache-Web process should be running"
    );

    // Verify DAG order: DB should have started before AppServer
    let db_transitions = h.get_transitions(db_id).await;
    let app_transitions = h.get_transitions(app_srv_id).await;
    let web_transitions = h.get_transitions(web_id).await;

    let db_running_at = db_transitions
        .iter()
        .find(|(_, to, _)| to == "RUNNING")
        .map(|(_, _, at)| at.as_str())
        .expect("DB should have RUNNING transition");

    let app_starting_at = app_transitions
        .iter()
        .find(|(_, to, _)| to == "STARTING")
        .map(|(_, _, at)| at.as_str())
        .expect("AppServer should have STARTING transition");

    assert!(
        db_running_at <= app_starting_at,
        "DB must be RUNNING before AppServer starts (DB RUNNING at {db_running_at}, App STARTING at {app_starting_at})"
    );

    let app_running_at = app_transitions
        .iter()
        .find(|(_, to, _)| to == "RUNNING")
        .map(|(_, _, at)| at.as_str())
        .expect("AppServer should have RUNNING transition");

    let web_starting_at = web_transitions
        .iter()
        .find(|(_, to, _)| to == "STARTING")
        .map(|(_, _, at)| at.as_str())
        .expect("Web should have STARTING transition");

    assert!(
        app_running_at <= web_starting_at,
        "AppServer must be RUNNING before Web starts (App RUNNING at {app_running_at}, Web STARTING at {web_starting_at})"
    );

    // ---- STOP ----
    eprintln!("[test] Stopping application {app_id}...");
    h.api_post(&format!("/apps/{app_id}/stop"), serde_json::json!({}))
        .await;

    let web_stopped = h
        .wait_for_state(web_id, "STOPPED", Duration::from_secs(60))
        .await;
    assert!(web_stopped, "Apache-Web should reach STOPPED state");

    let app_stopped = h
        .wait_for_state(app_srv_id, "STOPPED", Duration::from_secs(60))
        .await;
    assert!(app_stopped, "Tomcat-App should reach STOPPED state");

    let db_stopped = h
        .wait_for_state(db_id, "STOPPED", Duration::from_secs(60))
        .await;
    assert!(db_stopped, "Oracle-DB should reach STOPPED state");

    // Verify: processes are actually dead
    assert!(
        !h.process_running("oracle-db"),
        "Oracle-DB should be stopped"
    );
    assert!(
        !h.process_running("tomcat-app"),
        "Tomcat-App should be stopped"
    );
    assert!(
        !h.process_running("apache-web"),
        "Apache-Web should be stopped"
    );

    // Verify reverse DAG order: Web stopped before AppServer, AppServer before DB
    let web_stop_transitions = h.get_transitions(web_id).await;
    let app_stop_transitions = h.get_transitions(app_srv_id).await;

    let web_stopped_at = web_stop_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPED")
        .map(|(_, _, at)| at.as_str())
        .expect("Web should have STOPPED transition");

    let app_stopping_at = app_stop_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPING")
        .map(|(_, _, at)| at.as_str())
        .expect("AppServer should have STOPPING transition");

    assert!(
        web_stopped_at <= app_stopping_at,
        "Web must stop before AppServer (Web STOPPED at {web_stopped_at}, App STOPPING at {app_stopping_at})"
    );

    eprintln!("[test] All assertions passed. Cleaning up...");
    h.cleanup().await;
}
