//! SQLite E2E: YAML map import tests.

use super::common::TestContext;
use serde_json::{json, Value};
use uuid::Uuid;

const SAMPLE_YAML: &str = r#"
application:
  name: LYNX-PRD
  description: "Application LYNX Production"
  variables:
    - name: APP_HOST
      value: "10.0.1.100"
      description: "Primary application host"
    - name: APP_PORT
      value: "8443"
    - name: DB_HOST
      value: "10.0.1.50"
  components:
    - name: oracle-db
      displayName: "Oracle Database 19c"
      componentType: database
      group: "Databases"
      icon: database
      dependsOn: []
      actions:
        - name: check
          command: "/opt/oracle/check_db.sh"
        - name: start
          command: "/opt/oracle/start_db.sh"
        - name: stop
          command: "/opt/oracle/stop_db.sh"
    - name: tomcat-app
      displayName: "Tomcat Application Server"
      componentType: appserver
      group: "AppServers"
      dependsOn:
        - oracle-db
      actions:
        - name: check
          command: "curl -s http://localhost:8080/health"
        - name: start
          command: "/opt/tomcat/bin/startup.sh"
        - name: stop
          command: "/opt/tomcat/bin/shutdown.sh"
    - name: apache-front
      displayName: "Apache Web Frontend"
      componentType: webfront
      group: "WebFronts"
      dependsOn:
        - tomcat-app
      actions:
        - name: check
          command: "check_apache.sh"
        - name: start
          command: "start_apache.sh"
        - name: stop
          command: "stop_apache.sh"
"#;

#[tokio::test]
async fn test_import_yaml_map() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD')",
    )
    .bind(site_id.to_string())
    .bind(ctx.organization_id.to_string())
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let resp = ctx
        .post(
            "/api/v1/import/yaml",
            json!({ "yaml": SAMPLE_YAML, "site_id": site_id }),
        )
        .await;
    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    assert_eq!(status, 201, "Import failed: {body_text}");

    let result: Value = serde_json::from_str(&body_text).unwrap();
    let app_id = result["application_id"].as_str().unwrap();

    assert_eq!(result["application_name"], "LYNX-PRD");
    assert_eq!(result["components_created"], 3);
    assert_eq!(result["variables_created"], 3);

    // Verify the application exists
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
    assert_eq!(resp.status(), 200);
    let app: Value = resp.json().await.unwrap();
    assert_eq!(app["name"], "LYNX-PRD");

    // Verify components
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/components")).await;
    let body: Value = resp.json().await.unwrap();
    let comps = body["components"].as_array().unwrap();
    assert_eq!(comps.len(), 3);

    let oracle = comps.iter().find(|c| c["name"] == "oracle-db").unwrap();
    assert_eq!(oracle["display_name"], "Oracle Database 19c");

    // Verify variables
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/variables")).await;
    let body: Value = resp.json().await.unwrap();
    let vars = body["variables"].as_array().unwrap();
    assert_eq!(vars.len(), 3);
    let host_var = vars.iter().find(|v| v["name"] == "APP_HOST").unwrap();
    assert_eq!(host_var["value"], "10.0.1.100");

    // Verify dependencies
    let resp = ctx
        .get(&format!("/api/v1/apps/{app_id}/dependencies"))
        .await;
    let body: Value = resp.json().await.unwrap();
    let deps = body["dependencies"].as_array().unwrap();
    assert!(deps.len() >= 2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_import_invalid_yaml() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD4')",
    )
    .bind(site_id.to_string())
    .bind(ctx.organization_id.to_string())
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let resp = ctx
        .post(
            "/api/v1/import/yaml",
            json!({"yaml": "{{invalid yaml: [", "site_id": site_id}),
        )
        .await;
    assert_eq!(resp.status(), 400);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_import_missing_dependency_warns() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD5')",
    )
    .bind(site_id.to_string())
    .bind(ctx.organization_id.to_string())
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let yaml = r#"
application:
  name: Missing-Dep-App
  components:
    - name: web
      dependsOn:
        - nonexistent-component
      actions:
        - name: check
          command: "check.sh"
"#;

    let resp = ctx
        .post(
            "/api/v1/import/yaml",
            json!({"yaml": yaml, "site_id": site_id}),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let result: Value = resp.json().await.unwrap();

    assert_eq!(result["components_created"], 1);
    assert_eq!(result["dependencies_created"], 0);
    let warnings = result["warnings"].as_array().unwrap();
    assert!(!warnings.is_empty());
    assert!(warnings[0]
        .as_str()
        .unwrap()
        .contains("nonexistent-component"));

    ctx.cleanup().await;
}
