/// Real E2E Test: Full Application Start/Stop (SQLite version)
///
/// Validates the complete flow with state coherence:
/// 1. Backend sends start commands via gateway to agent
/// 2. Agent executes start_process.sh scripts
/// 3. Agent health checks detect running processes
/// 4. Backend FSM transitions: STOPPED → STARTING → RUNNING (no skips)
/// 5. Stop follows reverse DAG order: RUNNING → STOPPING → STOPPED
/// 6. State transitions are coherent (no invalid jumps)
use std::time::Duration;

/// Verify FSM state coherence: each component must follow valid transition paths.
/// Start path: UNKNOWN/STOPPED → STARTING → RUNNING
/// Stop path: RUNNING → STOPPING → STOPPED
fn verify_start_transitions(transitions: &[(String, String, String)], name: &str) {
    assert!(
        !transitions.is_empty(),
        "{name} should have state transitions"
    );

    // Find the STARTING transition
    let has_starting = transitions.iter().any(|(_, to, _)| to == "STARTING");
    assert!(
        has_starting,
        "{name} must pass through STARTING state (no skip from STOPPED to RUNNING)"
    );

    // Find the RUNNING transition
    let has_running = transitions.iter().any(|(_, to, _)| to == "RUNNING");
    assert!(has_running, "{name} must reach RUNNING state");

    // STARTING must come before RUNNING
    let starting_idx = transitions
        .iter()
        .position(|(_, to, _)| to == "STARTING")
        .unwrap();
    let running_idx = transitions
        .iter()
        .position(|(_, to, _)| to == "RUNNING")
        .unwrap();
    assert!(
        starting_idx < running_idx,
        "{name}: STARTING (idx {starting_idx}) must come before RUNNING (idx {running_idx})"
    );

    // The RUNNING transition must come from STARTING
    let (from, to, _) = &transitions[running_idx];
    assert_eq!(
        from, "STARTING",
        "{name}: RUNNING transition must come from STARTING, got from={from} to={to}"
    );
}

fn verify_stop_transitions(transitions: &[(String, String, String)], name: &str) {
    // Find the STOPPING transition (in the stop phase — after last RUNNING)
    let last_running_idx = transitions
        .iter()
        .rposition(|(_, to, _)| to == "RUNNING")
        .expect(&format!("{name} should have RUNNING before stop"));

    let stop_transitions = &transitions[last_running_idx..];

    let has_stopping = stop_transitions.iter().any(|(_, to, _)| to == "STOPPING");
    assert!(
        has_stopping,
        "{name} must pass through STOPPING state (no skip from RUNNING to STOPPED)"
    );

    let has_stopped = stop_transitions.iter().any(|(_, to, _)| to == "STOPPED");
    assert!(has_stopped, "{name} must reach STOPPED state");

    // STOPPING must come before STOPPED
    let stopping_idx = stop_transitions
        .iter()
        .position(|(_, to, _)| to == "STOPPING")
        .unwrap();
    let stopped_idx = stop_transitions
        .iter()
        .position(|(_, to, _)| to == "STOPPED")
        .unwrap();
    assert!(
        stopping_idx < stopped_idx,
        "{name}: STOPPING must come before STOPPED"
    );
}

#[tokio::test]
#[ignore = "requires pre-built binaries — run with --ignored"]
async fn test_start_stop_full_sequence() {
    let h = super::harness::TestHarness::start().await;
    let site_id = h.default_site_id().await;

    // Create app: Oracle-DB → Tomcat-App → Apache-Web
    let (app_id, db_id, app_srv_id, web_id) = h.create_test_app(site_id).await;

    // ---- START ----
    eprintln!("[test] Starting application {app_id}...");
    let resp = h
        .api_post(&format!("/apps/{app_id}/start"), serde_json::json!({}))
        .await;
    assert!(resp.get("status").is_some() || resp.get("plan").is_some());

    // Wait for all components to reach RUNNING (max 120s)
    assert!(
        h.wait_for_state(db_id, "RUNNING", Duration::from_secs(120))
            .await,
        "Oracle-DB should reach RUNNING"
    );
    assert!(
        h.wait_for_state(app_srv_id, "RUNNING", Duration::from_secs(60))
            .await,
        "Tomcat-App should reach RUNNING"
    );
    assert!(
        h.wait_for_state(web_id, "RUNNING", Duration::from_secs(60))
            .await,
        "Apache-Web should reach RUNNING"
    );

    // Verify: processes are actually alive
    assert!(h.process_running("oracle-db"), "Oracle-DB process alive");
    assert!(h.process_running("tomcat-app"), "Tomcat-App process alive");
    assert!(h.process_running("apache-web"), "Apache-Web process alive");

    // ---- STATE COHERENCE: START ----
    let db_transitions = h.get_transitions(db_id).await;
    let app_transitions = h.get_transitions(app_srv_id).await;
    let web_transitions = h.get_transitions(web_id).await;

    eprintln!("[test] DB transitions: {:?}", db_transitions);
    eprintln!("[test] App transitions: {:?}", app_transitions);
    eprintln!("[test] Web transitions: {:?}", web_transitions);

    // Each component must follow STOPPED→STARTING→RUNNING (no skips)
    verify_start_transitions(&db_transitions, "Oracle-DB");
    verify_start_transitions(&app_transitions, "Tomcat-App");
    verify_start_transitions(&web_transitions, "Apache-Web");

    // ---- DAG ORDER: START ----
    let db_running_at = db_transitions
        .iter()
        .find(|(_, to, _)| to == "RUNNING")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    let app_starting_at = app_transitions
        .iter()
        .find(|(_, to, _)| to == "STARTING")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    assert!(
        db_running_at <= app_starting_at,
        "DB RUNNING ({db_running_at}) must be before App STARTING ({app_starting_at})"
    );

    let app_running_at = app_transitions
        .iter()
        .find(|(_, to, _)| to == "RUNNING")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    let web_starting_at = web_transitions
        .iter()
        .find(|(_, to, _)| to == "STARTING")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    assert!(
        app_running_at <= web_starting_at,
        "App RUNNING ({app_running_at}) must be before Web STARTING ({web_starting_at})"
    );

    // ---- STOP ----
    eprintln!("[test] Stopping application {app_id}...");
    h.api_post(&format!("/apps/{app_id}/stop"), serde_json::json!({}))
        .await;

    assert!(
        h.wait_for_state(web_id, "STOPPED", Duration::from_secs(60))
            .await,
        "Apache-Web should reach STOPPED"
    );
    assert!(
        h.wait_for_state(app_srv_id, "STOPPED", Duration::from_secs(60))
            .await,
        "Tomcat-App should reach STOPPED"
    );
    assert!(
        h.wait_for_state(db_id, "STOPPED", Duration::from_secs(60))
            .await,
        "Oracle-DB should reach STOPPED"
    );

    // Verify: processes are actually dead
    assert!(!h.process_running("oracle-db"), "Oracle-DB should be dead");
    assert!(
        !h.process_running("tomcat-app"),
        "Tomcat-App should be dead"
    );
    assert!(
        !h.process_running("apache-web"),
        "Apache-Web should be dead"
    );

    // ---- STATE COHERENCE: STOP ----
    // Re-fetch transitions (now includes stop transitions)
    let db_transitions = h.get_transitions(db_id).await;
    let app_transitions = h.get_transitions(app_srv_id).await;
    let web_transitions = h.get_transitions(web_id).await;

    // Each component must follow RUNNING→STOPPING→STOPPED (no skips)
    verify_stop_transitions(&db_transitions, "Oracle-DB");
    verify_stop_transitions(&app_transitions, "Tomcat-App");
    verify_stop_transitions(&web_transitions, "Apache-Web");

    // ---- DAG ORDER: STOP (reverse) ----
    // Web must stop before AppServer, AppServer before DB
    let web_stopped_at = web_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPED")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    let app_stopping_at = app_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPING")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    assert!(
        web_stopped_at <= app_stopping_at,
        "Web STOPPED ({web_stopped_at}) must be before App STOPPING ({app_stopping_at})"
    );

    let app_stopped_at = app_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPED")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    let db_stopping_at = db_transitions
        .iter()
        .rev()
        .find(|(_, to, _)| to == "STOPPING")
        .map(|(_, _, at)| at.as_str())
        .unwrap();
    assert!(
        app_stopped_at <= db_stopping_at,
        "App STOPPED ({app_stopped_at}) must be before DB STOPPING ({db_stopping_at})"
    );

    eprintln!("[test] All start/stop assertions passed with full state coherence.");
    h.cleanup().await;
}
