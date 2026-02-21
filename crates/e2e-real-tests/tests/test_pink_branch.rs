/// Real E2E Test: Pink Branch Remediation (Smart Start)
///
/// Validates the AppControl v1 "smart start" logic:
/// 1. Start app (all 3 components RUNNING)
/// 2. Kill AppServer (simulate crash)
/// 3. Wait for agent to detect FAILED
/// 4. User does POST /apps/{id}/start (regular start, NOT start-branch)
/// 5. Smart start detects pink branch:
///    - DB is RUNNING → skip (untouched)
///    - AppServer is FAILED → pink branch root
///    - Apache-Web (child of AppServer) is stopped first, then both restart
/// 6. Verify: DB was NOT restarted (no new transitions)
/// 7. Verify: AppServer restarted BEFORE Apache-Web (DAG order)
/// 8. Verify: All components RUNNING at the end
mod harness;

use std::time::Duration;

#[tokio::test]
async fn test_pink_branch_remediation() {
    let h = harness::TestHarness::start().await;
    let site_id = h.default_site_id().await;

    // Create app: Oracle-DB → Tomcat-App → Apache-Web
    let (app_id, db_id, app_id_comp, web_id) = h.create_test_app(site_id).await;

    // ---- PHASE 1: Start everything ----
    h.api_post(&format!("/apps/{app_id}/start"), serde_json::json!({}))
        .await;

    // Wait for all RUNNING
    assert!(
        h.wait_for_state(db_id, "RUNNING", Duration::from_secs(120))
            .await
            && h.wait_for_state(app_id_comp, "RUNNING", Duration::from_secs(60))
                .await
            && h.wait_for_state(web_id, "RUNNING", Duration::from_secs(60))
                .await,
        "All components should reach RUNNING"
    );

    // All processes alive
    assert!(h.process_running("oracle-db"), "DB should be running");
    assert!(h.process_running("tomcat-app"), "AppServer should be running");
    assert!(h.process_running("apache-web"), "Web should be running");

    // Record timestamp BEFORE the crash — we'll use this to check that DB
    // had no new transitions after this point.
    let before_crash = chrono::Utc::now();

    // ---- PHASE 2: Simulate crash ----
    h.kill_process("tomcat-app");
    assert!(
        !h.process_running("tomcat-app"),
        "Tomcat should be dead after kill"
    );

    // Wait for agent's health check to detect the failure
    let detected = h
        .wait_for_state(app_id_comp, "FAILED", Duration::from_secs(90))
        .await;
    assert!(
        detected,
        "Agent should detect Tomcat crash and transition to FAILED"
    );

    // Record timestamp after detection, before remediation
    let before_remediation = chrono::Utc::now();

    // ---- PHASE 3: Smart start (pink branch remediation) ----
    // This is a REGULAR start — the smart start logic should:
    // 1. See DB is RUNNING → skip
    // 2. See AppServer is FAILED → pink branch root
    // 3. Stop Apache-Web first (child of AppServer), then restart AppServer, then Apache-Web
    h.api_post(&format!("/apps/{app_id}/start"), serde_json::json!({}))
        .await;

    // Wait for AppServer and Web to come back to RUNNING
    let remediated = h
        .wait_for_state(app_id_comp, "RUNNING", Duration::from_secs(120))
        .await
        && h.wait_for_state(web_id, "RUNNING", Duration::from_secs(60))
            .await;
    assert!(remediated, "Pink branch should remediate: AppServer + Web should be RUNNING again");

    // ---- PHASE 4: Verify smart start behavior ----

    // 4a. DB should NOT have been restarted (no new transitions since before_crash)
    let db_new_transitions = h.count_transitions_since(db_id, before_crash).await;
    assert_eq!(
        db_new_transitions, 0,
        "DB should have ZERO new transitions — smart start should skip RUNNING components. Got {db_new_transitions}"
    );

    // 4b. DB process should still be the same one (not restarted)
    assert!(
        h.process_running("oracle-db"),
        "DB process should still be alive (never touched)"
    );

    // 4c. AppServer should have restarted (new transitions after remediation)
    let app_new_transitions = h.count_transitions_since(app_id_comp, before_remediation).await;
    assert!(
        app_new_transitions > 0,
        "AppServer should have new transitions after remediation"
    );

    // 4d. Web should have new transitions (stopped then restarted)
    let web_new_transitions = h.count_transitions_since(web_id, before_remediation).await;
    assert!(
        web_new_transitions > 0,
        "Apache-Web should have new transitions (stopped then restarted as part of pink branch)"
    );

    // 4e. Verify DAG order: AppServer restarted BEFORE Apache-Web
    let app_transitions = h.get_transitions(app_id_comp).await;
    let web_transitions = h.get_transitions(web_id).await;

    // Find the latest RUNNING transition for AppServer (after remediation)
    let app_running_at = app_transitions
        .iter()
        .rev()
        .find(|(_, to, ts)| to == "RUNNING" && *ts > before_remediation)
        .map(|(_, _, at)| *at)
        .expect("AppServer should have a new RUNNING transition after remediation");

    // Find the latest STARTING transition for Web (after remediation)
    let web_starting_at = web_transitions
        .iter()
        .rev()
        .find(|(_, to, ts)| to == "STARTING" && *ts > before_remediation)
        .map(|(_, _, at)| *at)
        .expect("Apache-Web should have a STARTING transition after remediation");

    assert!(
        app_running_at <= web_starting_at,
        "AppServer must be RUNNING before Apache-Web starts (DAG order in pink branch). \
         AppServer RUNNING at {:?}, Web STARTING at {:?}",
        app_running_at,
        web_starting_at
    );

    // 4f. All processes should be alive at the end
    assert!(h.process_running("oracle-db"), "DB process should be alive");
    assert!(
        h.process_running("tomcat-app"),
        "AppServer process should be alive after remediation"
    );
    assert!(
        h.process_running("apache-web"),
        "Apache-Web process should be alive after remediation"
    );

    // 4g. All component states should be RUNNING
    assert_eq!(h.get_component_state(db_id).await, "RUNNING");
    assert_eq!(h.get_component_state(app_id_comp).await, "RUNNING");
    assert_eq!(h.get_component_state(web_id).await, "RUNNING");

    h.cleanup().await;
}
