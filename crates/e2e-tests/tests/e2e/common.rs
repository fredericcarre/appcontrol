// Shared E2E test infrastructure.
//
// TestContext sets up a temporary PostgreSQL database, starts the backend
// in-process on a random port, seeds users, and provides HTTP helpers.
// Each test gets a fully isolated database that is dropped on cleanup.

#![allow(dead_code)]

use reqwest::{Client, Response};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use uuid::Uuid;

/// Run SQL migration files from the migrations/ directory at runtime.
/// Supports our V001__name.sql naming convention by sorting on the V-prefix number.
async fn run_migrations(pool: &PgPool) {
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let migrations_dir = migrations_dir
        .canonicalize()
        .expect("Cannot find migrations directory");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("Cannot read migrations directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let sql = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Cannot read migration: {}", path.display()));
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("Migration {} failed: {e}", path.display()));
    }
}

// ---------------------------------------------------------------------------
// TestContext
// ---------------------------------------------------------------------------

pub struct TestContext {
    pub db_pool: PgPool,
    pub api_url: String,
    pub ws_url: String,
    pub admin_user_id: Uuid,
    pub operator_user_id: Uuid,
    pub viewer_user_id: Uuid,
    pub editor_user_id: Uuid,
    pub organization_id: Uuid,
    pub default_site_id: Uuid,
    pub db_name: String,
    client: Client,
    pub admin_token: String,
    pub operator_token: String,
    pub viewer_token: String,
    pub editor_token: String,
    _server_handle: tokio::task::JoinHandle<()>,
}

impl TestContext {
    /// Create a fresh test environment:
    /// 1. Create a temporary PostgreSQL database
    /// 2. Run migrations
    /// 3. Seed organization + 4 users (admin, operator, viewer, editor)
    /// 4. Start backend on a random port
    pub async fn new() -> Self {
        let db_name = format!("test_{}", Uuid::new_v4().simple());
        let admin_url = std::env::var("TEST_DATABASE_ADMIN_URL")
            .unwrap_or_else(|_| "postgres://appcontrol:test@localhost:5432/postgres".to_string());

        // Create temp DB
        let admin_pool = PgPool::connect(&admin_url)
            .await
            .expect("Cannot connect to PostgreSQL. Is it running?");
        sqlx::query(&format!("CREATE DATABASE {db_name}"))
            .execute(&admin_pool)
            .await
            .expect("Failed to create temp database");

        let db_url = format!("postgres://appcontrol:test@localhost:5432/{db_name}");
        let pool = PgPool::connect(&db_url).await.unwrap();

        // Run migrations
        run_migrations(&pool).await;

        // Seed organization and users
        let org_id = Uuid::new_v4();
        let admin_id = Uuid::new_v4();
        let operator_id = Uuid::new_v4();
        let viewer_id = Uuid::new_v4();
        let editor_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Test Org', 'test-org')")
            .bind(org_id)
            .execute(&pool)
            .await
            .unwrap();

        for (id, name, role) in [
            (admin_id, "admin", "admin"),
            (operator_id, "operator", "operator"),
            (viewer_id, "viewer", "viewer"),
            (editor_id, "editor", "editor"),
        ] {
            sqlx::query(
                "INSERT INTO users (id, organization_id, external_id, display_name, role, email)
                 VALUES ($1, $2, $3, $3, $4, $3 || '@test.local')",
            )
            .bind(id)
            .bind(org_id)
            .bind(name)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Create default site
        let default_site_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Default', 'DEF')",
        )
        .bind(default_site_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        // Start backend on random port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let api_url = format!("http://{addr}");
        let ws_url = format!("ws://{addr}");

        let config = appcontrol_backend::config::AppConfig {
            database_url: db_url,
            redis_url: std::env::var("TEST_REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            port: addr.port(),
            jwt_secret: "test-jwt-secret".to_string(),
            jwt_issuer: "appcontrol-test".to_string(),
            oidc: None,
            saml: None,
        };

        let state = Arc::new(appcontrol_backend::AppState {
            db: pool.clone(),
            ws_hub: appcontrol_backend::websocket::Hub::new(),
            config,
        });

        let app = appcontrol_backend::create_router(state);
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Generate JWT tokens for each user
        let admin_token = Self::make_jwt(admin_id, org_id, "admin", "test-jwt-secret");
        let operator_token = Self::make_jwt(operator_id, org_id, "operator", "test-jwt-secret");
        let viewer_token = Self::make_jwt(viewer_id, org_id, "viewer", "test-jwt-secret");
        let editor_token = Self::make_jwt(editor_id, org_id, "editor", "test-jwt-secret");

        Self {
            db_pool: pool,
            api_url,
            ws_url,
            admin_user_id: admin_id,
            operator_user_id: operator_id,
            viewer_user_id: viewer_id,
            editor_user_id: editor_id,
            organization_id: org_id,
            default_site_id,
            db_name,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            admin_token,
            operator_token,
            viewer_token,
            editor_token,
            _server_handle: server_handle,
        }
    }

    /// Create a TestContext with SAML enabled.
    pub async fn new_with_saml(idp_sso_url: &str, sp_entity_id: &str) -> Self {
        Self::new_with_saml_inner(idp_sso_url, sp_entity_id, None).await
    }

    /// Create a TestContext with SAML enabled and an admin group.
    pub async fn new_with_saml_admin(
        idp_sso_url: &str,
        sp_entity_id: &str,
        admin_group: &str,
    ) -> Self {
        Self::new_with_saml_inner(idp_sso_url, sp_entity_id, Some(admin_group.to_string())).await
    }

    async fn new_with_saml_inner(
        idp_sso_url: &str,
        sp_entity_id: &str,
        admin_group: Option<String>,
    ) -> Self {
        let db_name = format!("test_{}", Uuid::new_v4().simple());
        let admin_url = std::env::var("TEST_DATABASE_ADMIN_URL")
            .unwrap_or_else(|_| "postgres://appcontrol:test@localhost:5432/postgres".to_string());

        let admin_pool = PgPool::connect(&admin_url)
            .await
            .expect("Cannot connect to PostgreSQL. Is it running?");
        sqlx::query(&format!("CREATE DATABASE {db_name}"))
            .execute(&admin_pool)
            .await
            .expect("Failed to create temp database");

        let db_url = format!("postgres://appcontrol:test@localhost:5432/{db_name}");
        let pool = PgPool::connect(&db_url).await.unwrap();

        run_migrations(&pool).await;

        let org_id = Uuid::new_v4();
        let admin_id = Uuid::new_v4();
        let operator_id = Uuid::new_v4();
        let viewer_id = Uuid::new_v4();
        let editor_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO organizations (id, name, slug) VALUES ($1, 'Test Org', 'test-org')",
        )
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        for (id, name, role) in [
            (admin_id, "admin", "admin"),
            (operator_id, "operator", "operator"),
            (viewer_id, "viewer", "viewer"),
            (editor_id, "editor", "editor"),
        ] {
            sqlx::query(
                "INSERT INTO users (id, organization_id, external_id, display_name, role, email)
                 VALUES ($1, $2, $3, $3, $4, $3 || '@test.local')",
            )
            .bind(id)
            .bind(org_id)
            .bind(name)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Create default site
        let default_site_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Default', 'DEF')",
        )
        .bind(default_site_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let api_url = format!("http://{addr}");
        let ws_url = format!("ws://{addr}");

        let config = appcontrol_backend::config::AppConfig {
            database_url: db_url,
            redis_url: std::env::var("TEST_REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            port: addr.port(),
            jwt_secret: "test-jwt-secret".to_string(),
            jwt_issuer: "appcontrol-test".to_string(),
            oidc: None,
            saml: Some(appcontrol_backend::auth::saml::SamlConfig {
                idp_sso_url: idp_sso_url.to_string(),
                idp_cert: "test-cert".to_string(),
                sp_entity_id: sp_entity_id.to_string(),
                sp_acs_url: format!("{api_url}/api/v1/auth/saml/acs"),
                group_attribute: "memberOf".to_string(),
                email_attribute: "email".to_string(),
                name_attribute: "displayName".to_string(),
                admin_group,
                want_assertions_signed: false,
            }),
        };

        let state = Arc::new(appcontrol_backend::AppState {
            db: pool.clone(),
            ws_hub: appcontrol_backend::websocket::Hub::new(),
            config,
        });

        let app = appcontrol_backend::create_router(state);
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let admin_token = Self::make_jwt(admin_id, org_id, "admin", "test-jwt-secret");
        let operator_token = Self::make_jwt(operator_id, org_id, "operator", "test-jwt-secret");
        let viewer_token = Self::make_jwt(viewer_id, org_id, "viewer", "test-jwt-secret");
        let editor_token = Self::make_jwt(editor_id, org_id, "editor", "test-jwt-secret");

        Self {
            db_pool: pool,
            api_url,
            ws_url,
            admin_user_id: admin_id,
            operator_user_id: operator_id,
            viewer_user_id: viewer_id,
            editor_user_id: editor_id,
            organization_id: org_id,
            default_site_id,
            db_name,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            admin_token,
            operator_token,
            viewer_token,
            editor_token,
            _server_handle: server_handle,
        }
    }

    /// Returns a reqwest Client that does NOT follow redirects.
    pub fn client_no_redirect(&self) -> Client {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    /// POST a form (application/x-www-form-urlencoded) without authentication.
    pub async fn post_form_anonymous(&self, path: &str, params: &[(&str, &str)]) -> Response {
        self.client
            .post(format!("{}{path}", self.api_url))
            .form(params)
            .send()
            .await
            .unwrap()
    }

    fn make_jwt(user_id: Uuid, org_id: Uuid, role: &str, secret: &str) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "sub": user_id.to_string(),
            "org": org_id.to_string(),
            "email": format!("{role}@test.local"),
            "role": role,
            "exp": now + 3600,
            "iat": now,
            "iss": "appcontrol-test",
        });
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    fn token_for(&self, user: &str) -> &str {
        match user {
            "admin" => &self.admin_token,
            "operator" => &self.operator_token,
            "viewer" => &self.viewer_token,
            "editor" => &self.editor_token,
            _ => panic!("Unknown test user: {user}"),
        }
    }

    // ---- HTTP helpers ----

    pub async fn post(&self, path: &str, body: Value) -> Response {
        self.post_as("admin", path, body).await
    }

    pub async fn post_as(&self, user: &str, path: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.api_url))
            .bearer_auth(self.token_for(user))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get(&self, path: &str) -> Response {
        self.get_as("admin", path).await
    }

    pub async fn get_as(&self, user: &str, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.api_url))
            .bearer_auth(self.token_for(user))
            .send()
            .await
            .unwrap()
    }

    pub async fn put(&self, path: &str, body: Value) -> Response {
        self.put_as("admin", path, body).await
    }

    pub async fn put_as(&self, user: &str, path: &str, body: Value) -> Response {
        self.client
            .put(format!("{}{path}", self.api_url))
            .bearer_auth(self.token_for(user))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn delete_as(&self, user: &str, path: &str) -> Response {
        self.client
            .delete(format!("{}{path}", self.api_url))
            .bearer_auth(self.token_for(user))
            .send()
            .await
            .unwrap()
    }

    pub async fn get_anonymous(&self, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.api_url))
            .send()
            .await
            .unwrap()
    }

    pub async fn get_with_api_key(&self, key: &str, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.api_url))
            .header("Authorization", format!("ApiKey {key}"))
            .send()
            .await
            .unwrap()
    }

    pub async fn post_with_api_key(&self, key: &str, path: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.api_url))
            .header("Authorization", format!("ApiKey {key}"))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_with_api_key_timeout(
        &self,
        key: &str,
        path: &str,
        timeout: Duration,
    ) -> Response {
        self.client
            .get(format!("{}{path}", self.api_url))
            .header("Authorization", format!("ApiKey {key}"))
            .timeout(timeout)
            .send()
            .await
            .unwrap()
    }

    /// POST with a custom Authorization header (for share link tokens, etc.)
    pub async fn post_with_token(&self, token: &str, path: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.api_url))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_with_token(&self, token: &str, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.api_url))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }

    // ---- App factory helpers ----

    /// Creates a 5-component "Payments-SEPA" application:
    ///   Oracle-DB → Tomcat-App → Apache-Front
    ///   Oracle-DB → RabbitMQ  → Batch-Processor
    pub async fn create_payments_app(&self) -> Uuid {
        let resp = self
            .post(
                "/api/v1/apps",
                json!({
                    "name": "Paiements-SEPA",
                    "description": "SEPA payment processing",
                    "tags": ["payments", "critical"],
                    "site_id": self.default_site_id,
                }),
            )
            .await;
        assert!(
            resp.status().is_success(),
            "Failed to create app: {}",
            resp.status()
        );
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        let components = vec![
            (
                "Oracle-DB",
                "database",
                "check_oracle.sh",
                "start_oracle.sh",
                "stop_oracle.sh",
            ),
            (
                "Tomcat-App",
                "appserver",
                "check_tomcat.sh",
                "start_tomcat.sh",
                "stop_tomcat.sh",
            ),
            (
                "RabbitMQ",
                "middleware",
                "check_rabbitmq.sh",
                "start_rabbitmq.sh",
                "stop_rabbitmq.sh",
            ),
            (
                "Apache-Front",
                "webfront",
                "check_apache.sh",
                "start_apache.sh",
                "stop_apache.sh",
            ),
            (
                "Batch-Processor",
                "batch",
                "check_batch.sh",
                "start_batch.sh",
                "stop_batch.sh",
            ),
        ];

        let mut ids: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
        for (name, comp_type, check, start, stop) in &components {
            let resp = self
                .post(
                    &format!("/api/v1/apps/{app_id}/components"),
                    json!({
                        "name": name,
                        "component_type": comp_type,
                        "hostname": format!("srv-{}", name.to_lowercase().replace('-', "")),
                        "check_cmd": check,
                        "start_cmd": start,
                        "stop_cmd": stop,
                        "check_interval_seconds": 30,
                        "start_timeout_seconds": 120,
                        "stop_timeout_seconds": 60,
                    }),
                )
                .await;
            let c: Value = resp.json().await.unwrap();
            ids.insert(name.to_string(), c["id"].as_str().unwrap().parse().unwrap());
        }

        // Dependencies: Oracle-DB → Tomcat-App, Oracle-DB → RabbitMQ,
        // Tomcat-App → Apache-Front, RabbitMQ → Batch-Processor
        let deps = [
            ("Oracle-DB", "Tomcat-App"),
            ("Oracle-DB", "RabbitMQ"),
            ("Tomcat-App", "Apache-Front"),
            ("RabbitMQ", "Batch-Processor"),
        ];
        for (from, to) in &deps {
            self.post(
                &format!("/api/v1/apps/{app_id}/dependencies"),
                json!({
                    "from_component_id": ids[*from],
                    "to_component_id": ids[*to],
                }),
            )
            .await;
        }

        app_id
    }

    /// Creates a 10-component application with two independent branches:
    ///   DB-1 → App-1 → Front-1 / App-1 → Queue-1 → Worker-1
    ///   DB-2 → App-2 → Front-2 / App-2 → Queue-2 → Worker-2
    pub async fn create_ten_component_app(&self) -> Uuid {
        let resp = self
            .post(
                "/api/v1/apps",
                json!({
                    "name": "Multi-Branch-App",
                    "description": "10-component app with two branches",
                    "site_id": self.default_site_id,
                }),
            )
            .await;
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        let names = [
            "DB-1", "App-1", "Front-1", "Queue-1", "Worker-1", "DB-2", "App-2", "Front-2",
            "Queue-2", "Worker-2",
        ];
        let mut ids: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
        for name in &names {
            let resp = self
                .post(
                    &format!("/api/v1/apps/{app_id}/components"),
                    json!({
                        "name": name,
                        "component_type": "service",
                        "hostname": format!("srv-{}", name.to_lowercase()),
                        "check_cmd": format!("check_{}.sh", name.to_lowercase()),
                        "start_cmd": format!("start_{}.sh", name.to_lowercase()),
                        "stop_cmd": format!("stop_{}.sh", name.to_lowercase()),
                    }),
                )
                .await;
            let c: Value = resp.json().await.unwrap();
            ids.insert(name.to_string(), c["id"].as_str().unwrap().parse().unwrap());
        }

        let deps = [
            ("DB-1", "App-1"),
            ("App-1", "Front-1"),
            ("App-1", "Queue-1"),
            ("Queue-1", "Worker-1"),
            ("DB-2", "App-2"),
            ("App-2", "Front-2"),
            ("App-2", "Queue-2"),
            ("Queue-2", "Worker-2"),
        ];
        for (from, to) in &deps {
            self.post(
                &format!("/api/v1/apps/{app_id}/dependencies"),
                json!({
                    "from_component_id": ids[*from],
                    "to_component_id": ids[*to],
                }),
            )
            .await;
        }

        app_id
    }

    /// Creates an app with two DR sites (site_a = PRD, site_b = DR).
    pub async fn create_app_with_dr_sites(&self) -> (Uuid, Uuid, Uuid) {
        let site_a = Uuid::new_v4();
        let site_b = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'PRD', 'PRD')",
        )
        .bind(site_a)
        .bind(self.organization_id)
        .execute(&self.db_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'DR', 'DR')",
        )
        .bind(site_b)
        .bind(self.organization_id)
        .execute(&self.db_pool)
        .await
        .unwrap();

        let resp = self
            .post(
                "/api/v1/apps",
                json!({
                    "name": "DR-App",
                    "description": "Multi-site DR application",
                    "site_id": site_a,
                }),
            )
            .await;
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        for (site_id, suffix) in [(site_a, "prd"), (site_b, "dr")] {
            for name in ["Oracle-DB", "Tomcat-App", "Apache-Front"] {
                self.post(
                    &format!("/api/v1/apps/{app_id}/components"),
                    json!({
                        "name": format!("{name}-{suffix}"),
                        "component_type": "service",
                        "hostname": format!("srv-{}-{suffix}", name.to_lowercase()),
                        "site_id": site_id,
                        "check_cmd": "check.sh",
                        "start_cmd": "start.sh",
                        "stop_cmd": "stop.sh",
                    }),
                )
                .await;
            }
        }

        (app_id, site_a, site_b)
    }

    /// Creates a payment app with 3-level diagnostic check commands configured.
    pub async fn create_payments_app_with_checks(&self) -> Uuid {
        let resp = self
            .post(
                "/api/v1/apps",
                json!({
                    "name": "Diag-App",
                    "description": "App with diagnostic checks",
                    "site_id": self.default_site_id,
                }),
            )
            .await;
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        for (name, comp_type) in [
            ("Redis", "middleware"),
            ("Tomcat", "appserver"),
            ("Oracle", "database"),
            ("Apache", "webfront"),
        ] {
            self.post(
                &format!("/api/v1/apps/{app_id}/components"),
                json!({
                    "name": name,
                    "component_type": comp_type,
                    "hostname": format!("srv-{}", name.to_lowercase()),
                    "check_cmd": format!("check_{}.sh", name.to_lowercase()),
                    "start_cmd": format!("start_{}.sh", name.to_lowercase()),
                    "stop_cmd": format!("stop_{}.sh", name.to_lowercase()),
                    "integrity_check_cmd": format!("integrity_{}.sh", name.to_lowercase()),
                    "infra_check_cmd": format!("infra_{}.sh", name.to_lowercase()),
                    "rebuild_cmd": format!("rebuild_{}.sh", name.to_lowercase()),
                    "rebuild_infra_cmd": format!("rebuild_infra_{}.sh", name.to_lowercase()),
                }),
            )
            .await;
        }

        // Oracle → Tomcat dependency
        let oracle_id = self.component_id(app_id, "Oracle").await;
        let tomcat_id = self.component_id(app_id, "Tomcat").await;
        self.post(
            &format!("/api/v1/apps/{app_id}/dependencies"),
            json!({
                "from_component_id": oracle_id,
                "to_component_id": tomcat_id,
            }),
        )
        .await;

        app_id
    }

    // ---- State helpers ----

    pub async fn set_all_running(&self, app_id: Uuid) {
        let comp_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM components WHERE application_id = $1",
        )
        .bind(app_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap();
        for cid in comp_ids {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
                 VALUES ($1, 'UNKNOWN', 'RUNNING', 'test_setup')",
            )
            .bind(cid)
            .execute(&self.db_pool)
            .await
            .unwrap();
        }
    }

    pub async fn set_all_running_on_site(&self, _app_id: Uuid, _site_id: Uuid) {
        // Site overrides are per-component, not a column on components.
        // For DR tests, use set_all_running or force_component_state per component.
    }

    pub async fn get_app_status(&self, app_id: Uuid) -> AppStatus {
        let resp = self
            .get(&format!("/api/v1/orchestration/apps/{app_id}/status"))
            .await;
        resp.json().await.unwrap()
    }

    pub async fn get_component_state(&self, app_id: Uuid, name: &str) -> String {
        let status = self.get_app_status(app_id).await;
        self.component_state(&status, name).to_string()
    }

    pub async fn force_component_state(&self, app_id: Uuid, name: &str, state: &str) {
        let comp_id = self.component_id(app_id, name).await;
        sqlx::query(
            "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
             VALUES ($1, 'UNKNOWN', $2, 'test_setup')",
        )
        .bind(comp_id)
        .bind(state)
        .execute(&self.db_pool)
        .await
        .unwrap();
    }

    pub async fn component_id(&self, app_id: Uuid, name: &str) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM components WHERE application_id = $1 AND name = $2",
        )
        .bind(app_id)
        .bind(name)
        .fetch_one(&self.db_pool)
        .await
        .unwrap()
    }

    pub fn component_state<'a>(&self, status: &'a AppStatus, name: &str) -> &'a str {
        status
            .components
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.state.as_str())
            .unwrap_or("NOT_FOUND")
    }

    pub async fn get_app(&self, app_id: Uuid) -> App {
        let resp = self.get(&format!("/api/v1/apps/{app_id}")).await;
        resp.json().await.unwrap()
    }

    // ---- Wait helpers ----

    pub async fn wait_app_running(&self, app_id: Uuid, timeout: Duration) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let status = self.get_app_status(app_id).await;
            if status.components.iter().all(|c| c.state == "RUNNING") {
                return Ok(());
            }
            if status.components.iter().any(|c| c.state == "FAILED") {
                anyhow::bail!(
                    "Component failed: {:?}",
                    status
                        .components
                        .iter()
                        .find(|c| c.state == "FAILED")
                        .unwrap()
                        .name
                );
            }
            if tokio::time::Instant::now() > deadline {
                anyhow::bail!("Timeout waiting for app to be RUNNING");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn wait_app_stopped(&self, app_id: Uuid, timeout: Duration) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let status = self.get_app_status(app_id).await;
            if status.components.iter().all(|c| c.state == "STOPPED") {
                return Ok(());
            }
            if tokio::time::Instant::now() > deadline {
                anyhow::bail!("Timeout waiting for app to be STOPPED");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn wait_app_branch_running(
        &self,
        app_id: Uuid,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let status = self.get_app_status(app_id).await;
            if !status
                .components
                .iter()
                .any(|c| c.state == "FAILED" || c.state == "STARTING")
            {
                return Ok(());
            }
            if tokio::time::Instant::now() > deadline {
                anyhow::bail!("Timeout waiting for branch to be running");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // ---- Data query helpers ----

    pub async fn get_state_transitions(&self, app_id: Uuid) -> Vec<StateTransition> {
        sqlx::query_as::<_, StateTransition>(
            "SELECT st.component_id, c.name AS component_name,
                    st.from_state, st.to_state, st.trigger, st.created_at
             FROM state_transitions st
             JOIN components c ON c.id = st.component_id
             WHERE c.application_id = $1
             ORDER BY st.created_at",
        )
        .bind(app_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    pub async fn get_state_transitions_for(
        &self,
        app_id: Uuid,
        name: &str,
    ) -> Vec<StateTransition> {
        sqlx::query_as::<_, StateTransition>(
            "SELECT st.component_id, c.name AS component_name,
                    st.from_state, st.to_state, st.trigger, st.created_at
             FROM state_transitions st
             JOIN components c ON c.id = st.component_id
             WHERE c.application_id = $1 AND c.name = $2
             ORDER BY st.created_at",
        )
        .bind(app_id)
        .bind(name)
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    pub async fn get_action_log(&self, app_id: Uuid, action: &str) -> Vec<ActionLog> {
        self.get_action_log_for_type(app_id, action).await
    }

    pub async fn get_action_log_for_type(&self, app_id: Uuid, action: &str) -> Vec<ActionLog> {
        sqlx::query_as::<_, ActionLog>(
            "SELECT user_id, action, resource_type, resource_id, details, created_at
             FROM action_log WHERE resource_id = $1 AND action = $2
             ORDER BY created_at",
        )
        .bind(app_id)
        .bind(action)
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    pub async fn get_all_action_logs(&self) -> Vec<ActionLog> {
        sqlx::query_as::<_, ActionLog>(
            "SELECT user_id, action, resource_type, resource_id, details, created_at
             FROM action_log ORDER BY created_at",
        )
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    pub async fn count_action_logs(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM action_log")
            .fetch_one(&self.db_pool)
            .await
            .unwrap()
    }

    pub async fn get_config_versions(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Vec<ConfigVersion> {
        sqlx::query_as::<_, ConfigVersion>(
            "SELECT changed_by, before_snapshot, after_snapshot
             FROM config_versions WHERE resource_type = $1 AND resource_id = $2
             ORDER BY created_at",
        )
        .bind(resource_type)
        .bind(resource_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    pub async fn get_switchover_log_entries(&self, switchover_id: Uuid) -> Vec<SwitchoverLogEntry> {
        sqlx::query_as::<_, SwitchoverLogEntry>(
            "SELECT switchover_id, phase, status, details, created_at
             FROM switchover_log WHERE switchover_id = $1
             ORDER BY created_at",
        )
        .bind(switchover_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    pub async fn get_job_status(&self, job_id: Uuid) -> JobStatus {
        let resp = self.get(&format!("/api/v1/jobs/{job_id}")).await;
        resp.json().await.unwrap()
    }

    pub async fn get_check_events(&self, component_id: Uuid) -> Vec<CheckEvent> {
        sqlx::query_as::<_, CheckEvent>(
            "SELECT component_id, check_type, exit_code, stdout, duration_ms, created_at
             FROM check_events WHERE component_id = $1
             ORDER BY created_at",
        )
        .bind(component_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap()
    }

    // ---- Configuration helpers ----

    pub async fn set_component_check_will_fail(&self, app_id: Uuid, name: &str) {
        sqlx::query(
            "UPDATE components SET check_cmd = 'exit 2' WHERE application_id = $1 AND name = $2",
        )
        .bind(app_id)
        .bind(name)
        .execute(&self.db_pool)
        .await
        .unwrap();
    }

    pub async fn configure_check_results(&self, app_id: Uuid, configs: Vec<(&str, i32, i32, i32)>) {
        for (name, health, integrity, infra) in configs {
            sqlx::query(
                "UPDATE components SET
                    check_cmd = CASE WHEN $3 = 0 THEN 'exit 0' ELSE 'exit ' || $3::text END,
                    integrity_check_cmd = CASE WHEN $4 = 0 THEN 'exit 0' ELSE 'exit ' || $4::text END,
                    infra_check_cmd = CASE WHEN $5 = 0 THEN 'exit 0' ELSE 'exit ' || $5::text END
                 WHERE application_id = $1 AND name = $2"
            )
            .bind(app_id).bind(name).bind(health).bind(integrity).bind(infra)
            .execute(&self.db_pool).await.unwrap();
        }
    }

    pub async fn grant_permission(&self, app_id: Uuid, user_id: Uuid, level: &str) {
        self.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/users"),
            json!({"user_id": user_id, "permission_level": level}),
        )
        .await;
    }

    pub async fn grant_permission_with_expiry(
        &self,
        app_id: Uuid,
        user_id: Uuid,
        level: &str,
        expires: chrono::DateTime<chrono::Utc>,
    ) {
        self.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/users"),
            json!({
                "user_id": user_id,
                "permission_level": level,
                "expires_at": expires.to_rfc3339()
            }),
        )
        .await;
    }

    pub async fn grant_team_permission(&self, app_id: Uuid, team_id: Uuid, level: &str) {
        self.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/teams"),
            json!({"team_id": team_id, "permission_level": level}),
        )
        .await;
    }

    pub async fn create_team(&self, name: &str, members: Vec<Uuid>) -> Uuid {
        let resp = self
            .post(
                "/api/v1/teams",
                json!({
                    "name": name,
                    "description": format!("Test team: {name}"),
                }),
            )
            .await;
        let team: Value = resp.json().await.unwrap();
        let team_id: Uuid = team["id"].as_str().unwrap().parse().unwrap();

        for member_id in members {
            self.post(
                &format!("/api/v1/teams/{team_id}/members"),
                json!({
                    "user_id": member_id,
                    "role": "member",
                }),
            )
            .await;
        }

        team_id
    }

    pub async fn create_command(&self, component_id: Uuid, name: &str, cmd: &str, confirm: bool) {
        self.put(
            &format!("/api/v1/components/{component_id}"),
            json!({
                "commands": [{
                    "name": name,
                    "display_name": name,
                    "command": cmd,
                    "category": "custom",
                    "requires_confirmation": confirm,
                    "timeout_seconds": 30,
                }]
            }),
        )
        .await;
    }

    pub async fn create_api_key(&self, name: &str, actions: Vec<&str>) -> String {
        let resp = self
            .post(
                "/api/v1/api-keys",
                json!({
                    "name": name,
                    "allowed_actions": actions,
                }),
            )
            .await;
        let key: Value = resp.json().await.unwrap();
        key["key"].as_str().unwrap().to_string()
    }

    pub async fn disconnect_agent(&self, hostname: &str) {
        // Find components linked to agents with this hostname and mark them UNREACHABLE
        let comp_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT c.id FROM components c
             JOIN agents a ON c.agent_id = a.id
             WHERE a.hostname = $1",
        )
        .bind(hostname)
        .fetch_all(&self.db_pool)
        .await
        .unwrap();
        for cid in comp_ids {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
                 VALUES ($1, 'RUNNING', 'UNREACHABLE', 'agent_disconnect')",
            )
            .bind(cid)
            .execute(&self.db_pool)
            .await
            .unwrap();
        }
    }

    pub async fn reconnect_agent(&self, hostname: &str) {
        let comp_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT c.id FROM components c
             JOIN agents a ON c.agent_id = a.id
             WHERE a.hostname = $1",
        )
        .bind(hostname)
        .fetch_all(&self.db_pool)
        .await
        .unwrap();
        for cid in comp_ids {
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
                 VALUES ($1, 'UNREACHABLE', 'RUNNING', 'agent_reconnect')",
            )
            .bind(cid)
            .execute(&self.db_pool)
            .await
            .unwrap();
        }
    }

    /// Connect a WebSocket client for real-time event testing.
    pub async fn connect_websocket(
        &self,
        token: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let url = format!("{}/ws?token={token}", self.ws_url);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        ws_stream
    }

    /// Create a second organization for isolation tests.
    pub async fn create_second_org(&self) -> (Uuid, Uuid, String) {
        let org2_id = Uuid::new_v4();
        let user2_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Other Org', 'other-org')")
            .bind(org2_id)
            .execute(&self.db_pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id, organization_id, external_id, display_name, role, email)
             VALUES ($1, $2, 'other_admin', 'Other Admin', 'admin', 'other@test.local')",
        )
        .bind(user2_id)
        .bind(org2_id)
        .execute(&self.db_pool)
        .await
        .unwrap();

        let token = Self::make_jwt(user2_id, org2_id, "admin", "test-jwt-secret");
        (org2_id, user2_id, token)
    }

    // ---- Cleanup ----

    pub async fn cleanup(&self) {
        let admin_url = std::env::var("TEST_DATABASE_ADMIN_URL")
            .unwrap_or_else(|_| "postgres://appcontrol:test@localhost:5432/postgres".to_string());
        let admin_pool = PgPool::connect(&admin_url).await.unwrap();
        sqlx::query(&format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.db_name
        ))
        .execute(&admin_pool)
        .await
        .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
pub struct AppStatus {
    pub components: Vec<ComponentStatus>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ComponentStatus {
    pub name: String,
    pub state: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct App {
    #[serde(default)]
    pub id: Option<Uuid>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub active_site_id: Option<Uuid>,
}

#[derive(Debug, serde::Deserialize, sqlx::FromRow)]
pub struct StateTransition {
    pub component_id: Uuid,
    pub component_name: String,
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Deserialize, sqlx::FromRow)]
pub struct ActionLog {
    pub user_id: Uuid,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub details: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Deserialize, sqlx::FromRow)]
pub struct ConfigVersion {
    pub changed_by: Uuid,
    pub before_snapshot: Option<Value>,
    pub after_snapshot: Value,
}

#[derive(Debug, serde::Deserialize, sqlx::FromRow)]
pub struct SwitchoverLogEntry {
    pub switchover_id: Uuid,
    pub phase: String,
    pub status: String,
    pub details: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Deserialize)]
pub struct JobStatus {
    pub state: String,
    #[serde(default)]
    pub failed_component: Option<String>,
}

#[derive(Debug, serde::Deserialize, sqlx::FromRow)]
pub struct CheckEvent {
    pub component_id: Uuid,
    pub check_type: String,
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub duration_ms: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
