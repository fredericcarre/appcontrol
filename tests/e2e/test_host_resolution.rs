// E2E tests for host-based agent resolution.
//
// Users enter a "host" (FQDN or IP) when creating components in the map.
// The backend resolves this to an agent_id by matching agents.hostname or agents.ip_addresses.
// No multicast: one component → one host → one agent.

use super::*;

#[tokio::test]
async fn test_create_component_with_host_field() {
    let ctx = TestContext::new().await;

    let app_id = ctx.create_payments_app().await;

    // Create a component with the new "host" field
    let resp = ctx.post(&format!("/api/v1/apps/{app_id}/components"), json!({
        "name": "NewComp",
        "component_type": "service",
        "host": "srv-new.prod.company.com",
        "check_cmd": "check.sh",
        "start_cmd": "start.sh",
        "stop_cmd": "stop.sh",
    })).await;
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await;
    assert_eq!(body["host"], "srv-new.prod.company.com");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_create_component_with_hostname_alias() {
    let ctx = TestContext::new().await;

    let app_id = ctx.create_payments_app().await;

    // Old callers use "hostname" — should be accepted as alias for "host"
    let resp = ctx.post(&format!("/api/v1/apps/{app_id}/components"), json!({
        "name": "LegacyComp",
        "component_type": "service",
        "hostname": "srv-legacy.prod.company.com",
        "check_cmd": "check.sh",
        "start_cmd": "start.sh",
        "stop_cmd": "stop.sh",
    })).await;
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await;
    assert_eq!(body["host"], "srv-legacy.prod.company.com");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_host_resolves_to_agent_by_hostname() {
    let ctx = TestContext::new().await;

    // Create an agent with a specific hostname
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active)
         VALUES ($1, $2, 'srv-oracle.prod.paris.com', true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let app_id = ctx.create_payments_app().await;

    // Create component referencing the agent's hostname
    let resp = ctx.post(&format!("/api/v1/apps/{app_id}/components"), json!({
        "name": "Oracle-Resolved",
        "component_type": "database",
        "host": "srv-oracle.prod.paris.com",
        "check_cmd": "check.sh",
        "start_cmd": "start.sh",
        "stop_cmd": "stop.sh",
    })).await;
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await;
    assert_eq!(body["host"], "srv-oracle.prod.paris.com");
    // agent_id should have been resolved
    assert_eq!(body["agent_id"], agent_id.to_string());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_host_resolves_to_agent_by_ip() {
    let ctx = TestContext::new().await;

    // Create an agent with IP addresses
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, ip_addresses, is_active)
         VALUES ($1, $2, 'azure-vm-1234.internal', $3, true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .bind(json!(["10.0.1.42", "172.16.0.5"]))
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let app_id = ctx.create_payments_app().await;

    // Create component using the agent's IP instead of hostname
    let resp = ctx.post(&format!("/api/v1/apps/{app_id}/components"), json!({
        "name": "DB-ByIP",
        "component_type": "database",
        "host": "10.0.1.42",
        "check_cmd": "check.sh",
        "start_cmd": "start.sh",
        "stop_cmd": "stop.sh",
    })).await;
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await;
    assert_eq!(body["host"], "10.0.1.42");
    // agent_id should have been resolved via IP match
    assert_eq!(body["agent_id"], agent_id.to_string());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_host_no_match_leaves_agent_null() {
    let ctx = TestContext::new().await;

    let app_id = ctx.create_payments_app().await;

    // Create component with a host that doesn't match any agent
    let resp = ctx.post(&format!("/api/v1/apps/{app_id}/components"), json!({
        "name": "Orphan",
        "component_type": "service",
        "host": "srv-unknown.nowhere.com",
        "check_cmd": "check.sh",
        "start_cmd": "start.sh",
        "stop_cmd": "stop.sh",
    })).await;
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await;
    assert_eq!(body["host"], "srv-unknown.nowhere.com");
    // No agent match → agent_id is null
    assert!(body["agent_id"].is_null());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_late_binding_agent_resolves_pending_components() {
    let ctx = TestContext::new().await;

    let app_id = ctx.create_payments_app().await;

    // Create component BEFORE the agent exists
    let resp = ctx.post(&format!("/api/v1/apps/{app_id}/components"), json!({
        "name": "LateBind",
        "component_type": "service",
        "host": "srv-future.prod.com",
        "check_cmd": "check.sh",
        "start_cmd": "start.sh",
        "stop_cmd": "stop.sh",
    })).await;
    assert_eq!(resp.status(), 201);
    let comp: Value = resp.json().await;
    let comp_id: Uuid = comp["id"].as_str().unwrap().parse().unwrap();
    // No agent yet
    assert!(comp["agent_id"].is_null());

    // Now create the agent and simulate registration
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active)
         VALUES ($1, $2, 'srv-future.prod.com', true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Simulate what process_agent_message does on Register:
    // resolve_components_for_agent
    appcontrol_backend::api::components::resolve_components_for_agent(
        &ctx.db_pool,
        agent_id,
        "srv-future.prod.com",
        &[],
    ).await;

    // Verify component now has agent_id set
    let resolved_agent: Option<Uuid> = sqlx::query_scalar(
        "SELECT agent_id FROM components WHERE id = $1"
    )
    .bind(comp_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    assert_eq!(resolved_agent, Some(agent_id), "Late binding should resolve agent_id");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_no_multicast_first_agent_wins() {
    let ctx = TestContext::new().await;

    // Two agents with the same IP (shouldn't happen, but test the no-multicast guarantee)
    let agent1 = Uuid::new_v4();
    let agent2 = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, ip_addresses, is_active, created_at)
         VALUES ($1, $2, 'host1.com', $3, true, now() - interval '1 hour')"
    )
    .bind(agent1)
    .bind(ctx.organization_id)
    .bind(json!(["10.0.0.1"]))
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, ip_addresses, is_active, created_at)
         VALUES ($1, $2, 'host2.com', $3, true, now())"
    )
    .bind(agent2)
    .bind(ctx.organization_id)
    .bind(json!(["10.0.0.1"]))
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Resolve — should get agent1 (first by created_at)
    let resolved = appcontrol_backend::api::components::resolve_host_to_agent(
        &ctx.db_pool, "10.0.0.1"
    ).await;

    assert_eq!(resolved, Some(agent1), "No multicast: first agent (by created_at) wins");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_component_list_includes_host() {
    let ctx = TestContext::new().await;

    let app_id = ctx.create_payments_app().await;

    // All components created by create_payments_app use "hostname" in JSON
    // (backward compat alias) — they should have host set
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/components")).await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await;
    let components = body["components"].as_array().unwrap();
    assert!(!components.is_empty());

    // Each component should have the host field in the response
    for comp in components {
        assert!(comp.get("host").is_some(), "Component response should include 'host' field");
    }

    ctx.cleanup().await;
}
