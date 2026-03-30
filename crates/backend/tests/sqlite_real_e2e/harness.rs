/// Real E2E Test Harness (SQLite version)
///
/// Launches actual backend (SQLite), gateway, and agent binaries.
/// Creates a temporary SQLite database with migrations.
/// Seeds test data via direct SQL (DbUuid binds for TEXT encoding).
/// Provides helpers to wait for component states and verify transitions.
use appcontrol_backend::db::DbUuid;
use reqwest::Client;
use serde_json::{json, Value};
use sqlx::SqlitePool;
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
    pub db_pool: SqlitePool,
    pub admin_token: String,
    pub scripts_dir: PathBuf,
    pub pid_dir: PathBuf,
    tmp_dir: PathBuf,
    backend_process: Option<Child>,
    gateway_process: Option<Child>,
    agent_process: Option<Child>,
    client: Client,
}

impl TestHarness {
    /// Build all binaries and start the full stack.
    pub async fn start() -> Self {
        let test_id = Uuid::new_v4().simple().to_string();

        // Temporary directory for SQLite DB and PIDs
        let tmp_dir = std::env::temp_dir().join(format!("appcontrol-sqlite-e2e-{}", test_id));
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let db_path = tmp_dir.join("test.db");
        let db_url = format!("sqlite:{}", db_path.display());

        // Create SQLite pool
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

        // Run migrations
        run_sqlite_migrations(&pool).await;

        // Seed org + admin user + site
        let org_id = Uuid::new_v4();
        let admin_id = Uuid::new_v4();
        let site_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'E2E Org', 'e2e-org')")
            .bind(DbUuid::from(org_id))
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO users (id, organization_id, external_id, display_name, role, email)
             VALUES ($1, $2, 'admin', 'Admin', 'admin', 'admin@e2e.local')",
        )
        .bind(DbUuid::from(admin_id))
        .bind(DbUuid::from(org_id))
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO sites (id, organization_id, name, code) VALUES ($1, $2, 'Default', 'DEF')",
        )
        .bind(DbUuid::from(site_id))
        .bind(DbUuid::from(org_id))
        .execute(&pool)
        .await
        .unwrap();

        // Paths
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("Cannot resolve workspace root");
        let scripts_dir = workspace_root.join("crates/e2e-real-tests/scripts");
        assert!(
            scripts_dir.exists(),
            "Scripts directory not found: {}",
            scripts_dir.display()
        );
        let pid_dir = tmp_dir.join("pids");
        std::fs::create_dir_all(&pid_dir).unwrap();

        // Build SQLite backend (separate target dir to avoid feature unification)
        let backend_crate_dir = workspace_root.join("crates/backend");
        let sqlite_target_dir = workspace_root.join("target-sqlite");

        eprintln!("[harness] Building SQLite backend...");
        let status = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg("appcontrol-backend")
            .arg("--no-default-features")
            .arg("--features")
            .arg("sqlite")
            .env("CARGO_TARGET_DIR", &sqlite_target_dir)
            .current_dir(&backend_crate_dir)
            .status()
            .expect("Failed to build SQLite backend");
        assert!(status.success(), "cargo build (SQLite backend) failed");

        // Build gateway + agent normally
        eprintln!("[harness] Building gateway and agent...");
        let status = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg("appcontrol-gateway")
            .arg("-p")
            .arg("appcontrol-agent")
            .current_dir(&workspace_root)
            .status()
            .expect("Failed to build gateway/agent");
        assert!(status.success(), "cargo build (gateway/agent) failed");

        let backend_bin = sqlite_target_dir.join("debug/appcontrol-backend");
        let gateway_bin = workspace_root.join("target/debug/appcontrol-gateway");
        let agent_bin = workspace_root.join("target/debug/appcontrol-agent");

        assert!(
            backend_bin.exists(),
            "Backend binary not found: {}",
            backend_bin.display()
        );
        assert!(
            gateway_bin.exists(),
            "Gateway binary not found: {}",
            gateway_bin.display()
        );
        assert!(
            agent_bin.exists(),
            "Agent binary not found: {}",
            agent_bin.display()
        );

        // Find available ports
        let backend_port = find_available_port();
        let gateway_port = find_available_port();
        let backend_url = format!("http://127.0.0.1:{backend_port}");
        let gateway_url = format!("ws://127.0.0.1:{gateway_port}/ws");

        // Start backend
        eprintln!("[harness] Starting backend on port {backend_port}...");
        let backend_process = Command::new(&backend_bin)
            .env("DATABASE_URL", &db_url)
            .env("PORT", backend_port.to_string())
            .env("JWT_SECRET", "e2e-test-secret")
            .env("JWT_ISSUER", "appcontrol-e2e")
            .env("RUST_LOG", "appcontrol_backend=debug")
            .spawn()
            .expect("Failed to start backend");

        // Wait for backend to be ready
        wait_for_http(&format!("{backend_url}/health"), Duration::from_secs(30)).await;
        eprintln!("[harness] Backend ready.");

        // Start gateway
        let gateway_ws_url = format!("ws://127.0.0.1:{backend_port}/ws/gateway");
        eprintln!("[harness] Starting gateway on port {gateway_port}...");
        let gateway_process = Command::new(&gateway_bin)
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
        eprintln!("[harness] Gateway ready.");

        // Start agent
        eprintln!("[harness] Starting agent...");
        let agent_process = Command::new(&agent_bin)
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
             VALUES ($1, 'e2e-agent', NULL, 'connected', '{}')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(DbUuid::from(agent_id))
        .execute(&pool)
        .await
        .unwrap();
        eprintln!("[harness] Agent registered.");

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
            tmp_dir,
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

    /// Create a 3-component app: DB -> AppServer -> WebFront
    pub async fn create_test_app(&self, site_id: Uuid) -> (Uuid, Uuid, Uuid, Uuid) {
        let scripts = self.scripts_dir.to_str().unwrap();
        let pid_dir = self.pid_dir.to_str().unwrap();

        let app = self
            .api_post(
                "/apps",
                json!({
                    "name": "E2E-Test-App",
                    "description": "Real E2E test application (SQLite)",
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
        let row: (String,) = sqlx::query_as("SELECT id FROM sites LIMIT 1")
            .fetch_one(&self.db_pool)
            .await
            .unwrap();
        Uuid::parse_str(&row.0).unwrap()
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
        let final_state = self.get_component_state(component_id).await;
        eprintln!(
            "[harness] Timeout waiting for component {component_id} to reach {expected}, current state: {final_state}"
        );
        false
    }

    /// Get current component state from state_transitions.
    pub async fn get_component_state(&self, component_id: Uuid) -> String {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT to_state FROM state_transitions WHERE component_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(DbUuid::from(component_id))
        .fetch_optional(&self.db_pool)
        .await
        .unwrap_or(None);

        result.map(|(s,)| s).unwrap_or_else(|| "UNKNOWN".to_string())
    }

    /// Get state transitions for a component, ordered by time.
    pub async fn get_transitions(
        &self,
        component_id: Uuid,
    ) -> Vec<(String, String, String)> {
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT from_state, to_state, created_at FROM state_transitions WHERE component_id = $1 ORDER BY created_at",
        )
        .bind(DbUuid::from(component_id))
        .fetch_all(&self.db_pool)
        .await
        .unwrap_or_default()
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

    /// Cleanup: kill all processes, remove temp directory.
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

        // Close DB pool
        self.db_pool.close().await;

        // Clean up temp directory (includes DB file and PIDs)
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
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
        // Kill test processes
        for name in &["oracle-db", "tomcat-app", "apache-web"] {
            self.kill_process(name);
        }
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

async fn run_sqlite_migrations(pool: &SqlitePool) {
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations/sqlite");
    let migrations_dir = migrations_dir
        .canonicalize()
        .expect("Cannot find migrations/sqlite directory");

    let mut entries: Vec<(i32, String, PathBuf)> = Vec::new();
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
            .bind(name.as_str())
            .execute(pool)
            .await
            .unwrap();
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
