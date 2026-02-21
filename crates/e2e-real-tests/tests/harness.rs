/// Real E2E Test Harness
///
/// Launches actual backend, gateway, and agent binaries.
/// Creates a temporary PostgreSQL database with migrations.
/// Seeds test data via HTTP API calls (not SQL).
/// Provides helpers to wait for component states and verify transitions.
#[allow(dead_code)]

use reqwest::Client;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TestHarness
// ---------------------------------------------------------------------------

pub struct TestHarness {
    pub backend_url: String,
    pub gateway_url: String,
    pub db_pool: PgPool,
    pub admin_token: String,
    pub scripts_dir: PathBuf,
    pub pid_dir: PathBuf,
    db_name: String,
    backend_process: Option<Child>,
    gateway_process: Option<Child>,
    agent_process: Option<Child>,
    client: Client,
}

impl TestHarness {
    /// Build all binaries and start the full stack.
    pub async fn start() -> Self {
        let db_name = format!("e2e_real_{}", Uuid::new_v4().simple());
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
        admin_pool.close().await;

        let db_url = format!("postgres://appcontrol:test@localhost:5432/{db_name}");
        let pool = PgPool::connect(&db_url).await.unwrap();

        // Run migrations
        run_migrations(&pool).await;

        // Seed org + admin user
        let org_id = Uuid::new_v4();
        let admin_id = Uuid::new_v4();
        let site_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'E2E Org', 'e2e-org')")
            .bind(org_id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO users (id, organization_id, external_id, display_name, role, email)
             VALUES ($1, $2, 'admin', 'Admin', 'admin', 'admin@e2e.local')",
        )
        .bind(admin_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Default', 'DEF')",
        )
        .bind(site_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        // Paths
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scripts_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts");
        let pid_dir = std::env::temp_dir().join(format!("appcontrol-e2e-{}", db_name));
        std::fs::create_dir_all(&pid_dir).unwrap();

        // Build binaries
        let status = Command::new("cargo")
            .arg("build")
            .arg("--workspace")
            .current_dir(&workspace_root)
            .status()
            .expect("Failed to build workspace");
        assert!(status.success(), "cargo build failed");

        let target_dir = workspace_root.join("target/debug");

        // Find available ports
        let backend_port = find_available_port();
        let gateway_port = find_available_port();
        let backend_url = format!("http://127.0.0.1:{backend_port}");
        let gateway_url = format!("ws://127.0.0.1:{gateway_port}/ws");

        // Start backend
        let backend_process = Command::new(target_dir.join("appcontrol-backend"))
            .env("DATABASE_URL", &db_url)
            .env("REDIS_URL", "redis://localhost:6379")
            .env("PORT", backend_port.to_string())
            .env("JWT_SECRET", "e2e-test-secret")
            .env("JWT_ISSUER", "appcontrol-e2e")
            .env("RUST_LOG", "appcontrol_backend=debug")
            .spawn()
            .expect("Failed to start backend");

        // Wait for backend to be ready
        wait_for_http(&format!("{backend_url}/health"), Duration::from_secs(30)).await;

        // Start gateway
        let gateway_ws_url = format!("ws://127.0.0.1:{backend_port}/ws/gateway");
        let gateway_process = Command::new(target_dir.join("appcontrol-gateway"))
            .env("LISTEN_ADDR", "127.0.0.1")
            .env("LISTEN_PORT", gateway_port.to_string())
            .env("BACKEND_URL", &gateway_ws_url)
            .env("RUST_LOG", "appcontrol_gateway=debug")
            .spawn()
            .expect("Failed to start gateway");

        // Wait for gateway to be ready
        wait_for_http(
            &format!("http://127.0.0.1:{gateway_port}/health"),
            Duration::from_secs(10),
        )
        .await;

        // Start agent
        let agent_id = Uuid::new_v4();
        let agent_process = Command::new(target_dir.join("appcontrol-agent"))
            .env("GATEWAY_URL", &gateway_url)
            .env("AGENT_ID", agent_id.to_string())
            .env("RUST_LOG", "appcontrol_agent=debug")
            .arg("--agent-id")
            .arg(agent_id.to_string())
            .spawn()
            .expect("Failed to start agent");

        // Give agent time to connect
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Register agent in DB
        sqlx::query(
            "INSERT INTO agents (id, hostname, gateway_id, status, labels)
             VALUES ($1, 'e2e-agent', NULL, 'connected', '{}'::jsonb)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(agent_id)
        .execute(&pool)
        .await
        .unwrap();

        // Generate admin JWT
        let admin_token = make_jwt(admin_id, org_id, "admin", "e2e-test-secret");

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            backend_url,
            gateway_url,
            db_pool: pool,
            admin_token,
            scripts_dir,
            pid_dir,
            db_name,
            backend_process: Some(backend_process),
            gateway_process: Some(gateway_process),
            agent_process: Some(agent_process),
            client,
        }
    }

    // --- HTTP helpers ---

    pub async fn api_post(&self, path: &str, body: Value) -> Value {
        let resp = self
            .client
            .post(format!("{}/api/v1{}", self.backend_url, path))
            .bearer_auth(&self.admin_token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = resp.status();
        let text = resp.text().await.unwrap();
        if !status.is_success() {
            panic!("POST {path} returned {status}: {text}");
        }
        serde_json::from_str(&text).unwrap_or(json!({"status": "ok"}))
    }

    pub async fn api_get(&self, path: &str) -> Value {
        let resp = self
            .client
            .get(format!("{}/api/v1{}", self.backend_url, path))
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .unwrap();
        let status = resp.status();
        let text = resp.text().await.unwrap();
        if !status.is_success() {
            panic!("GET {path} returned {status}: {text}");
        }
        serde_json::from_str(&text).unwrap_or(json!(null))
    }

    // --- Test helpers ---

    /// Create a 3-component app: DB → AppServer → WebFront
    pub async fn create_test_app(&self, site_id: Uuid) -> (Uuid, Uuid, Uuid, Uuid) {
        let scripts = self.scripts_dir.to_str().unwrap();
        let pid_dir = self.pid_dir.to_str().unwrap();

        let app = self
            .api_post(
                "/apps",
                json!({
                    "name": "E2E-Test-App",
                    "description": "Real E2E test application",
                    "site_id": site_id,
                }),
            )
            .await;
        let app_id: Uuid = app["id"].as_str().unwrap().parse().unwrap();

        // Create components with real scripts
        let db_comp = self
            .api_post(
                &format!("/apps/{app_id}/components"),
                json!({
                    "name": "Oracle-DB",
                    "component_type": "database",
                    "hostname": "localhost",
                    "check_cmd": format!("{scripts}/check_process.sh oracle-db {pid_dir}"),
                    "start_cmd": format!("{scripts}/start_process.sh oracle-db {pid_dir}"),
                    "stop_cmd": format!("{scripts}/stop_process.sh oracle-db {pid_dir}"),
                }),
            )
            .await;
        let db_id: Uuid = db_comp["id"].as_str().unwrap().parse().unwrap();

        let app_comp = self
            .api_post(
                &format!("/apps/{app_id}/components"),
                json!({
                    "name": "Tomcat-App",
                    "component_type": "appserver",
                    "hostname": "localhost",
                    "check_cmd": format!("{scripts}/check_process.sh tomcat-app {pid_dir}"),
                    "start_cmd": format!("{scripts}/start_process.sh tomcat-app {pid_dir}"),
                    "stop_cmd": format!("{scripts}/stop_process.sh tomcat-app {pid_dir}"),
                }),
            )
            .await;
        let app_srv_id: Uuid = app_comp["id"].as_str().unwrap().parse().unwrap();

        let web_comp = self
            .api_post(
                &format!("/apps/{app_id}/components"),
                json!({
                    "name": "Apache-Web",
                    "component_type": "webfront",
                    "hostname": "localhost",
                    "check_cmd": format!("{scripts}/check_process.sh apache-web {pid_dir}"),
                    "start_cmd": format!("{scripts}/start_process.sh apache-web {pid_dir}"),
                    "stop_cmd": format!("{scripts}/stop_process.sh apache-web {pid_dir}"),
                }),
            )
            .await;
        let web_id: Uuid = web_comp["id"].as_str().unwrap().parse().unwrap();

        // Create dependencies: AppServer depends on DB, Web depends on AppServer
        self.api_post(
            &format!("/apps/{app_id}/dependencies"),
            json!({
                "from_component_id": app_srv_id,
                "to_component_id": db_id,
            }),
        )
        .await;

        self.api_post(
            &format!("/apps/{app_id}/dependencies"),
            json!({
                "from_component_id": web_id,
                "to_component_id": app_srv_id,
            }),
        )
        .await;

        (app_id, db_id, app_srv_id, web_id)
    }

    /// Get the site_id from the DB.
    pub async fn default_site_id(&self) -> Uuid {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM sites LIMIT 1")
            .fetch_one(&self.db_pool)
            .await
            .unwrap()
    }

    /// Wait for a component to reach a specific state (poll API).
    pub async fn wait_for_state(
        &self,
        component_id: Uuid,
        expected: &str,
        timeout: Duration,
    ) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let state = self.get_component_state(component_id).await;
            if state == expected {
                return true;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        false
    }

    /// Get current component state from state_transitions.
    pub async fn get_component_state(&self, component_id: Uuid) -> String {
        sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(
                (SELECT to_state FROM state_transitions WHERE component_id = $1 ORDER BY created_at DESC LIMIT 1),
                'UNKNOWN'
            )",
        )
        .bind(component_id)
        .fetch_one(&self.db_pool)
        .await
        .unwrap_or_else(|_| "UNKNOWN".to_string())
    }

    /// Get state transitions for a component, ordered by time.
    pub async fn get_transitions(&self, component_id: Uuid) -> Vec<(String, String, chrono::DateTime<chrono::Utc>)> {
        sqlx::query_as::<_, (String, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT from_state, to_state, created_at FROM state_transitions WHERE component_id = $1 ORDER BY created_at",
        )
        .bind(component_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap_or_default()
    }

    /// Count state transitions for a component since a given time.
    pub async fn count_transitions_since(
        &self,
        component_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM state_transitions WHERE component_id = $1 AND created_at > $2",
        )
        .bind(component_id)
        .bind(since)
        .fetch_one(&self.db_pool)
        .await
        .unwrap_or(0)
    }

    /// Check if a PID file exists (process is running).
    pub fn process_running(&self, name: &str) -> bool {
        let pid_file = self.pid_dir.join(format!("{name}.pid"));
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                return unsafe { libc::kill(pid, 0) == 0 };
            }
        }
        false
    }

    /// Kill a test process directly (simulates crash).
    pub fn kill_process(&self, name: &str) {
        let pid_file = self.pid_dir.join(format!("{name}.pid"));
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }

    /// Cleanup: kill all processes, drop temp database.
    pub async fn cleanup(mut self) {
        // Kill child processes
        if let Some(mut p) = self.agent_process.take() {
            let _ = p.kill();
        }
        if let Some(mut p) = self.gateway_process.take() {
            let _ = p.kill();
        }
        if let Some(mut p) = self.backend_process.take() {
            let _ = p.kill();
        }

        // Kill any test processes
        for name in &["oracle-db", "tomcat-app", "apache-web"] {
            self.kill_process(name);
        }

        // Clean up PID dir
        let _ = std::fs::remove_dir_all(&self.pid_dir);

        // Drop temp DB
        self.db_pool.close().await;
        let admin_url = std::env::var("TEST_DATABASE_ADMIN_URL")
            .unwrap_or_else(|_| "postgres://appcontrol:test@localhost:5432/postgres".to_string());
        if let Ok(admin_pool) = PgPool::connect(&admin_url).await {
            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.db_name))
                .execute(&admin_pool)
                .await;
            admin_pool.close().await;
        }
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        // Best-effort cleanup of child processes
        if let Some(mut p) = self.agent_process.take() {
            let _ = p.kill();
        }
        if let Some(mut p) = self.gateway_process.take() {
            let _ = p.kill();
        }
        if let Some(mut p) = self.backend_process.take() {
            let _ = p.kill();
        }
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

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

fn find_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn wait_for_http(url: &str, timeout: Duration) {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let deadline = std::time::Instant::now() + timeout;

    while std::time::Instant::now() < deadline {
        if client.get(url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("Service at {url} did not become ready within {timeout:?}");
}

fn make_jwt(user_id: Uuid, org_id: Uuid, role: &str, secret: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub": user_id.to_string(),
        "org": org_id.to_string(),
        "email": format!("{role}@e2e.local"),
        "role": role,
        "exp": now + 3600,
        "iat": now,
        "iss": "appcontrol-e2e",
    });
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}
