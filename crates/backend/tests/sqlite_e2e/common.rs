/// Shared test infrastructure for SQLite E2E tests.
///
/// Mirrors `crates/e2e-tests/tests/e2e/common.rs` (PostgreSQL version)
/// but uses an in-memory SQLite database instead.
use appcontrol_backend::db::DbUuid;
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// SQLite migrations runner
// ---------------------------------------------------------------------------

async fn run_sqlite_migrations(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .expect("Failed to create _migrations table");

    let migrations_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/sqlite");
    let migrations_dir = migrations_dir
        .canonicalize()
        .expect("Cannot find migrations/sqlite");

    let mut entries: Vec<(i32, String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&migrations_dir).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.ends_with(".sql") && filename.starts_with('V') {
            if let Some(v) = filename
                .strip_prefix('V')
                .and_then(|s| s.split("__").next())
            {
                if let Ok(version) = v.parse::<i32>() {
                    entries.push((version, filename, entry.path()));
                }
            }
        }
    }
    entries.sort_by_key(|(v, _, _)| *v);

    for (version, name, path) in &entries {
        let applied: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM _migrations WHERE version = $1")
                .bind(version)
                .fetch_one(pool)
                .await
                .unwrap_or(false);
        if applied {
            continue;
        }
        let sql = std::fs::read_to_string(path).unwrap();
        sqlx::raw_sql(&sql).execute(pool).await.unwrap_or_else(|e| {
            panic!("Migration {} failed: {}", name, e);
        });
        sqlx::query("INSERT INTO _migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }
}

// ---------------------------------------------------------------------------
// TestContext
// ---------------------------------------------------------------------------

pub struct TestContext {
    pub api_url: String,
    pub ws_url: String,
    pub admin_user_id: Uuid,
    pub operator_user_id: Uuid,
    pub viewer_user_id: Uuid,
    pub editor_user_id: Uuid,
    pub organization_id: Uuid,
    pub default_site_id: Uuid,
    pub db_pool: sqlx::SqlitePool,
    client: Client,
    pub admin_token: String,
    pub operator_token: String,
    pub viewer_token: String,
    pub editor_token: String,
    _tmp_dir: std::path::PathBuf,
    _server_handle: tokio::task::JoinHandle<()>,
}

impl TestContext {
    pub async fn new() -> Self {
        let tmp_dir =
            std::env::temp_dir().join(format!("appcontrol_e2e_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let db_path = tmp_dir.join("test.db");
        let db_url = format!("sqlite:{}", db_path.display());

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    use sqlx::Executor;
                    conn.execute("PRAGMA journal_mode=WAL").await?;
                    conn.execute("PRAGMA busy_timeout=30000").await?;
                    conn.execute("PRAGMA foreign_keys=ON").await?;
                    conn.execute("PRAGMA synchronous=NORMAL").await?;
                    Ok(())
                })
            })
            .connect_with(
                db_url
                    .parse::<sqlx::sqlite::SqliteConnectOptions>()
                    .unwrap()
                    .create_if_missing(true),
            )
            .await
            .expect("Failed to create SQLite pool");

        run_sqlite_migrations(&pool).await;

        // Seed org + 4 users + default site
        let org_id = Uuid::new_v4();
        let admin_id = Uuid::new_v4();
        let operator_id = Uuid::new_v4();
        let viewer_id = Uuid::new_v4();
        let editor_id = Uuid::new_v4();
        let default_site_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO organizations (id, name, slug) VALUES ($1, 'Test Org', 'test-org')",
        )
        .bind(DbUuid::from(org_id))
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
                "INSERT INTO users (id, organization_id, external_id, display_name, role, email) \
                 VALUES ($1, $2, $3, $3, $4, $5)",
            )
            .bind(DbUuid::from(id))
            .bind(DbUuid::from(org_id))
            .bind(name)
            .bind(role)
            .bind(format!("{}@test.local", name))
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Default', 'DEF')",
        )
        .bind(DbUuid::from(default_site_id))
        .bind(DbUuid::from(org_id))
        .execute(&pool)
        .await
        .unwrap();

        let db_pool = pool.clone();

        // Start backend
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let api_url = format!("http://{}", addr);
        let ws_url = format!("ws://{}", addr);

        let config = appcontrol_backend::config::AppConfig {
            database_url: db_url,
            port: addr.port(),
            jwt_secret: "test-jwt-secret".to_string(),
            jwt_issuer: "appcontrol-test".to_string(),
            oidc: None,
            saml: None,
            app_env: "development".to_string(),
            seed: appcontrol_backend::config::SeedConfig {
                enabled: false,
                admin_email: "admin@test.local".to_string(),
                admin_password: "test".to_string(),
                admin_display_name: "Test Admin".to_string(),
                org_name: "Test Org".to_string(),
                org_slug: "test-org".to_string(),
            },
            rate_limit_auth: 100,
            rate_limit_operations: 100,
            rate_limit_reads: 1000,
            ha_mode: false,
            cors_origins: vec![],
            log_format: "text".to_string(),
            db_pool_size: 4,
            db_idle_timeout_secs: 600,
            db_connect_timeout_secs: 30,
            shutdown_timeout_secs: 5,
            retention_action_log_days: 0,
            retention_check_events_days: 0,
            public_gateway_url: None,
            public_backend_url: None,
        };

        let operation_lock =
            appcontrol_backend::core::operation_lock::OperationLock::new(pool.clone());

        let state = Arc::new(appcontrol_backend::AppState {
            db: pool,
            ws_hub: appcontrol_backend::websocket::Hub::new(),
            config,
            rate_limiter: appcontrol_backend::middleware::rate_limit::RateLimitState::new(),
            heartbeat_batcher: appcontrol_backend::core::heartbeat_batcher::HeartbeatBatcher::new(),
            operation_lock,
            terminal_sessions: appcontrol_backend::terminal::TerminalSessionManager::new(),
            log_subscriptions: appcontrol_backend::websocket::LogSubscriptionManager::new(),
            pending_log_requests: appcontrol_backend::websocket::PendingLogRequests::new(),
        });

        let app = appcontrol_backend::create_router(state);
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(200)).await;

        let admin_token = Self::make_jwt(admin_id, org_id, "admin", "test-jwt-secret");
        let operator_token = Self::make_jwt(operator_id, org_id, "operator", "test-jwt-secret");
        let viewer_token = Self::make_jwt(viewer_id, org_id, "viewer", "test-jwt-secret");
        let editor_token = Self::make_jwt(editor_id, org_id, "editor", "test-jwt-secret");

        Self {
            api_url,
            ws_url,
            admin_user_id: admin_id,
            operator_user_id: operator_id,
            viewer_user_id: viewer_id,
            editor_user_id: editor_id,
            organization_id: org_id,
            default_site_id,
            db_pool,
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            admin_token,
            operator_token,
            viewer_token,
            editor_token,
            _tmp_dir: tmp_dir,
            _server_handle: server_handle,
        }
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
            _ => panic!("Unknown user: {user}"),
        }
    }

    // ---- HTTP helpers (same API as PG TestContext) ----

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

    pub async fn delete(&self, path: &str) -> Response {
        self.delete_as("admin", path).await
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

    // ---- App factory helpers ----

    /// Creates a 5-component "Payments-SEPA" app with DAG dependencies.
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
            assert!(
                resp.status().is_success(),
                "Failed to create component {}: {}",
                name,
                resp.status()
            );
            let c: Value = resp.json().await.unwrap();
            ids.insert(name.to_string(), c["id"].as_str().unwrap().parse().unwrap());
        }

        for (from, to) in &[
            ("Oracle-DB", "Tomcat-App"),
            ("Oracle-DB", "RabbitMQ"),
            ("Tomcat-App", "Apache-Front"),
            ("RabbitMQ", "Batch-Processor"),
        ] {
            let resp = self
                .post(
                    &format!("/api/v1/apps/{app_id}/dependencies"),
                    json!({
                        "from_component_id": ids[*from],
                        "to_component_id": ids[*to],
                    }),
                )
                .await;
            assert!(
                resp.status().is_success(),
                "Failed to create dep {} → {}",
                from,
                to
            );
        }

        app_id
    }

    /// Creates a 4-component app with diagnostic check commands configured.
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
        assert!(resp.status().is_success(), "create diag app: {}", resp.status());
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        for (name, comp_type) in [
            ("Redis", "middleware"),
            ("Tomcat", "appserver"),
            ("Oracle", "database"),
            ("Apache", "webfront"),
        ] {
            let resp = self
                .post(
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
            assert!(resp.status().is_success(), "create comp {}: {}", name, resp.status());
        }

        // Oracle -> Tomcat dependency
        let oracle_id = self.component_id(app_id, "Oracle").await;
        let tomcat_id = self.component_id(app_id, "Tomcat").await;
        self.post(
            &format!("/api/v1/apps/{app_id}/dependencies"),
            json!({"from_component_id": oracle_id, "to_component_id": tomcat_id}),
        )
        .await;

        app_id
    }

    /// Creates an app with two DR sites (PRD + DR), 3 components per site.
    pub async fn create_app_with_dr_sites(&self) -> (Uuid, Uuid, Uuid) {
        let resp = self
            .post("/api/v1/sites", json!({"name": "PRD", "code": "PRD"}))
            .await;
        assert!(resp.status().is_success(), "create PRD site: {}", resp.status());
        let site_a: Value = resp.json().await.unwrap();
        let site_a_id: Uuid = site_a["id"].as_str().unwrap().parse().unwrap();

        let resp = self
            .post("/api/v1/sites", json!({"name": "DR", "code": "DR"}))
            .await;
        assert!(resp.status().is_success(), "create DR site: {}", resp.status());
        let site_b: Value = resp.json().await.unwrap();
        let site_b_id: Uuid = site_b["id"].as_str().unwrap().parse().unwrap();

        let resp = self
            .post(
                "/api/v1/apps",
                json!({
                    "name": "DR-App",
                    "description": "Multi-site DR application",
                    "site_id": site_a_id,
                }),
            )
            .await;
        assert!(resp.status().is_success(), "create DR app: {}", resp.status());
        let app: Value = resp.json().await.unwrap();
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        for (site_id, suffix) in [(site_a_id, "prd"), (site_b_id, "dr")] {
            for name in ["Oracle-DB", "Tomcat-App", "Apache-Front"] {
                let resp = self
                    .post(
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
                assert!(resp.status().is_success(), "create comp {name}-{suffix}: {}", resp.status());
            }
        }

        (app_id, site_a_id, site_b_id)
    }

    /// Look up a component ID via the components list API.
    pub async fn component_id(&self, app_id: Uuid, name: &str) -> Uuid {
        let resp = self.get(&format!("/api/v1/apps/{app_id}/components")).await;
        assert!(resp.status().is_success(), "list comps: {}", resp.status());
        let body: Value = resp.json().await.unwrap();
        // Handle both bare array and wrapped {"components": [...]}
        let comps = body.as_array()
            .or_else(|| body["components"].as_array())
            .unwrap_or_else(|| panic!("Unexpected response format for components: {body}"));
        comps
            .iter()
            .find(|c| c["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("Component {name} not found in app {app_id}"))
            ["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    pub async fn force_component_state(&self, app_id: Uuid, name: &str, state: &str) {
        let comp_id = self.component_id(app_id, name).await;
        // Directly update the component's current_state and insert a state_transition
        sqlx::query("UPDATE components SET current_state = $1 WHERE id = $2")
            .bind(state)
            .bind(DbUuid::from(comp_id))
            .execute(&self.db_pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
             VALUES ($1, 'UNKNOWN', $2, 'test_force')",
        )
        .bind(DbUuid::from(comp_id))
        .bind(state)
        .execute(&self.db_pool)
        .await
        .unwrap();
    }

    pub async fn grant_permission(&self, app_id: Uuid, user_id: Uuid, level: &str) {
        self.post_as(
            "admin",
            &format!("/api/v1/apps/{app_id}/permissions/users"),
            json!({"user_id": user_id, "permission_level": level}),
        )
        .await;
    }

    pub async fn create_api_key(&self, name: &str, actions: Vec<&str>) -> String {
        let resp = self
            .post(
                "/api/v1/api-keys",
                json!({"name": name, "allowed_actions": actions}),
            )
            .await;
        let key: Value = resp.json().await.unwrap();
        key["key"].as_str().unwrap().to_string()
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

    pub fn client_no_redirect(&self) -> Client {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap()
    }

    pub async fn post_form_anonymous(&self, path: &str, params: &[(&str, &str)]) -> Response {
        self.client
            .post(format!("{}{path}", self.api_url))
            .form(params)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_action_log(&self, app_id: Uuid, action: &str) -> Vec<Value> {
        let resp = self
            .get(&format!(
                "/api/v1/apps/{app_id}/reports/audit?from=2020-01-01T00:00:00Z&to=2030-12-31T23:59:59Z"
            ))
            .await;
        if !resp.status().is_success() {
            return Vec::new();
        }
        let report: Value = resp.json().await.unwrap();
        let data = report["data"].as_array().cloned().unwrap_or_default();
        data.into_iter()
            .filter(|e| e["action"].as_str() == Some(action))
            .collect()
    }

    pub async fn set_all_running_on_site(&self, _app_id: Uuid, _site_id: Uuid) {
        // Site overrides are per-component; for tests, this is a no-op.
    }

    // ---- SAML constructors ----

    pub async fn new_with_saml(idp_sso_url: &str, sp_entity_id: &str) -> Self {
        Self::new_with_saml_inner(idp_sso_url, sp_entity_id, None).await
    }

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
        let tmp_dir =
            std::env::temp_dir().join(format!("appcontrol_e2e_{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let db_path = tmp_dir.join("test.db");
        let db_url = format!("sqlite:{}", db_path.display());

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    use sqlx::Executor;
                    conn.execute("PRAGMA journal_mode=WAL").await?;
                    conn.execute("PRAGMA busy_timeout=30000").await?;
                    conn.execute("PRAGMA foreign_keys=ON").await?;
                    conn.execute("PRAGMA synchronous=NORMAL").await?;
                    Ok(())
                })
            })
            .connect_with(
                db_url
                    .parse::<sqlx::sqlite::SqliteConnectOptions>()
                    .unwrap()
                    .create_if_missing(true),
            )
            .await
            .expect("Failed to create SQLite pool");

        run_sqlite_migrations(&pool).await;

        let org_id = Uuid::new_v4();
        let admin_id = Uuid::new_v4();
        let operator_id = Uuid::new_v4();
        let viewer_id = Uuid::new_v4();
        let editor_id = Uuid::new_v4();
        let default_site_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Test Org', 'test-org')")
            .bind(DbUuid::from(org_id))
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
                "INSERT INTO users (id, organization_id, external_id, display_name, role, email) \
                 VALUES ($1, $2, $3, $3, $4, $5)",
            )
            .bind(DbUuid::from(id))
            .bind(DbUuid::from(org_id))
            .bind(name)
            .bind(role)
            .bind(format!("{}@test.local", name))
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query("INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Default', 'DEF')")
            .bind(DbUuid::from(default_site_id))
            .bind(DbUuid::from(org_id))
            .execute(&pool)
            .await
            .unwrap();

        let db_pool = pool.clone();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let api_url = format!("http://{}", addr);
        let ws_url = format!("ws://{}", addr);

        let config = appcontrol_backend::config::AppConfig {
            database_url: db_url,
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
            app_env: "development".to_string(),
            seed: appcontrol_backend::config::SeedConfig {
                enabled: false,
                admin_email: "admin@test.local".to_string(),
                admin_password: "test".to_string(),
                admin_display_name: "Test Admin".to_string(),
                org_name: "Test Org".to_string(),
                org_slug: "test-org".to_string(),
            },
            rate_limit_auth: 100,
            rate_limit_operations: 100,
            rate_limit_reads: 1000,
            ha_mode: false,
            cors_origins: vec![],
            log_format: "text".to_string(),
            db_pool_size: 4,
            db_idle_timeout_secs: 600,
            db_connect_timeout_secs: 30,
            shutdown_timeout_secs: 5,
            retention_action_log_days: 0,
            retention_check_events_days: 0,
            public_gateway_url: None,
            public_backend_url: None,
        };

        let operation_lock =
            appcontrol_backend::core::operation_lock::OperationLock::new(pool.clone());

        let state = Arc::new(appcontrol_backend::AppState {
            db: pool,
            ws_hub: appcontrol_backend::websocket::Hub::new(),
            config,
            rate_limiter: appcontrol_backend::middleware::rate_limit::RateLimitState::new(),
            heartbeat_batcher: appcontrol_backend::core::heartbeat_batcher::HeartbeatBatcher::new(),
            operation_lock,
            terminal_sessions: appcontrol_backend::terminal::TerminalSessionManager::new(),
            log_subscriptions: appcontrol_backend::websocket::LogSubscriptionManager::new(),
            pending_log_requests: appcontrol_backend::websocket::PendingLogRequests::new(),
        });

        let app = appcontrol_backend::create_router(state);
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        tokio::time::sleep(Duration::from_millis(200)).await;

        let admin_token = Self::make_jwt(admin_id, org_id, "admin", "test-jwt-secret");
        let operator_token = Self::make_jwt(operator_id, org_id, "operator", "test-jwt-secret");
        let viewer_token = Self::make_jwt(viewer_id, org_id, "viewer", "test-jwt-secret");
        let editor_token = Self::make_jwt(editor_id, org_id, "editor", "test-jwt-secret");

        Self {
            api_url,
            ws_url,
            admin_user_id: admin_id,
            operator_user_id: operator_id,
            viewer_user_id: viewer_id,
            editor_user_id: editor_id,
            organization_id: org_id,
            default_site_id,
            db_pool,
            client: Client::builder().timeout(Duration::from_secs(10)).build().unwrap(),
            admin_token,
            operator_token,
            viewer_token,
            editor_token,
            _tmp_dir: tmp_dir,
            _server_handle: server_handle,
        }
    }

    // ---- Token-based HTTP helpers (for cross-org tests) ----

    pub async fn get_with_token(&self, token: &str, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.api_url))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }

    pub async fn post_with_token(&self, token: &str, path: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.api_url))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    // ---- WebSocket helper ----

    pub async fn connect_websocket(
        &self,
        token: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let url = format!("{}/ws?token={token}", self.ws_url);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        ws_stream
    }

    // ---- Extract ID helper ----

    pub fn extract_id(value: &Value) -> Uuid {
        value["id"].as_str().unwrap().parse().unwrap()
    }

    // ---- Second org for isolation tests ----

    pub async fn create_second_org(&self) -> (Uuid, Uuid, String) {
        let org2_id = Uuid::new_v4();
        let user2_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO organizations (id, name, slug) VALUES ($1, 'Other Org', 'other-org')",
        )
        .bind(DbUuid::from(org2_id))
        .execute(&self.db_pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO users (id, organization_id, external_id, display_name, role, email)
             VALUES ($1, $2, 'other_admin', 'Other Admin', 'admin', 'other@test.local')",
        )
        .bind(DbUuid::from(user2_id))
        .bind(DbUuid::from(org2_id))
        .execute(&self.db_pool)
        .await
        .unwrap();

        let token = Self::make_jwt(user2_id, org2_id, "admin", "test-jwt-secret");
        (org2_id, user2_id, token)
    }

    // ---- Ten-component app for branch restart tests ----

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

    // ---- State helpers ----

    pub async fn set_all_running(&self, app_id: Uuid) {
        let comp_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM components WHERE application_id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.db_pool)
        .await
        .unwrap();
        for cid in comp_ids {
            sqlx::query("UPDATE components SET current_state = 'RUNNING' WHERE id = $1")
                .bind(&cid)
                .execute(&self.db_pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO state_transitions (component_id, from_state, to_state, trigger)
                 VALUES ($1, 'UNKNOWN', 'RUNNING', 'test_setup')",
            )
            .bind(&cid)
            .execute(&self.db_pool)
            .await
            .unwrap();
        }
    }

    // ---- Config version helper ----

    pub async fn get_config_versions(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Vec<ConfigVersion> {
        let rows = sqlx::query_as::<_, ConfigVersionRow>(
            "SELECT changed_by, before_snapshot, after_snapshot
             FROM config_versions WHERE resource_type = $1 AND resource_id = $2
             ORDER BY created_at",
        )
        .bind(resource_type)
        .bind(DbUuid::from(resource_id))
        .fetch_all(&self.db_pool)
        .await
        .unwrap();
        rows.into_iter()
            .map(|r| ConfigVersion {
                changed_by: r.changed_by.parse().unwrap_or_default(),
                before_snapshot: r.before_snapshot.map(|s| serde_json::from_str(&s).unwrap_or_default()),
                after_snapshot: r.after_snapshot.map(|s| serde_json::from_str(&s).unwrap_or_default()).unwrap_or(Value::Null),
            })
            .collect()
    }

    // ---- Team helper ----

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

    // ---- Custom command helper ----

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

    pub async fn cleanup(&self) {
        // Cleanup is handled by Drop
    }
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct ConfigVersionRow {
    pub changed_by: String,
    pub before_snapshot: Option<String>,
    pub after_snapshot: Option<String>,
}

pub struct ConfigVersion {
    pub changed_by: Uuid,
    pub before_snapshot: Option<Value>,
    pub after_snapshot: Value,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._tmp_dir);
    }
}
