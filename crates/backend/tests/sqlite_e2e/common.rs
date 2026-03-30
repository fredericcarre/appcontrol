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
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._tmp_dir);
    }
}
