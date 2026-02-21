//! E2E tests for YAML map import (old AppControl format → v4 model).

use super::*;

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
      description: "Application HTTPS port"
    - name: DB_HOST
      value: "10.0.1.50"
  components:
    - name: oracle-db
      displayName: "Oracle Database 19c"
      description: "Primary Oracle database instance"
      componentType: database
      group: "Bases de données"
      icon: database
      dependsOn: []
      actions:
        - name: check
          command: "/opt/oracle/check_db.sh"
        - name: start
          command: "/opt/oracle/start_db.sh"
        - name: stop
          command: "/opt/oracle/stop_db.sh"
        - name: purge_logs
          displayName: "Purge Logs"
          command: "purge_oracle_logs.sh --days=$(days)"
          description: "Remove old Oracle logs"
          requiresConfirmation: true
          parameters:
            - name: days
              description: "Number of days to retain"
              defaultValue: "30"
              validationRegex: "^\\d+$"
              required: true
      hypertextLinks:
        - label: "Oracle Documentation"
          url: "https://docs.oracle.com"
          type: documentation
        - label: "Grafana Dashboard"
          url: "https://grafana.local/oracle"
          type: monitoring

    - name: tomcat-app
      displayName: "Tomcat Application Server"
      componentType: appserver
      group: "Serveurs applicatifs"
      dependsOn:
        - oracle-db
      actions:
        - name: check
          command: "curl -s http://$(APP_HOST):$(APP_PORT)/health"
        - name: start
          command: "/opt/tomcat/bin/startup.sh"
        - name: stop
          command: "/opt/tomcat/bin/shutdown.sh"
        - name: deploy
          displayName: "Deploy WAR"
          command: "deploy.sh --env=$(env) --version=$(version)"
          requiresConfirmation: true
          parameters:
            - name: env
              description: "Target environment"
              defaultValue: "prod"
            - name: version
              description: "WAR version to deploy"
              validationRegex: "^\\d+\\.\\d+\\.\\d+$"
              required: true

    - name: apache-front
      displayName: "Apache Web Frontend"
      componentType: webfront
      group: "Fronts web"
      dependsOn:
        - tomcat-app
      actions:
        - name: check
          command: "check_apache.sh"
        - name: start
          command: "start_apache.sh"
        - name: stop
          command: "stop_apache.sh"
      hypertextLinks:
        - label: "Apache Config"
          url: "https://wiki.local/apache-config"
          type: runbook

    - name: rabbitmq
      displayName: "RabbitMQ Message Broker"
      componentType: middleware
      group: "Middlewares"
      dependsOn:
        - oracle-db
      actions:
        - name: check
          command: "rabbitmqctl status"
        - name: start
          command: "rabbitmq-server -detached"
        - name: stop
          command: "rabbitmqctl stop"

    - name: batch-processor
      displayName: "Batch Processor"
      componentType: batch
      group: "Traitements batch"
      dependsOn:
        - rabbitmq
      actions:
        - name: check
          command: "check_batch.sh"
        - name: start
          command: "start_batch.sh"
        - name: stop
          command: "stop_batch.sh"
"#;

#[tokio::test]
async fn test_import_yaml_map() {
    let ctx = TestContext::new().await;

    // Create a site first
    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD')",
    )
    .bind(site_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Import the YAML map
    let resp = ctx
        .post(
            "/api/v1/import/yaml",
            json!({
                "yaml": SAMPLE_YAML,
                "site_id": site_id,
            }),
        )
        .await;
    assert_eq!(resp.status(), 201, "Import failed: {:?}", resp.text().await);

    let result: Value = resp.json().await.unwrap();
    let app_id = result["application_id"].as_str().unwrap();

    // Verify counts
    assert_eq!(result["application_name"], "LYNX-PRD");
    assert_eq!(result["components_created"], 5);
    assert_eq!(result["groups_created"], 4); // Bases de données, Serveurs applicatifs, Fronts web, Middlewares, Traitements batch
    assert_eq!(result["variables_created"], 3);
    assert!(result["dependencies_created"].as_i64().unwrap() >= 4);
    assert!(result["commands_created"].as_i64().unwrap() >= 2); // purge_logs + deploy (standard actions excluded)
    assert!(result["links_created"].as_i64().unwrap() >= 3);

    // Verify the application exists
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}")).await;
    assert_eq!(resp.status(), 200);
    let app: Value = resp.json().await.unwrap();
    assert_eq!(app["name"], "LYNX-PRD");

    // Verify components
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/components")).await;
    let body: Value = resp.json().await.unwrap();
    let comps = body["components"].as_array().unwrap();
    assert_eq!(comps.len(), 5);

    // Verify oracle-db has display_name, icon, commands
    let oracle = comps.iter().find(|c| c["name"] == "oracle-db").unwrap();
    assert_eq!(oracle["display_name"], "Oracle Database 19c");
    assert_eq!(oracle["icon"], "database");
    assert!(oracle["check_cmd"]
        .as_str()
        .unwrap()
        .contains("check_db.sh"));
    assert!(oracle["start_cmd"]
        .as_str()
        .unwrap()
        .contains("start_db.sh"));

    // Verify variables
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/variables")).await;
    let body: Value = resp.json().await.unwrap();
    let vars = body["variables"].as_array().unwrap();
    assert_eq!(vars.len(), 3);
    let host_var = vars.iter().find(|v| v["name"] == "APP_HOST").unwrap();
    assert_eq!(host_var["value"], "10.0.1.100");

    // Verify groups
    let resp = ctx.get(&format!("/api/v1/apps/{app_id}/groups")).await;
    let body: Value = resp.json().await.unwrap();
    let groups = body["groups"].as_array().unwrap();
    assert!(groups.len() >= 4);
    assert!(groups.iter().any(|g| g["name"] == "Bases de données"));
    assert!(groups.iter().any(|g| g["name"] == "Middlewares"));

    // Verify dependencies
    let resp = ctx
        .get(&format!("/api/v1/apps/{app_id}/dependencies"))
        .await;
    let body: Value = resp.json().await.unwrap();
    let deps = body["dependencies"].as_array().unwrap();
    assert!(deps.len() >= 4);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_import_creates_links() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD2')",
    )
    .bind(site_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let resp = ctx
        .post(
            "/api/v1/import/yaml",
            json!({"yaml": SAMPLE_YAML, "site_id": site_id}),
        )
        .await;
    let result: Value = resp.json().await.unwrap();
    let app_id_str = result["application_id"].as_str().unwrap();
    let app_id: Uuid = app_id_str.parse().unwrap();

    // Get oracle-db component and check its links
    let oracle_id = ctx.component_id(app_id, "oracle-db").await;
    let resp = ctx
        .get(&format!("/api/v1/components/{oracle_id}/links"))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let links = body["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(links
        .iter()
        .any(|l| l["label"] == "Oracle Documentation" && l["link_type"] == "documentation"));
    assert!(links
        .iter()
        .any(|l| l["label"] == "Grafana Dashboard" && l["link_type"] == "monitoring"));

    // Check apache links
    let apache_id = ctx.component_id(app_id, "apache-front").await;
    let resp = ctx
        .get(&format!("/api/v1/components/{apache_id}/links"))
        .await;
    let body: Value = resp.json().await.unwrap();
    let links = body["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["link_type"], "runbook");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_import_creates_command_params() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD3')",
    )
    .bind(site_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let resp = ctx
        .post(
            "/api/v1/import/yaml",
            json!({"yaml": SAMPLE_YAML, "site_id": site_id}),
        )
        .await;
    let result: Value = resp.json().await.unwrap();
    let app_id_str = result["application_id"].as_str().unwrap();
    let app_id: Uuid = app_id_str.parse().unwrap();

    // Find the purge_logs command for oracle-db
    let oracle_id = ctx.component_id(app_id, "oracle-db").await;
    let cmd_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM component_commands WHERE component_id = $1 AND name = 'Purge Logs'",
    )
    .bind(oracle_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    // Check input params
    let resp = ctx.get(&format!("/api/v1/commands/{cmd_id}/params")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let params = body["params"].as_array().unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0]["name"], "days");
    assert_eq!(params[0]["default_value"], "30");
    assert_eq!(params[0]["validation_regex"], r"^\d+$");

    // Find the deploy command for tomcat-app
    let tomcat_id = ctx.component_id(app_id, "tomcat-app").await;
    let deploy_cmd_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM component_commands WHERE component_id = $1 AND name = 'Deploy WAR'",
    )
    .bind(tomcat_id)
    .fetch_one(&ctx.db_pool)
    .await
    .unwrap();

    let resp = ctx
        .get(&format!("/api/v1/commands/{deploy_cmd_id}/params"))
        .await;
    let body: Value = resp.json().await.unwrap();
    let params = body["params"].as_array().unwrap();
    assert_eq!(params.len(), 2);
    assert!(params
        .iter()
        .any(|p| p["name"] == "env" && p["default_value"] == "prod"));
    assert!(params
        .iter()
        .any(|p| p["name"] == "version" && p["validation_regex"] == r"^\d+\.\d+\.\d+$"));

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_import_invalid_yaml() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD4')",
    )
    .bind(site_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    // Invalid YAML should return 400
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
    .bind(site_id)
    .bind(ctx.organization_id)
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

    // Should have 1 component created and a warning about missing dep
    assert_eq!(result["components_created"], 1);
    assert_eq!(result["dependencies_created"], 0);
    let warnings = result["warnings"].as_array().unwrap();
    assert!(warnings.len() > 0);
    assert!(warnings[0]
        .as_str()
        .unwrap()
        .contains("nonexistent-component"));

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_import_audit_trail() {
    let ctx = TestContext::new().await;

    let site_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD6')",
    )
    .bind(site_id)
    .bind(ctx.organization_id)
    .execute(&ctx.db_pool)
    .await
    .unwrap();

    let yaml = r#"
application:
  name: Audit-Import-App
  components:
    - name: db
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
    let result: Value = resp.json().await.unwrap();
    let app_id_str = result["application_id"].as_str().unwrap();
    let app_id: Uuid = app_id_str.parse().unwrap();

    // Verify import action was logged
    let logs = ctx.get_action_log(app_id, "import_yaml").await;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].action, "import_yaml");
    assert_eq!(logs[0].details["name"], "Audit-Import-App");

    ctx.cleanup().await;
}
