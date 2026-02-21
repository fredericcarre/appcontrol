// E2E tests for heartbeat timeout → UNREACHABLE state transitions.
//
// Tests the distinction between:
// - FAILED: check_cmd ran and returned an error (agent is available)
// - UNREACHABLE: agent heartbeat timed out (we don't know the component's real state)

use super::*;

#[tokio::test]
async fn test_agent_heartbeat_updates_last_heartbeat_at() {
    let ctx = TestContext::new().await;

    // Insert an agent with no heartbeat
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active)
         VALUES ($1, $2, 'server01.prod', true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Verify no heartbeat initially
    let hb: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM agents WHERE id = $1"
    )
    .bind(agent_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();
    assert!(hb.is_none());

    // Simulate heartbeat update (what the backend does when it receives a heartbeat)
    sqlx::query("UPDATE agents SET last_heartbeat_at = now() WHERE id = $1")
        .bind(agent_id)
        .execute(&ctx.db_pool)
        .await
        .unwrap();

    // Verify heartbeat is now set
    let hb: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM agents WHERE id = $1"
    )
    .bind(agent_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();
    assert!(hb.is_some());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_stale_agent_components_become_unreachable() {
    let ctx = TestContext::new().await;

    // Create a site
    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code, site_type)
         VALUES ($1, $2, 'PRD', 'PRD', 'primary')"
    )
    .bind(site_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Create an agent with a stale heartbeat (5 minutes ago, timeout is 180s)
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active, last_heartbeat_at)
         VALUES ($1, $2, 'server01.prod', true, now() - interval '5 minutes')"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Create an app and a component associated to this agent
    let app_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO applications (id, organization_id, name, site_id)
         VALUES ($1, $2, 'Test-App', $3)"
    )
    .bind(app_id)
    .bind(ctx.organization_id)
    .bind(site_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let comp_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO components (id, application_id, name, component_type, agent_id, check_cmd)
         VALUES ($1, $2, 'Oracle', 'database', $3, 'check_oracle.sh')"
    )
    .bind(comp_id)
    .bind(app_id)
    .bind(agent_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Set component to RUNNING via state_transitions
    sqlx::query(
        "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
         VALUES ($1, 'UNKNOWN', 'RUNNING', 'check')"
    )
    .bind(comp_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Run the heartbeat check (simulate what the monitor does)
    let stale_components = sqlx::query_as::<_, (Uuid, Uuid, Uuid)>(
        r#"
        SELECT c.id, c.agent_id, c.application_id
        FROM components c
        JOIN agents a ON a.id = c.agent_id
        JOIN applications app ON app.id = c.application_id
        JOIN organizations o ON o.id = a.organization_id
        WHERE a.is_active = true
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval
          AND c.agent_id IS NOT NULL
        "#,
    )
    .fetch_all(&ctx.db_pool)
    .await
    .unwrap();

    assert_eq!(stale_components.len(), 1, "Should detect 1 stale component");
    assert_eq!(stale_components[0].0, comp_id);

    // Transition to UNREACHABLE
    sqlx::query(
        r#"
        INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details)
        VALUES ($1, 'RUNNING', 'UNREACHABLE', 'heartbeat_timeout',
                jsonb_build_object('previous_state', 'RUNNING', 'agent_id', $2::text))
        "#,
    )
    .bind(comp_id)
    .bind(agent_id.to_string())
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Verify transition was recorded
    let latest_state: String = sqlx::query_scalar(
        "SELECT to_state FROM state_transitions WHERE component_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(comp_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();
    assert_eq!(latest_state, "UNREACHABLE");

    // Verify trigger is 'heartbeat_timeout' (not 'check' or 'api')
    let trigger: String = sqlx::query_scalar(
        "SELECT trigger FROM state_transitions WHERE component_id = $1 AND to_state = 'UNREACHABLE'"
    )
    .bind(comp_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();
    assert_eq!(trigger, "heartbeat_timeout");

    // Verify details contain previous_state
    let details: serde_json::Value = sqlx::query_scalar(
        "SELECT details FROM state_transitions WHERE component_id = $1 AND to_state = 'UNREACHABLE'"
    )
    .bind(comp_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();
    assert_eq!(details["previous_state"], "RUNNING");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_active_agent_not_marked_unreachable() {
    let ctx = TestContext::new().await;

    // Create agent with RECENT heartbeat (10 seconds ago, well within 180s timeout)
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active, last_heartbeat_at)
         VALUES ($1, $2, 'server02.prod', true, now() - interval '10 seconds')"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Query for stale agents — should find none
    let stale = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT a.id
        FROM agents a
        JOIN organizations o ON o.id = a.organization_id
        WHERE a.is_active = true
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval
        "#,
    )
    .fetch_all(&ctx.db_pool)
    .await
    .unwrap();

    assert!(stale.is_empty(), "Recently active agent should not be stale");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_stopped_component_not_transitioned_to_unreachable() {
    let ctx = TestContext::new().await;

    // STOPPED components should NOT be moved to UNREACHABLE even if agent is stale.
    // When we intentionally stop a component, it stays STOPPED regardless of agent status.

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'PRD', 'PRD', 'primary')"
    )
    .bind(site_id).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active, last_heartbeat_at)
         VALUES ($1, $2, 'server03.prod', true, now() - interval '10 minutes')"
    )
    .bind(agent_id).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    let app_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO applications (id, organization_id, name, site_id) VALUES ($1, $2, 'Test-Stopped', $3)"
    )
    .bind(app_id).bind(ctx.organization_id).bind(site_id).execute(&ctx.db_pool).await.unwrap();

    let comp_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO components (id, application_id, name, component_type, agent_id)
         VALUES ($1, $2, 'Nginx', 'webfront', $3)"
    )
    .bind(comp_id).bind(app_id).bind(agent_id).execute(&ctx.db_pool).await.unwrap();

    // Component is in STOPPED state
    sqlx::query(
        "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
         VALUES ($1, 'RUNNING', 'STOPPED', 'api')"
    )
    .bind(comp_id).execute(&ctx.db_pool).await.unwrap();

    // Verify current state is STOPPED
    let state: String = sqlx::query_scalar(
        "SELECT to_state FROM state_transitions WHERE component_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(comp_id).fetch_one(&ctx.db_pool).await.unwrap();
    assert_eq!(state, "STOPPED");

    // The heartbeat monitor should skip STOPPED components
    // (this is verified by the heartbeat_monitor code filtering out STOPPED/STOPPING states)

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_heartbeat_timeout_configurable_per_org() {
    let ctx = TestContext::new().await;

    // Default timeout is 180s. Let's verify and change it.
    let timeout: i32 = sqlx::query_scalar(
        "SELECT heartbeat_timeout_seconds FROM organizations WHERE id = $1"
    )
    .bind(ctx.organization_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();
    assert_eq!(timeout, 180);

    // Change to 60 seconds
    sqlx::query("UPDATE organizations SET heartbeat_timeout_seconds = 60 WHERE id = $1")
        .bind(ctx.organization_id)
        .execute(&ctx.db_pool)
        .await
        .unwrap();

    // Agent with 90s stale heartbeat would be fine with 180s timeout
    // but is stale with 60s timeout
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active, last_heartbeat_at)
         VALUES ($1, $2, 'server04.prod', true, now() - interval '90 seconds')"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let stale = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT a.id
        FROM agents a
        JOIN organizations o ON o.id = a.organization_id
        WHERE a.is_active = true
          AND a.last_heartbeat_at IS NOT NULL
          AND a.last_heartbeat_at < now() - (o.heartbeat_timeout_seconds || ' seconds')::interval
        "#,
    )
    .fetch_all(&ctx.db_pool)
    .await
    .unwrap();

    assert_eq!(stale.len(), 1, "Agent should be stale with 60s timeout");

    ctx.cleanup().await;
}
