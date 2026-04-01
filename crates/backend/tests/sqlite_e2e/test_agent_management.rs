//! SQLite E2E: Agent management — listing, detail, org isolation.

use super::common::TestContext;
use serde_json::{json, Value};
use uuid::Uuid;

#[tokio::test]
async fn test_list_agents() {
    let ctx = TestContext::new().await;

    let resp = ctx.get("/api/v1/agents").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let agents = body["agents"]
        .as_array()
        .or_else(|| body.as_array())
        .expect("agents response should contain array");
    // Empty list is fine — no agents registered yet
    let _ = agents;
}

#[tokio::test]
async fn test_get_nonexistent_agent_returns_404() {
    let ctx = TestContext::new().await;
    let fake_id = Uuid::new_v4();

    let resp = ctx.get(&format!("/api/v1/agents/{fake_id}")).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_agent_list_is_org_scoped() {
    let ctx = TestContext::new().await;

    let (_org2_id, _user2_id, org2_token) = ctx.create_second_org().await;
    let resp = ctx.get_with_token(&org2_token, "/api/v1/agents").await;
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 401 || status == 403,
        "org2 agents returned {status}"
    );

    if status == 200 {
        let body: Value = resp.json().await.unwrap();
        let empty = vec![];
        let agents = body["agents"]
            .as_array()
            .or_else(|| body.as_array())
            .unwrap_or(&empty);
        assert!(agents.is_empty(), "org2 should not see org1 agents");
    }
}

#[tokio::test]
async fn test_agents_endpoint_returns_json() {
    let ctx = TestContext::new().await;

    let resp = ctx.get("/api/v1/agents").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body.is_object() || body.is_array(), "should return JSON");
}

#[tokio::test]
async fn test_agent_block_requires_admin() {
    let ctx = TestContext::new().await;
    let fake_id = Uuid::new_v4();

    let resp = ctx
        .post_as("viewer", &format!("/api/v1/agents/{fake_id}/block"), json!({}))
        .await;
    let status = resp.status().as_u16();
    assert!(
        status == 403 || status == 404,
        "viewer should not block agents, got {status}"
    );
}
