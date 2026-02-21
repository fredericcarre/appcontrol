//! E2E tests for application variables, component groups, component links,
//! and command input parameters.

use super::*;

#[tokio::test]
async fn test_variable_crud() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Create variables
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/variables"),
            json!({"name": "APP_HOST", "value": "10.0.0.1", "description": "Application host"}),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let var: Value = resp.json().await.unwrap();
    let var_id = var["id"].as_str().unwrap();
    assert_eq!(var["name"], "APP_HOST");
    assert_eq!(var["value"], "10.0.0.1");

    // Create a second variable
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/variables"),
            json!({"name": "APP_PORT", "value": "8080"}),
        )
        .await;
    assert_eq!(resp.status(), 201);

    // List variables
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/variables")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let vars = body["variables"].as_array().unwrap();
    assert_eq!(vars.len(), 2);

    // Update variable
    let resp = ctx
        .put(
            &format!("/api/v1/apps/{app_id}/variables/{var_id}"),
            json!({"value": "10.0.0.2"}),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["value"], "10.0.0.2");

    // Delete variable
    let resp = ctx
        .delete_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/variables/{var_id}"),
        )
        .await;
    assert_eq!(resp.status(), 204);

    // Verify deleted
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/variables")).await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["variables"].as_array().unwrap().len(), 1);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_secret_variable_masking() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Create a secret variable
    ctx.post(
        &format!("/api/v1/apps/{app_id}/variables"),
        json!({"name": "DB_PASSWORD", "value": "s3cret!", "is_secret": true}),
    )
    .await;

    // Grant viewer access
    ctx.grant_permission(app_id, ctx.viewer_user_id, "view")
        .await;

    // Viewer should see masked value
    let resp = ctx
        .get_as("viewer", &format!("/api/v1/apps/{app_id}/variables"))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let var = &body["variables"][0];
    assert_eq!(var["value"], "********");
    assert_eq!(var["is_secret"], true);

    // Admin (edit+) should see real value
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/variables")).await;
    let body: Value = resp.json().await.unwrap();
    let var = &body["variables"][0];
    assert_eq!(var["value"], "s3cret!");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_component_group_crud() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Create groups
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/groups"),
            json!({"name": "Bases de données", "color": "#3B82F6", "display_order": 0}),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let group: Value = resp.json().await.unwrap();
    let group_id = group["id"].as_str().unwrap();
    assert_eq!(group["name"], "Bases de données");
    assert_eq!(group["color"], "#3B82F6");

    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/groups"),
            json!({"name": "Middlewares", "color": "#F59E0B", "display_order": 1}),
        )
        .await;
    assert_eq!(resp.status(), 201);

    // List groups
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/groups")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let groups = body["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);

    // Update group
    let resp = ctx
        .put(
            &format!("/api/v1/apps/{app_id}/groups/{group_id}"),
            json!({"name": "Databases", "color": "#2563EB"}),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["name"], "Databases");

    // Delete group
    let resp = ctx
        .delete_as("admin", &format!("/api/v1/apps/{app_id}/groups/{group_id}"))
        .await;
    assert_eq!(resp.status(), 204);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_component_display_fields() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Create a group
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/groups"),
            json!({"name": "Infrastructure", "color": "#8B5CF6"}),
        )
        .await;
    let group: Value = resp.json().await.unwrap();
    let group_id = group["id"].as_str().unwrap();

    // Create a component with display fields
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/components"),
            json!({
                "name": "azure-vm-01",
                "display_name": "Azure VM Production",
                "description": "Primary production virtual machine",
                "icon": "cloud",
                "group_id": group_id,
                "component_type": "custom",
                "check_cmd": "az vm show --name vm-01",
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let comp: Value = resp.json().await.unwrap();
    assert_eq!(comp["display_name"], "Azure VM Production");
    assert_eq!(comp["description"], "Primary production virtual machine");
    assert_eq!(comp["icon"], "cloud");
    assert_eq!(comp["group_id"], group_id);

    // List components should include display fields
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/components")).await;
    let body: Value = resp.json().await.unwrap();
    let comps = body["components"].as_array().unwrap();
    let azure = comps.iter().find(|c| c["name"] == "azure-vm-01").unwrap();
    assert_eq!(azure["display_name"], "Azure VM Production");
    assert_eq!(azure["icon"], "cloud");
    assert_eq!(azure["group_id"], group_id);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_component_links_crud() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let comp_id = ctx.component_id(app_id, "Oracle-DB").await;

    // Create links
    let resp = ctx.post(
        &format!("/api/v1/components/{comp_id}/links"),
        json!({"label": "Oracle Documentation", "url": "https://docs.oracle.com", "link_type": "documentation"}),
    ).await;
    assert_eq!(resp.status(), 201);
    let link: Value = resp.json().await.unwrap();
    let link_id = link["id"].as_str().unwrap();

    ctx.post(
        &format!("/api/v1/components/{comp_id}/links"),
        json!({"label": "Grafana Dashboard", "url": "https://grafana.local/oracle", "link_type": "monitoring"}),
    ).await;

    ctx.post(
        &format!("/api/v1/components/{comp_id}/links"),
        json!({"label": "CMDB Entry", "url": "https://cmdb.local/ci/oracle-prd", "link_type": "cmdb"}),
    ).await;

    // List links
    let resp = ctx
        .get(&format!("/api/v1/components/{comp_id}/links"))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let links = body["links"].as_array().unwrap();
    assert_eq!(links.len(), 3);

    // Update link
    let resp = ctx
        .put(
            &format!("/api/v1/components/{comp_id}/links/{link_id}"),
            json!({"url": "https://docs.oracle.com/en/database/"}),
        )
        .await;
    assert_eq!(resp.status(), 200);

    // Delete link
    let resp = ctx
        .delete_as(
            "admin",
            &format!("/api/v1/components/{comp_id}/links/{link_id}"),
        )
        .await;
    assert_eq!(resp.status(), 204);

    // Verify deleted
    let resp = ctx
        .get(&format!("/api/v1/components/{comp_id}/links"))
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["links"].as_array().unwrap().len(), 2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_command_input_params_crud() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let comp_id = ctx.component_id(app_id, "Oracle-DB").await;

    // Create a custom command
    sqlx::query(
        "INSERT INTO component_commands (id, component_id, name, command, description, requires_confirmation)
         VALUES ($1, $2, 'purge_logs', 'purge_logs.sh --days=$(days) --env=$(env)', 'Purge old log files', true)"
    )
    .bind(Uuid::new_v4())
    .bind(comp_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let cmd_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM component_commands WHERE component_id = $1 AND name = 'purge_logs'",
    )
    .bind(comp_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    // Create input parameters
    let resp = ctx
        .post(
            &format!("/api/v1/commands/{cmd_id}/params"),
            json!({
                "name": "days",
                "description": "Number of days to retain",
                "default_value": "30",
                "validation_regex": "^\\d+$",
                "required": true,
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);

    let resp = ctx
        .post(
            &format!("/api/v1/commands/{cmd_id}/params"),
            json!({
                "name": "env",
                "description": "Target environment",
                "default_value": "prod",
                "required": false,
            }),
        )
        .await;
    assert_eq!(resp.status(), 201);

    // List params
    let resp = ctx.get(&format!("/api/v1/commands/{cmd_id}/params")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let params = body["params"].as_array().unwrap();
    assert_eq!(params.len(), 2);

    // Verify the days param
    let days = params.iter().find(|p| p["name"] == "days").unwrap();
    assert_eq!(days["default_value"], "30");
    assert_eq!(days["validation_regex"], "^\\d+$");
    assert_eq!(days["required"], true);

    // Delete a param
    let param_id = params[0]["id"].as_str().unwrap();
    let resp = ctx
        .delete_as(
            "admin",
            &format!("/api/v1/commands/{cmd_id}/params/{param_id}"),
        )
        .await;
    assert_eq!(resp.status(), 204);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_invalid_regex_rejected() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;
    let comp_id = ctx.component_id(app_id, "Oracle-DB").await;

    sqlx::query(
        "INSERT INTO component_commands (id, component_id, name, command)
         VALUES ($1, $2, 'test_cmd', 'test.sh')",
    )
    .bind(Uuid::new_v4())
    .bind(comp_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let cmd_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM component_commands WHERE component_id = $1 AND name = 'test_cmd'",
    )
    .bind(comp_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    // Invalid regex should be rejected
    let resp = ctx
        .post(
            &format!("/api/v1/commands/{cmd_id}/params"),
            json!({
                "name": "bad_param",
                "validation_regex": "[invalid(",
                "required": true,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_variable_uniqueness() {
    let ctx = TestContext::new().await;
    let app_id = ctx.create_payments_app().await;

    // Create first variable
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/variables"),
            json!({"name": "UNIQUE_VAR", "value": "first"}),
        )
        .await;
    assert_eq!(resp.status(), 201);

    // Duplicate name should fail
    let resp = ctx
        .post(
            &format!("/api/v1/apps/{app_id}/variables"),
            json!({"name": "UNIQUE_VAR", "value": "second"}),
        )
        .await;
    assert_eq!(resp.status(), 500); // unique constraint violation

    ctx.cleanup().await;
}
