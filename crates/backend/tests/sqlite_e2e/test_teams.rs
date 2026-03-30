/// SQLite E2E: Team CRUD (mirrors test_teams_crud.rs)
use super::common::TestContext;
use serde_json::{json, Value};

#[tokio::test]
async fn test_create_team() {
    let ctx = TestContext::new().await;
    let resp = ctx.post("/api/v1/teams", json!({"name": "DevOps"})).await;
    assert!(resp.status().is_success(), "Create team: {}", resp.status());
    let team: Value = resp.json().await.unwrap();
    assert_eq!(team["name"], "DevOps");
    assert!(team["id"].as_str().is_some());
}

#[tokio::test]
async fn test_list_teams() {
    let ctx = TestContext::new().await;
    ctx.post("/api/v1/teams", json!({"name": "Team-A"})).await;
    ctx.post("/api/v1/teams", json!({"name": "Team-B"})).await;

    let resp = ctx.get("/api/v1/teams").await;
    assert_eq!(resp.status(), 200);
    let teams: Vec<Value> = resp.json().await.unwrap();
    assert!(teams.len() >= 2, "Should have at least 2 teams");
}

#[tokio::test]
async fn test_update_team() {
    let ctx = TestContext::new().await;
    let resp = ctx.post("/api/v1/teams", json!({"name": "Old-Name"})).await;
    let team: Value = resp.json().await.unwrap();
    let team_id = team["id"].as_str().unwrap();

    let resp = ctx
        .put(
            &format!("/api/v1/teams/{team_id}"),
            json!({"name": "New-Name"}),
        )
        .await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_delete_team() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post("/api/v1/teams", json!({"name": "To-Delete"}))
        .await;
    let team: Value = resp.json().await.unwrap();
    let team_id = team["id"].as_str().unwrap();

    let resp = ctx.delete(&format!("/api/v1/teams/{team_id}")).await;
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn test_add_team_member() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .post("/api/v1/teams", json!({"name": "Members-Test"}))
        .await;
    let team: Value = resp.json().await.unwrap();
    let team_id = team["id"].as_str().unwrap();

    let resp = ctx
        .post(
            &format!("/api/v1/teams/{team_id}/members"),
            json!({"user_id": ctx.operator_user_id}),
        )
        .await;
    assert!(resp.status().is_success(), "Add member: {}", resp.status());
}
