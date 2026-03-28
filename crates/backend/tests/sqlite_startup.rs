//! Integration test: SQLite backend startup validation.
//!
//! This test verifies that the SQLite backend can:
//! 1. Create a database and run all migrations successfully
//! 2. Execute all startup-critical SQL queries without PostgreSQL syntax errors
//! 3. Seed the initial organization and admin user
//! 4. Serve health endpoints
//!
//! Run with: cargo test --package appcontrol-backend --test sqlite_startup --features sqlite --no-default-features

#![cfg(all(feature = "sqlite", not(feature = "postgres")))]

use std::sync::Arc;

/// Run SQLite migrations from the migrations/sqlite directory.
async fn run_sqlite_migrations(pool: &sqlx::SqlitePool) {
    // Create tracking table
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

    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations/sqlite");
    let migrations_dir = migrations_dir
        .canonicalize()
        .unwrap_or_else(|_| panic!("Cannot find SQLite migrations at {:?}", migrations_dir));

    let mut entries: Vec<(i32, String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&migrations_dir).expect("Cannot read migrations/sqlite/") {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.ends_with(".sql") && filename.starts_with('V') {
            if let Some(version_str) = filename.strip_prefix('V').and_then(|s| s.split("__").next())
            {
                if let Ok(version) = version_str.parse::<i32>() {
                    entries.push((version, filename, entry.path()));
                }
            }
        }
    }
    entries.sort_by_key(|(v, _, _)| *v);

    assert!(
        !entries.is_empty(),
        "No SQLite migration files found in {}",
        migrations_dir.display()
    );

    for (version, name, path) in &entries {
        // Skip already applied
        let applied: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM _migrations WHERE version = $1")
            .bind(version)
            .fetch_one(pool)
            .await
            .unwrap_or(false);

        if applied {
            continue;
        }

        let sql = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Cannot read migration: {}", path.display()));

        sqlx::raw_sql(&sql).execute(pool).await.unwrap_or_else(|e| {
            panic!("Migration {} failed: {}", name, e)
        });

        sqlx::query("INSERT INTO _migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("Failed to record migration {}: {}", name, e));
    }
}

/// Verify that all critical tables exist after migrations.
async fn verify_tables_exist(pool: &sqlx::SqlitePool) {
    let critical_tables = [
        "organizations",
        "users",
        "agents",
        "gateways",
        "sites",
        "applications",
        "components",
        "dependencies",
        "action_log",
        "state_transitions",
        "check_events",
        "teams",
        "team_members",
        "app_permissions_users",
        "app_permissions_teams",
        "api_keys",
        "operation_locks",
        "revoked_tokens",
        "rate_limit_counters",
        "operation_schedules",
        "snapshot_schedules",
    ];

    for table in &critical_tables {
        let exists: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=$1",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        assert!(exists, "Critical table '{}' not found after migrations", table);
    }
}

/// Verify that startup-critical SQL queries don't use PostgreSQL syntax.
async fn verify_startup_queries(pool: &sqlx::SqlitePool) {
    // 1. Operation lock cleanup (uses INTERVAL in PostgreSQL)
    let result = sqlx::query(
        "DELETE FROM operation_locks WHERE last_heartbeat < datetime('now', '-30 seconds')",
    )
    .execute(pool)
    .await;
    assert!(result.is_ok(), "Operation lock cleanup query failed: {:?}", result.err());

    // 2. Heartbeat update
    let result = sqlx::query(
        "UPDATE operation_locks SET last_heartbeat = datetime('now') WHERE app_id = $1",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(pool)
    .await;
    assert!(result.is_ok(), "Heartbeat update query failed: {:?}", result.err());

    // 3. Audit log queries
    let result = sqlx::query(
        "UPDATE action_log SET status = 'success', completed_at = datetime('now') WHERE id = $1",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(pool)
    .await;
    assert!(result.is_ok(), "Audit log update query failed: {:?}", result.err());

    // 4. Permission queries with expiry check
    let result = sqlx::query_scalar::<_, String>(
        "SELECT permission_level FROM app_permissions_users \
         WHERE application_id = $1 AND user_id = $2 \
         AND (expires_at IS NULL OR expires_at > datetime('now'))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .fetch_optional(pool)
    .await;
    assert!(result.is_ok(), "Permission expiry query failed: {:?}", result.err());

    // 5. Token revocation cleanup
    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < datetime('now')")
        .execute(pool)
        .await;
    assert!(result.is_ok(), "Token revocation cleanup failed: {:?}", result.err());

    // 6. Rate limit cleanup
    let result = sqlx::query(
        "DELETE FROM rate_limit_counters WHERE window_start < datetime('now', '-2 minutes')",
    )
    .execute(pool)
    .await;
    assert!(result.is_ok(), "Rate limit cleanup failed: {:?}", result.err());

    // 7. Seed organization (ON CONFLICT)
    let org_id = uuid::Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3) \
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, slug = EXCLUDED.slug",
    )
    .bind(org_id.to_string())
    .bind("Test Org")
    .bind("test-org")
    .execute(pool)
    .await;
    assert!(result.is_ok(), "Organization seed failed: {:?}", result.err());

    // 8. Seed user (ON CONFLICT)
    let user_id = uuid::Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO users (id, organization_id, external_id, email, display_name, role, platform_role, auth_provider, password_hash) \
         VALUES ($1, $2, 'test-admin', $3, $4, 'admin', 'super_admin', 'local', $5) \
         ON CONFLICT (id) DO UPDATE SET email = EXCLUDED.email, display_name = EXCLUDED.display_name, password_hash = EXCLUDED.password_hash",
    )
    .bind(user_id.to_string())
    .bind(org_id.to_string())
    .bind("test@localhost")
    .bind("Test Admin")
    .bind("$2b$12$dummy_hash_for_testing_purposes_only")
    .execute(pool)
    .await;
    assert!(result.is_ok(), "User seed failed: {:?}", result.err());

    // 9. Webhook update (now())
    let result = sqlx::query(
        &format!(
            "UPDATE webhook_endpoints SET last_triggered_at = {}, last_status_code = $2 WHERE id = $1",
            appcontrol_backend::db::sql::now()
        ),
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(200i32)
    .execute(pool)
    .await;
    assert!(result.is_ok(), "Webhook update query failed: {:?}", result.err());
}

/// Full SQLite startup integration test.
/// Creates a temp database, runs migrations, seeds data, and starts the backend.
#[tokio::test]
async fn test_sqlite_startup_full() {
    // Create temp database
    let tmp_dir = std::env::temp_dir().join(format!("appcontrol_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let db_path = tmp_dir.join("test.db");
    let db_url = format!("sqlite:{}", db_path.display());

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
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

    // Step 1: Run migrations
    run_sqlite_migrations(&pool).await;

    // Step 2: Verify critical tables
    verify_tables_exist(&pool).await;

    // Step 3: Verify startup SQL queries work (no PostgreSQL syntax)
    verify_startup_queries(&pool).await;

    // Step 4: Start the backend and verify health endpoint
    let config = appcontrol_backend::config::AppConfig {
        database_url: db_url,
        port: 0, // random port
        jwt_secret: "test-jwt-secret-for-sqlite-validation".to_string(),
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

    // Verify operation lock cleanup works at startup (this was failing before)
    let cleanup_result = operation_lock.cleanup_all_stale_locks().await;
    assert!(
        cleanup_result.is_ok(),
        "Operation lock cleanup at startup failed: {:?}",
        cleanup_result.err()
    );

    let state = Arc::new(appcontrol_backend::AppState {
        db: pool.clone(),
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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let api_url = format!("http://{}", addr);

    let _server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify health endpoint responds
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/health", api_url))
        .send()
        .await
        .expect("Health endpoint request failed");

    assert_eq!(
        resp.status(),
        200,
        "Health endpoint should return 200 on SQLite backend"
    );

    // Cleanup
    drop(pool);
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

/// Test that SQLite migration file count matches PostgreSQL migration count.
/// This catches cases where a new migration was added for PostgreSQL but not SQLite.
///
/// PostgreSQL migrations are at `migrations/` (root), SQLite at `migrations/sqlite/`.
#[test]
fn test_sqlite_migrations_match_postgres() {
    let base_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");

    // PostgreSQL migrations are at root level (or in postgres/ subdir)
    let pg_dir = if base_dir.join("postgres").exists() {
        base_dir.join("postgres")
    } else {
        base_dir.clone()
    };
    let sqlite_dir = base_dir.join("sqlite");

    assert!(
        sqlite_dir.exists(),
        "SQLite migrations directory not found: {:?}",
        sqlite_dir
    );

    let extract_versions = |dir: &std::path::Path| -> std::collections::BTreeSet<i32> {
        std::fs::read_dir(dir)
            .unwrap_or_else(|_| panic!("Cannot read {:?}", dir))
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with(".sql") && name.starts_with('V')
            })
            .filter_map(|e| {
                e.file_name()
                    .to_string_lossy()
                    .strip_prefix('V')
                    .and_then(|s| s.split("__").next().map(String::from))
                    .and_then(|s| s.parse::<i32>().ok())
            })
            .collect()
    };

    let pg_versions = extract_versions(&pg_dir);
    let sqlite_versions = extract_versions(&sqlite_dir);

    let missing_in_sqlite: Vec<_> = pg_versions.difference(&sqlite_versions).collect();

    assert!(
        missing_in_sqlite.is_empty(),
        "PostgreSQL migrations missing from SQLite: {:?}. \
         Each PostgreSQL migration must have a corresponding SQLite migration in migrations/sqlite/.",
        missing_in_sqlite
    );
}
