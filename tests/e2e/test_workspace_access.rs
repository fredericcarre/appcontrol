// E2E tests for workspace-based site/zone access control.
//
// Workspaces control which sites (and thus which gateways/agents/machines)
// a user or team can see and operate on. This prevents users from accessing
// machines outside their authorized scope.

use super::*;

#[tokio::test]
async fn test_create_workspace() {
    let ctx = TestContext::new().await;

    let resp = ctx.post("/api/v1/workspaces", json!({
        "name": "DBA Production",
        "description": "Database administrators - production access",
    })).await;
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await;
    assert_eq!(body["name"], "DBA Production");
    assert!(body["id"].is_string());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_create_workspace_requires_admin() {
    let ctx = TestContext::new().await;

    // Operator should not be able to create workspaces
    let resp = ctx.post_as("operator", "/api/v1/workspaces", json!({
        "name": "Unauthorized Workspace",
    })).await;
    assert_eq!(resp.status(), 403);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_list_workspaces() {
    let ctx = TestContext::new().await;

    // Create two workspaces
    ctx.post("/api/v1/workspaces", json!({"name": "WS-PRD"})).await;
    ctx.post("/api/v1/workspaces", json!({"name": "WS-DEV"})).await;

    let resp = ctx.get("/api/v1/workspaces").await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await;
    let workspaces = body["workspaces"].as_array().unwrap();
    assert!(workspaces.len() >= 2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_workspace_site_binding() {
    let ctx = TestContext::new().await;

    // Create workspace
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "PRD-Access"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();

    // Create sites
    let site_prd = Uuid::new_v4();
    let site_dr = Uuid::new_v4();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'PRD Paris', 'PRD', 'primary')")
        .bind(site_prd).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'DR Lyon', 'DR', 'dr')")
        .bind(site_dr).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    // Bind site PRD to workspace
    let resp = ctx.post(&format!("/api/v1/workspaces/{ws_id}/sites"), json!({
        "site_id": site_prd,
    })).await;
    assert_eq!(resp.status(), 201);

    // List workspace sites — should contain PRD only
    let resp = ctx.get(&format!("/api/v1/workspaces/{ws_id}/sites")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await;
    let sites = body["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0]["code"], "PRD");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_workspace_member_user() {
    let ctx = TestContext::new().await;

    // Create workspace
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "DBA-WS"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();

    // Add operator as member
    let resp = ctx.post(&format!("/api/v1/workspaces/{ws_id}/members"), json!({
        "user_id": ctx.operator_user_id,
        "role": "member",
    })).await;
    assert_eq!(resp.status(), 201);

    // List members
    let resp = ctx.get(&format!("/api/v1/workspaces/{ws_id}/members")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await;
    let members = body["members"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["user_id"], ctx.operator_user_id.to_string());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_workspace_member_team() {
    let ctx = TestContext::new().await;

    // Create a team with operator
    let team_id = ctx.create_team("DBA-Team", vec![ctx.operator_user_id]).await;

    // Create workspace and add team
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "Team-WS"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();

    let resp = ctx.post(&format!("/api/v1/workspaces/{ws_id}/members"), json!({
        "team_id": team_id,
        "role": "member",
    })).await;
    assert_eq!(resp.status(), 201);

    // Verify member was added
    let resp = ctx.get(&format!("/api/v1/workspaces/{ws_id}/members")).await;
    let body: Value = resp.json().await;
    let members = body["members"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["team_id"], team_id.to_string());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_workspace_site_access_control() {
    let ctx = TestContext::new().await;

    // Set up: 2 sites, 2 workspaces, each with 1 site
    let site_prd = Uuid::new_v4();
    let site_staging = Uuid::new_v4();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'PRD', 'PRD', 'primary')")
        .bind(site_prd).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'Staging', 'STG', 'staging')")
        .bind(site_staging).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    // Workspace for PRD team
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "PRD-Team"})).await;
    let ws_prd: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();
    ctx.post(&format!("/api/v1/workspaces/{ws_prd}/sites"), json!({"site_id": site_prd})).await;
    ctx.post(&format!("/api/v1/workspaces/{ws_prd}/members"), json!({"user_id": ctx.operator_user_id})).await;

    // Workspace for STG team
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "STG-Team"})).await;
    let ws_stg: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();
    ctx.post(&format!("/api/v1/workspaces/{ws_stg}/sites"), json!({"site_id": site_staging})).await;
    ctx.post(&format!("/api/v1/workspaces/{ws_stg}/members"), json!({"user_id": ctx.editor_user_id})).await;

    // Verify can_access_site for operator → PRD = true, STG = false
    let can_prd = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.operator_user_id, site_prd, ctx.organization_id, false,
    ).await;
    assert!(can_prd, "Operator should have access to PRD");

    let can_stg = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.operator_user_id, site_staging, ctx.organization_id, false,
    ).await;
    assert!(!can_stg, "Operator should NOT have access to Staging");

    // Verify can_access_site for editor → STG = true, PRD = false
    let can_stg = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.editor_user_id, site_staging, ctx.organization_id, false,
    ).await;
    assert!(can_stg, "Editor should have access to Staging");

    let can_prd = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.editor_user_id, site_prd, ctx.organization_id, false,
    ).await;
    assert!(!can_prd, "Editor should NOT have access to PRD");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_admin_bypasses_workspace_restrictions() {
    let ctx = TestContext::new().await;

    // Set up site and workspace, but DON'T add admin to any workspace
    let site_prd = Uuid::new_v4();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'PRD', 'PRD', 'primary')")
        .bind(site_prd).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    let resp = ctx.post("/api/v1/workspaces", json!({"name": "Restricted"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();
    ctx.post(&format!("/api/v1/workspaces/{ws_id}/sites"), json!({"site_id": site_prd})).await;
    // Only add operator, NOT admin
    ctx.post(&format!("/api/v1/workspaces/{ws_id}/members"), json!({"user_id": ctx.operator_user_id})).await;

    // Admin should still have access (is_org_admin = true bypasses workspace check)
    let can = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.admin_user_id, site_prd, ctx.organization_id, true,
    ).await;
    assert!(can, "Admin should ALWAYS have access regardless of workspace membership");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_no_workspaces_configured_means_open_access() {
    let ctx = TestContext::new().await;

    // No workspace_sites configured at all → everyone has access to everything
    let site_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'Open', 'OPEN', 'primary')")
        .bind(site_id).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    let can = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.viewer_user_id, site_id, ctx.organization_id, false,
    ).await;
    assert!(can, "With no workspace-site config, all users should have access");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_team_workspace_membership_grants_site_access() {
    let ctx = TestContext::new().await;

    // Create team and add operator
    let team_id = ctx.create_team("Ops-Team", vec![ctx.operator_user_id]).await;

    // Create site
    let site_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'TeamSite', 'TS', 'primary')")
        .bind(site_id).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    // Create workspace, bind site, add TEAM (not user directly)
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "Team-WS"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();
    ctx.post(&format!("/api/v1/workspaces/{ws_id}/sites"), json!({"site_id": site_id})).await;
    ctx.post(&format!("/api/v1/workspaces/{ws_id}/members"), json!({"team_id": team_id})).await;

    // Operator should have access via team membership
    let can = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.operator_user_id, site_id, ctx.organization_id, false,
    ).await;
    assert!(can, "Operator should have site access via team→workspace membership");

    // Viewer (not in team) should NOT have access
    let can = appcontrol_backend::core::permissions::can_access_site(
        &ctx.db_pool, ctx.viewer_user_id, site_id, ctx.organization_id, false,
    ).await;
    assert!(!can, "Viewer not in team should NOT have site access");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_delete_workspace() {
    let ctx = TestContext::new().await;

    // Create workspace
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "Temp-WS"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();

    // Delete it
    let resp = ctx.delete_as("admin", &format!("/api/v1/workspaces/{ws_id}")).await;
    assert_eq!(resp.status(), 204);

    // Verify it's gone
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE id = $1")
        .bind(ws_id).fetch_one(&ctx.db_pool).await.unwrap();
    assert_eq!(count, 0);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_remove_workspace_site() {
    let ctx = TestContext::new().await;

    // Create workspace + site
    let resp = ctx.post("/api/v1/workspaces", json!({"name": "RmSite-WS"})).await;
    let ws_id: Uuid = resp.json::<Value>().await["id"].as_str().unwrap().parse().unwrap();

    let site_id = Uuid::new_v4();
    sqlx::query("INSERT INTO sites (id, organization_id, name, code, site_type) VALUES ($1, $2, 'X', 'X', 'primary')")
        .bind(site_id).bind(ctx.organization_id).execute(&ctx.db_pool).await.unwrap();

    ctx.post(&format!("/api/v1/workspaces/{ws_id}/sites"), json!({"site_id": site_id})).await;

    // Remove site from workspace
    let resp = ctx.delete_as("admin", &format!("/api/v1/workspaces/{ws_id}/sites/{site_id}")).await;
    assert_eq!(resp.status(), 204);

    // Verify removed
    let resp = ctx.get(&format!("/api/v1/workspaces/{ws_id}/sites")).await;
    let body: Value = resp.json().await;
    assert!(body["sites"].as_array().unwrap().is_empty());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_workspace_access_audited() {
    let ctx = TestContext::new().await;

    // Create workspace — should be logged
    ctx.post("/api/v1/workspaces", json!({"name": "Audited-WS"})).await;

    let logs = ctx.get_all_action_logs().await;
    let ws_logs: Vec<_> = logs.iter().filter(|l| l.action == "create_workspace").collect();
    assert!(!ws_logs.is_empty(), "Workspace creation should be audited");

    ctx.cleanup().await;
}
