//! Helpers for the permission_resolution benchmark.
//!
//! Spins up an in-memory SQLite database, creates only the three tables that
//! `effective_permission` actually touches (`app_permissions_users`,
//! `app_permissions_teams`, `team_members`), and seeds them with N users ×
//! M teams × P apps × Q grants. This is a deliberate shortcut: we don't
//! replay the full V001..V031 migration stack because the function under test
//! only cares about three columns. Running the real migrations would make
//! benchmark setup take seconds instead of ~10ms.
//!
//! The function under test (`appcontrol_backend::core::permissions::effective_permission`)
//! is compiled with the `sqlite` feature, so the SQL we generate matches what
//! production uses on SQLite-backed deployments.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

pub struct SeedSpec {
    pub users: usize,
    pub teams: usize,
    pub apps: usize,
    pub grants: usize,
}

pub struct SeededDb {
    pub pool: SqlitePool,
    pub user_ids: Vec<Uuid>,
    pub app_ids: Vec<Uuid>,
}

/// Create an in-memory SQLite pool with the schema that
/// `effective_permission` reads from. We use file-based shared cache so the
/// pool can have >1 connection (pure :memory: gives each connection its own
/// blank DB).
pub async fn build_seeded_db(spec: &SeedSpec) -> SeededDb {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("sqlite memory url")
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("sqlite connect");

    // Minimum schema for effective_permission. Column types are simplified
    // (TEXT for UUIDs, TEXT for timestamps) which matches the production
    // SQLite schema for the relevant columns.
    sqlx::query(
        r#"
        CREATE TABLE app_permissions_users (
            application_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            permission_level TEXT NOT NULL,
            expires_at TEXT,
            PRIMARY KEY (application_id, user_id)
        );
        "#,
    )
    .execute(&pool)
    .await
    .expect("create app_permissions_users");

    sqlx::query(
        r#"
        CREATE TABLE app_permissions_teams (
            application_id TEXT NOT NULL,
            team_id TEXT NOT NULL,
            permission_level TEXT NOT NULL,
            expires_at TEXT,
            PRIMARY KEY (application_id, team_id)
        );
        "#,
    )
    .execute(&pool)
    .await
    .expect("create app_permissions_teams");

    sqlx::query(
        r#"
        CREATE TABLE team_members (
            team_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            PRIMARY KEY (team_id, user_id)
        );
        "#,
    )
    .execute(&pool)
    .await
    .expect("create team_members");

    sqlx::query(
        "CREATE INDEX idx_perms_users_app_user ON app_permissions_users(application_id, user_id);",
    )
    .execute(&pool)
    .await
    .expect("idx perms users");
    sqlx::query("CREATE INDEX idx_perms_teams_app ON app_permissions_teams(application_id);")
        .execute(&pool)
        .await
        .expect("idx perms teams");
    sqlx::query("CREATE INDEX idx_team_members_user ON team_members(user_id);")
        .execute(&pool)
        .await
        .expect("idx team_members");

    // Seed
    let users: Vec<Uuid> = (0..spec.users).map(|_| Uuid::new_v4()).collect();
    let teams: Vec<Uuid> = (0..spec.teams).map(|_| Uuid::new_v4()).collect();
    let apps: Vec<Uuid> = (0..spec.apps).map(|_| Uuid::new_v4()).collect();

    let levels = ["view", "operate", "edit", "manage"];

    // Put every user in 1..=2 teams (round robin).
    let mut tx = pool.begin().await.expect("begin tx");
    for (i, user) in users.iter().enumerate() {
        let t1 = teams[i % spec.teams.max(1)];
        sqlx::query("INSERT OR IGNORE INTO team_members (team_id, user_id) VALUES (?1, ?2)")
            .bind(t1.to_string())
            .bind(user.to_string())
            .execute(&mut *tx)
            .await
            .expect("insert team_member");
        if spec.teams > 1 {
            let t2 = teams[(i * 7 + 3) % spec.teams];
            sqlx::query("INSERT OR IGNORE INTO team_members (team_id, user_id) VALUES (?1, ?2)")
                .bind(t2.to_string())
                .bind(user.to_string())
                .execute(&mut *tx)
                .await
                .expect("insert team_member 2");
        }
    }
    tx.commit().await.expect("commit team_members");

    // Spread `grants` across apps × users (direct) and apps × teams (team).
    let half = spec.grants / 2;
    let other_half = spec.grants - half;

    let mut tx = pool.begin().await.expect("begin tx 2");
    for i in 0..half {
        let app = apps[i % spec.apps.max(1)];
        let user = users[i % spec.users.max(1)];
        let lvl = levels[i % levels.len()];
        sqlx::query(
            "INSERT OR IGNORE INTO app_permissions_users (application_id, user_id, permission_level, expires_at) VALUES (?1, ?2, ?3, NULL)",
        )
        .bind(app.to_string())
        .bind(user.to_string())
        .bind(lvl)
        .execute(&mut *tx)
        .await
        .expect("insert user perm");
    }
    for i in 0..other_half {
        let app = apps[i % spec.apps.max(1)];
        let team = teams[i % spec.teams.max(1)];
        let lvl = levels[(i + 1) % levels.len()];
        sqlx::query(
            "INSERT OR IGNORE INTO app_permissions_teams (application_id, team_id, permission_level, expires_at) VALUES (?1, ?2, ?3, NULL)",
        )
        .bind(app.to_string())
        .bind(team.to_string())
        .bind(lvl)
        .execute(&mut *tx)
        .await
        .expect("insert team perm");
    }
    tx.commit().await.expect("commit grants");

    SeededDb {
        pool,
        user_ids: users,
        app_ids: apps,
    }
}
