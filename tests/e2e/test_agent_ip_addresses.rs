// E2E tests for agent IP address support.
//
// Agents can now report both FQDN and IP addresses.
// The backend stores ip_addresses as JSONB on the agents table.
// Useful for Azure/cloud VMs where FQDN may not be available.

use super::*;

#[tokio::test]
async fn test_agent_stores_ip_addresses() {
    let ctx = TestContext::new().await;

    // Insert agent with IP addresses
    let agent_id = Uuid::new_v4();
    let ips = json!(["10.0.1.42", "172.16.0.5"]);
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, ip_addresses, is_active)
         VALUES ($1, $2, 'server01.prod.company.com', $3, true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .bind(&ips)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Retrieve via API
    let resp = ctx.get(&format!("/api/v1/agents/{agent_id}")).await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await;
    assert_eq!(body["hostname"], "server01.prod.company.com");
    assert_eq!(body["ip_addresses"][0], "10.0.1.42");
    assert_eq!(body["ip_addresses"][1], "172.16.0.5");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_agent_ip_addresses_default_empty() {
    let ctx = TestContext::new().await;

    // Agent with no IP addresses should have empty array
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, is_active)
         VALUES ($1, $2, 'legacy-agent.local', true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let resp = ctx.get(&format!("/api/v1/agents/{agent_id}")).await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await;
    assert_eq!(body["hostname"], "legacy-agent.local");
    // Default is empty array
    assert!(body["ip_addresses"].is_array());
    assert_eq!(body["ip_addresses"].as_array().unwrap().len(), 0);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_agent_register_updates_ip_addresses() {
    let ctx = TestContext::new().await;

    // Create agent with initial IPs
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, hostname, ip_addresses, is_active)
         VALUES ($1, $2, 'server02.prod', $3, true)"
    )
    .bind(agent_id)
    .bind(ctx.organization_id)
    .bind(json!(["10.0.1.1"]))
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Simulate agent re-registration with new IPs (what happens on agent reconnect)
    let new_ips = json!(["10.0.1.1", "10.0.2.100"]);
    sqlx::query(
        "UPDATE agents SET hostname = $2, ip_addresses = $3, last_heartbeat_at = now(), is_active = true WHERE id = $1"
    )
    .bind(agent_id)
    .bind("server02.prod.new-fqdn.com")
    .bind(&new_ips)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Verify updated
    let resp = ctx.get(&format!("/api/v1/agents/{agent_id}")).await;
    let body: Value = resp.json().await;
    assert_eq!(body["hostname"], "server02.prod.new-fqdn.com");
    assert_eq!(body["ip_addresses"].as_array().unwrap().len(), 2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_agent_list_includes_ip_addresses() {
    let ctx = TestContext::new().await;

    // Insert two agents
    for (hostname, ips) in [
        ("srv-prd-01.paris.company.com", json!(["10.0.1.10"])),
        ("srv-dr-01.lyon.company.com", json!(["10.0.2.20", "172.16.0.20"])),
    ] {
        sqlx::query(
            "INSERT INTO agents (id, organization_id, hostname, ip_addresses, is_active)
             VALUES ($1, $2, $3, $4, true)"
        )
        .bind(Uuid::new_v4())
        .bind(ctx.organization_id)
        .bind(hostname)
        .bind(&ips)
        .execute(&ctx.db_pool)
        .await
        .unwrap();
    }

    let resp = ctx.get("/api/v1/agents").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await;
    let agents = body["agents"].as_array().unwrap();
    assert!(agents.len() >= 2);

    // Both agents should have ip_addresses in the response
    for agent in agents {
        assert!(agent["ip_addresses"].is_array());
    }

    ctx.cleanup().await;
}
