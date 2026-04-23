//! Cluster members repository — first-class entities for fan-out clusters.
//!
//! A cluster member represents one physical instance of a fan-out component:
//! its own hostname, agent, optional per-member command/path/env overrides,
//! and its own FSM state (stored in `cluster_member_state`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use appcontrol_common::ClusterMemberConfig;

#[allow(unused_imports)]
use crate::db::{DbJson, DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug, Clone)]
pub struct ClusterMember {
    pub id: Uuid,
    pub component_id: Uuid,
    pub hostname: String,
    pub agent_id: Uuid,
    pub site_id: Option<Uuid>,
    pub check_cmd_override: Option<String>,
    pub start_cmd_override: Option<String>,
    pub stop_cmd_override: Option<String>,
    pub install_path: Option<String>,
    pub env_vars_override: Option<Value>,
    pub member_order: i32,
    pub is_enabled: bool,
    pub tags: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ClusterMemberState {
    pub cluster_member_id: Uuid,
    pub current_state: String,
    pub last_check_at: Option<DateTime<Utc>>,
    pub last_check_exit_code: Option<i16>,
    pub last_check_duration_ms: Option<i32>,
    pub last_stdout: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// A member together with its current FSM state (for UI listing).
#[derive(Debug, Clone)]
pub struct ClusterMemberWithState {
    pub member: ClusterMember,
    pub current_state: String,
    pub last_check_at: Option<DateTime<Utc>>,
    pub last_check_exit_code: Option<i16>,
}

#[derive(Debug, Clone)]
pub struct CreateClusterMember {
    pub component_id: Uuid,
    pub hostname: String,
    pub agent_id: Uuid,
    pub site_id: Option<Uuid>,
    pub check_cmd_override: Option<String>,
    pub start_cmd_override: Option<String>,
    pub stop_cmd_override: Option<String>,
    pub install_path: Option<String>,
    pub env_vars_override: Option<Value>,
    pub member_order: i32,
    pub is_enabled: bool,
    pub tags: Value,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateClusterMember {
    pub hostname: Option<String>,
    pub agent_id: Option<Uuid>,
    pub site_id: Option<Option<Uuid>>,
    pub check_cmd_override: Option<Option<String>>,
    pub start_cmd_override: Option<Option<String>>,
    pub stop_cmd_override: Option<Option<String>>,
    pub install_path: Option<Option<String>>,
    pub env_vars_override: Option<Option<Value>>,
    pub member_order: Option<i32>,
    pub is_enabled: Option<bool>,
    pub tags: Option<Value>,
}

/// Member config bundle pushed to an agent (fan-out mode).
///
/// Commands are already resolved: per-member override if set, else the parent
/// component's command. The agent never falls back at execution time.
#[derive(Debug, Clone)]
pub struct AgentMemberConfig {
    pub member_id: Uuid,
    pub component_id: Uuid,
    pub hostname: String,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub env_vars: Value,
}

impl AgentMemberConfig {
    pub fn into_protocol(self) -> ClusterMemberConfig {
        ClusterMemberConfig {
            member_id: self.member_id,
            hostname: self.hostname,
            check_cmd: self.check_cmd,
            start_cmd: self.start_cmd,
            stop_cmd: self.stop_cmd,
            env_vars: self.env_vars,
        }
    }
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait ClusterMemberRepository: Send + Sync {
    async fn list_by_component(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<ClusterMemberWithState>, sqlx::Error>;

    async fn get(&self, id: Uuid) -> Result<Option<ClusterMember>, sqlx::Error>;

    async fn get_component_id(&self, member_id: Uuid) -> Result<Option<Uuid>, sqlx::Error>;

    async fn create(&self, input: CreateClusterMember) -> Result<ClusterMember, sqlx::Error>;

    async fn update(
        &self,
        id: Uuid,
        input: UpdateClusterMember,
    ) -> Result<Option<ClusterMember>, sqlx::Error>;

    async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error>;

    /// Load all members assigned to `agent_id` for fan-out components,
    /// grouped by component_id with commands resolved (override or inherited).
    async fn load_for_agent(&self, agent_id: Uuid) -> Result<Vec<AgentMemberConfig>, sqlx::Error>;

    /// Upsert the member's cached state after a check result.
    async fn upsert_state(
        &self,
        member_id: Uuid,
        current_state: &str,
        exit_code: i16,
        duration_ms: i32,
        stdout: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    /// Read member state snapshot (for FSM aggregation).
    async fn get_states_for_component(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<ClusterMemberState>, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgClusterMemberRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgClusterMemberRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(sqlx::FromRow)]
struct PgMemberRow {
    id: Uuid,
    component_id: Uuid,
    hostname: String,
    agent_id: Uuid,
    site_id: Option<Uuid>,
    check_cmd_override: Option<String>,
    start_cmd_override: Option<String>,
    stop_cmd_override: Option<String>,
    install_path: Option<String>,
    env_vars_override: Option<Value>,
    member_order: i32,
    is_enabled: bool,
    tags: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[cfg(feature = "postgres")]
impl From<PgMemberRow> for ClusterMember {
    fn from(r: PgMemberRow) -> Self {
        ClusterMember {
            id: r.id,
            component_id: r.component_id,
            hostname: r.hostname,
            agent_id: r.agent_id,
            site_id: r.site_id,
            check_cmd_override: r.check_cmd_override,
            start_cmd_override: r.start_cmd_override,
            stop_cmd_override: r.stop_cmd_override,
            install_path: r.install_path,
            env_vars_override: r.env_vars_override,
            member_order: r.member_order,
            is_enabled: r.is_enabled,
            tags: r.tags,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(feature = "postgres")]
const PG_MEMBER_COLS: &str = "id, component_id, hostname, agent_id, site_id, \
    check_cmd_override, start_cmd_override, stop_cmd_override, install_path, \
    env_vars_override, member_order, is_enabled, tags, created_at, updated_at";

#[cfg(feature = "postgres")]
#[async_trait]
impl ClusterMemberRepository for PgClusterMemberRepository {
    async fn list_by_component(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<ClusterMemberWithState>, sqlx::Error> {
        let members_sql = format!(
            "SELECT {} FROM cluster_members \
             WHERE component_id = $1 \
             ORDER BY member_order, hostname",
            PG_MEMBER_COLS
        );
        let members = sqlx::query_as::<_, PgMemberRow>(&members_sql)
            .bind(crate::db::bind_id(component_id))
            .fetch_all(&self.pool)
            .await?;

        let states: Vec<(Uuid, String, Option<DateTime<Utc>>, Option<i16>)> = sqlx::query_as(
            "SELECT cluster_member_id, current_state, last_check_at, last_check_exit_code \
             FROM cluster_member_state cms \
             WHERE EXISTS (SELECT 1 FROM cluster_members cm WHERE cm.id = cms.cluster_member_id AND cm.component_id = $1)",
        )
        .bind(crate::db::bind_id(component_id))
        .fetch_all(&self.pool)
        .await?;

        let state_map: std::collections::HashMap<Uuid, _> =
            states.into_iter().map(|s| (s.0, (s.1, s.2, s.3))).collect();

        Ok(members
            .into_iter()
            .map(|m| {
                let base: ClusterMember = m.into();
                let (current_state, last_check_at, last_check_exit_code) = state_map
                    .get(&base.id)
                    .cloned()
                    .unwrap_or_else(|| ("UNKNOWN".to_string(), None, None));
                ClusterMemberWithState {
                    member: base,
                    current_state,
                    last_check_at,
                    last_check_exit_code,
                }
            })
            .collect())
    }

    async fn get(&self, id: Uuid) -> Result<Option<ClusterMember>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM cluster_members WHERE id = $1",
            PG_MEMBER_COLS
        );
        let row = sqlx::query_as::<_, PgMemberRow>(&sql)
            .bind(crate::db::bind_id(id))
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Into::into))
    }

    async fn get_component_id(&self, member_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>("SELECT component_id FROM cluster_members WHERE id = $1")
            .bind(crate::db::bind_id(member_id))
            .fetch_optional(&self.pool)
            .await
    }

    async fn create(&self, input: CreateClusterMember) -> Result<ClusterMember, sqlx::Error> {
        let sql = format!(
            "INSERT INTO cluster_members ( \
                component_id, hostname, agent_id, site_id, \
                check_cmd_override, start_cmd_override, stop_cmd_override, \
                install_path, env_vars_override, member_order, is_enabled, tags) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             RETURNING {}",
            PG_MEMBER_COLS
        );
        let row = sqlx::query_as::<_, PgMemberRow>(&sql)
            .bind(crate::db::bind_id(input.component_id))
            .bind(&input.hostname)
            .bind(crate::db::bind_id(input.agent_id))
            .bind(input.site_id.map(crate::db::bind_id))
            .bind(&input.check_cmd_override)
            .bind(&input.start_cmd_override)
            .bind(&input.stop_cmd_override)
            .bind(&input.install_path)
            .bind(&input.env_vars_override)
            .bind(input.member_order)
            .bind(input.is_enabled)
            .bind(&input.tags)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.into())
    }

    async fn update(
        &self,
        id: Uuid,
        input: UpdateClusterMember,
    ) -> Result<Option<ClusterMember>, sqlx::Error> {
        let existing = match self.get(id).await? {
            Some(m) => m,
            None => return Ok(None),
        };
        let hostname = input.hostname.unwrap_or(existing.hostname.clone());
        let agent_id = input.agent_id.unwrap_or(existing.agent_id);
        let site_id = input.site_id.unwrap_or(existing.site_id);
        let check = input
            .check_cmd_override
            .unwrap_or(existing.check_cmd_override.clone());
        let start = input
            .start_cmd_override
            .unwrap_or(existing.start_cmd_override.clone());
        let stop = input
            .stop_cmd_override
            .unwrap_or(existing.stop_cmd_override.clone());
        let install = input.install_path.unwrap_or(existing.install_path.clone());
        let env = input
            .env_vars_override
            .unwrap_or(existing.env_vars_override.clone());
        let order = input.member_order.unwrap_or(existing.member_order);
        let enabled = input.is_enabled.unwrap_or(existing.is_enabled);
        let tags = input.tags.unwrap_or(existing.tags.clone());

        let sql = format!(
            "UPDATE cluster_members SET \
                hostname = $2, agent_id = $3, site_id = $4, \
                check_cmd_override = $5, start_cmd_override = $6, stop_cmd_override = $7, \
                install_path = $8, env_vars_override = $9, \
                member_order = $10, is_enabled = $11, tags = $12, \
                updated_at = now() \
             WHERE id = $1 \
             RETURNING {}",
            PG_MEMBER_COLS
        );
        let row = sqlx::query_as::<_, PgMemberRow>(&sql)
            .bind(crate::db::bind_id(id))
            .bind(&hostname)
            .bind(crate::db::bind_id(agent_id))
            .bind(site_id.map(crate::db::bind_id))
            .bind(&check)
            .bind(&start)
            .bind(&stop)
            .bind(&install)
            .bind(&env)
            .bind(order)
            .bind(enabled)
            .bind(&tags)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Into::into))
    }

    async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM cluster_members WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn load_for_agent(&self, agent_id: Uuid) -> Result<Vec<AgentMemberConfig>, sqlx::Error> {
        // Only members of components in fan_out mode on non-suspended apps.
        // Commands are resolved: override → inherit from component.
        let sql = r#"
            SELECT cm.id, c.id, cm.hostname,
                   COALESCE(cm.check_cmd_override, c.check_cmd) AS check_cmd,
                   COALESCE(cm.start_cmd_override, c.start_cmd) AS start_cmd,
                   COALESCE(cm.stop_cmd_override, c.stop_cmd) AS stop_cmd,
                   COALESCE(cm.env_vars_override, c.env_vars, '{}'::jsonb) AS env_vars
            FROM cluster_members cm
            JOIN components c ON c.id = cm.component_id
            JOIN applications a ON a.id = c.application_id
            WHERE cm.agent_id = $1
              AND cm.is_enabled = true
              AND c.cluster_mode = 'fan_out'
              AND a.is_suspended = false
        "#;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Value,
            ),
        >(sql)
        .bind(crate::db::bind_id(agent_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AgentMemberConfig {
                member_id: r.0,
                component_id: r.1,
                hostname: r.2,
                check_cmd: r.3,
                start_cmd: r.4,
                stop_cmd: r.5,
                env_vars: r.6,
            })
            .collect())
    }

    async fn upsert_state(
        &self,
        member_id: Uuid,
        current_state: &str,
        exit_code: i16,
        duration_ms: i32,
        stdout: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO cluster_member_state ( \
                cluster_member_id, current_state, last_check_at, \
                last_check_exit_code, last_check_duration_ms, last_stdout, updated_at) \
             VALUES ($1, $2, now(), $3, $4, $5, now()) \
             ON CONFLICT (cluster_member_id) DO UPDATE SET \
                current_state = EXCLUDED.current_state, \
                last_check_at = EXCLUDED.last_check_at, \
                last_check_exit_code = EXCLUDED.last_check_exit_code, \
                last_check_duration_ms = EXCLUDED.last_check_duration_ms, \
                last_stdout = EXCLUDED.last_stdout, \
                updated_at = EXCLUDED.updated_at",
        )
        .bind(crate::db::bind_id(member_id))
        .bind(current_state)
        .bind(exit_code)
        .bind(duration_ms)
        .bind(stdout)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_states_for_component(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<ClusterMemberState>, sqlx::Error> {
        let sql = r#"
            SELECT cms.cluster_member_id, cms.current_state,
                   cms.last_check_at, cms.last_check_exit_code,
                   cms.last_check_duration_ms, cms.last_stdout, cms.updated_at
            FROM cluster_member_state cms
            JOIN cluster_members cm ON cm.id = cms.cluster_member_id
            WHERE cm.component_id = $1 AND cm.is_enabled = true
        "#;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<DateTime<Utc>>,
                Option<i16>,
                Option<i32>,
                Option<String>,
                DateTime<Utc>,
            ),
        >(sql)
        .bind(crate::db::bind_id(component_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ClusterMemberState {
                cluster_member_id: r.0,
                current_state: r.1,
                last_check_at: r.2,
                last_check_exit_code: r.3,
                last_check_duration_ms: r.4,
                last_stdout: r.5,
                updated_at: r.6,
            })
            .collect())
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteClusterMemberRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteClusterMemberRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn parse_json_opt(s: Option<String>) -> Option<Value> {
    s.and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn parse_json_or_default(s: Option<String>, default: Value) -> Value {
    parse_json_opt(s).unwrap_or(default)
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn json_to_string(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
const SQLITE_MEMBER_COLS: &str = "id, component_id, hostname, agent_id, site_id, \
    check_cmd_override, start_cmd_override, stop_cmd_override, install_path, \
    env_vars_override, member_order, is_enabled, tags, created_at, updated_at";

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
type SqliteMemberTuple = (
    DbUuid,
    DbUuid,
    String,
    DbUuid,
    Option<DbUuid>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i32,
    i32,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
);

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn member_from_tuple(r: SqliteMemberTuple) -> ClusterMember {
    ClusterMember {
        id: r.0.into_inner(),
        component_id: r.1.into_inner(),
        hostname: r.2,
        agent_id: r.3.into_inner(),
        site_id: r.4.map(|u| u.into_inner()),
        check_cmd_override: r.5,
        start_cmd_override: r.6,
        stop_cmd_override: r.7,
        install_path: r.8,
        env_vars_override: parse_json_opt(r.9),
        member_order: r.10,
        is_enabled: r.11 != 0,
        tags: parse_json_or_default(Some(r.12), serde_json::json!([])),
        created_at: r.13,
        updated_at: r.14,
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl ClusterMemberRepository for SqliteClusterMemberRepository {
    async fn list_by_component(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<ClusterMemberWithState>, sqlx::Error> {
        let members_sql = format!(
            "SELECT {} FROM cluster_members \
             WHERE component_id = $1 \
             ORDER BY member_order, hostname",
            SQLITE_MEMBER_COLS
        );
        let members = sqlx::query_as::<_, SqliteMemberTuple>(&members_sql)
            .bind(DbUuid::from(component_id))
            .fetch_all(&self.pool)
            .await?;

        let states: Vec<(DbUuid, String, Option<DateTime<Utc>>, Option<i16>)> = sqlx::query_as(
            "SELECT cluster_member_id, current_state, last_check_at, last_check_exit_code \
             FROM cluster_member_state cms \
             WHERE EXISTS (SELECT 1 FROM cluster_members cm WHERE cm.id = cms.cluster_member_id AND cm.component_id = $1)",
        )
        .bind(DbUuid::from(component_id))
        .fetch_all(&self.pool)
        .await?;

        let state_map: std::collections::HashMap<Uuid, _> = states
            .into_iter()
            .map(|s| (s.0.into_inner(), (s.1, s.2, s.3)))
            .collect();

        Ok(members
            .into_iter()
            .map(|t| {
                let base = member_from_tuple(t);
                let (current_state, last_check_at, last_check_exit_code) = state_map
                    .get(&base.id)
                    .cloned()
                    .unwrap_or_else(|| ("UNKNOWN".to_string(), None, None));
                ClusterMemberWithState {
                    member: base,
                    current_state,
                    last_check_at,
                    last_check_exit_code,
                }
            })
            .collect())
    }

    async fn get(&self, id: Uuid) -> Result<Option<ClusterMember>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM cluster_members WHERE id = $1",
            SQLITE_MEMBER_COLS
        );
        let row = sqlx::query_as::<_, SqliteMemberTuple>(&sql)
            .bind(DbUuid::from(id))
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(member_from_tuple))
    }

    async fn get_component_id(&self, member_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT component_id FROM cluster_members WHERE id = $1",
        )
        .bind(DbUuid::from(member_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }

    async fn create(&self, input: CreateClusterMember) -> Result<ClusterMember, sqlx::Error> {
        let new_id = DbUuid::new_v4();
        let sql = format!(
            "INSERT INTO cluster_members ( \
                id, component_id, hostname, agent_id, site_id, \
                check_cmd_override, start_cmd_override, stop_cmd_override, \
                install_path, env_vars_override, member_order, is_enabled, tags) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             RETURNING {}",
            SQLITE_MEMBER_COLS
        );
        let env_text = input.env_vars_override.as_ref().map(json_to_string);
        let tags_text = json_to_string(&input.tags);
        let row = sqlx::query_as::<_, SqliteMemberTuple>(&sql)
            .bind(new_id)
            .bind(DbUuid::from(input.component_id))
            .bind(&input.hostname)
            .bind(DbUuid::from(input.agent_id))
            .bind(input.site_id.map(DbUuid::from))
            .bind(&input.check_cmd_override)
            .bind(&input.start_cmd_override)
            .bind(&input.stop_cmd_override)
            .bind(&input.install_path)
            .bind(env_text)
            .bind(input.member_order)
            .bind(i32::from(input.is_enabled))
            .bind(tags_text)
            .fetch_one(&self.pool)
            .await?;
        Ok(member_from_tuple(row))
    }

    async fn update(
        &self,
        id: Uuid,
        input: UpdateClusterMember,
    ) -> Result<Option<ClusterMember>, sqlx::Error> {
        let existing = match self.get(id).await? {
            Some(m) => m,
            None => return Ok(None),
        };
        let hostname = input.hostname.unwrap_or_else(|| existing.hostname.clone());
        let agent_id = input.agent_id.unwrap_or(existing.agent_id);
        let site_id = input.site_id.unwrap_or(existing.site_id);
        let check = input
            .check_cmd_override
            .unwrap_or_else(|| existing.check_cmd_override.clone());
        let start = input
            .start_cmd_override
            .unwrap_or_else(|| existing.start_cmd_override.clone());
        let stop = input
            .stop_cmd_override
            .unwrap_or_else(|| existing.stop_cmd_override.clone());
        let install = input
            .install_path
            .unwrap_or_else(|| existing.install_path.clone());
        let env = input
            .env_vars_override
            .unwrap_or_else(|| existing.env_vars_override.clone());
        let order = input.member_order.unwrap_or(existing.member_order);
        let enabled = input.is_enabled.unwrap_or(existing.is_enabled);
        let tags = input.tags.unwrap_or_else(|| existing.tags.clone());

        let sql = format!(
            "UPDATE cluster_members SET \
                hostname = $2, agent_id = $3, site_id = $4, \
                check_cmd_override = $5, start_cmd_override = $6, stop_cmd_override = $7, \
                install_path = $8, env_vars_override = $9, \
                member_order = $10, is_enabled = $11, tags = $12, \
                updated_at = datetime('now') \
             WHERE id = $1 \
             RETURNING {}",
            SQLITE_MEMBER_COLS
        );
        let env_text = env.as_ref().map(json_to_string);
        let tags_text = json_to_string(&tags);
        let row = sqlx::query_as::<_, SqliteMemberTuple>(&sql)
            .bind(DbUuid::from(id))
            .bind(&hostname)
            .bind(DbUuid::from(agent_id))
            .bind(site_id.map(DbUuid::from))
            .bind(&check)
            .bind(&start)
            .bind(&stop)
            .bind(&install)
            .bind(env_text)
            .bind(order)
            .bind(i32::from(enabled))
            .bind(tags_text)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(member_from_tuple))
    }

    async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM cluster_members WHERE id = $1")
            .bind(DbUuid::from(id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn load_for_agent(&self, agent_id: Uuid) -> Result<Vec<AgentMemberConfig>, sqlx::Error> {
        let sql = r#"
            SELECT cm.id, c.id, cm.hostname,
                   COALESCE(cm.check_cmd_override, c.check_cmd) AS check_cmd,
                   COALESCE(cm.start_cmd_override, c.start_cmd) AS start_cmd,
                   COALESCE(cm.stop_cmd_override, c.stop_cmd) AS stop_cmd,
                   COALESCE(cm.env_vars_override, c.env_vars, '{}') AS env_vars
            FROM cluster_members cm
            JOIN components c ON c.id = cm.component_id
            JOIN applications a ON a.id = c.application_id
            WHERE cm.agent_id = $1
              AND cm.is_enabled = 1
              AND c.cluster_mode = 'fan_out'
              AND a.is_suspended = 0
        "#;
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                DbUuid,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
            ),
        >(sql)
        .bind(DbUuid::from(agent_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AgentMemberConfig {
                member_id: r.0.into_inner(),
                component_id: r.1.into_inner(),
                hostname: r.2,
                check_cmd: r.3,
                start_cmd: r.4,
                stop_cmd: r.5,
                env_vars: parse_json_or_default(Some(r.6), serde_json::json!({})),
            })
            .collect())
    }

    async fn upsert_state(
        &self,
        member_id: Uuid,
        current_state: &str,
        exit_code: i16,
        duration_ms: i32,
        stdout: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO cluster_member_state ( \
                cluster_member_id, current_state, last_check_at, \
                last_check_exit_code, last_check_duration_ms, last_stdout, updated_at) \
             VALUES ($1, $2, datetime('now'), $3, $4, $5, datetime('now')) \
             ON CONFLICT(cluster_member_id) DO UPDATE SET \
                current_state = excluded.current_state, \
                last_check_at = excluded.last_check_at, \
                last_check_exit_code = excluded.last_check_exit_code, \
                last_check_duration_ms = excluded.last_check_duration_ms, \
                last_stdout = excluded.last_stdout, \
                updated_at = excluded.updated_at",
        )
        .bind(DbUuid::from(member_id))
        .bind(current_state)
        .bind(exit_code)
        .bind(duration_ms)
        .bind(stdout)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_states_for_component(
        &self,
        component_id: Uuid,
    ) -> Result<Vec<ClusterMemberState>, sqlx::Error> {
        let sql = r#"
            SELECT cms.cluster_member_id, cms.current_state,
                   cms.last_check_at, cms.last_check_exit_code,
                   cms.last_check_duration_ms, cms.last_stdout, cms.updated_at
            FROM cluster_member_state cms
            JOIN cluster_members cm ON cm.id = cms.cluster_member_id
            WHERE cm.component_id = $1 AND cm.is_enabled = 1
        "#;
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                Option<DateTime<Utc>>,
                Option<i16>,
                Option<i32>,
                Option<String>,
                DateTime<Utc>,
            ),
        >(sql)
        .bind(DbUuid::from(component_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ClusterMemberState {
                cluster_member_id: r.0.into_inner(),
                current_state: r.1,
                last_check_at: r.2,
                last_check_exit_code: r.3,
                last_check_duration_ms: r.4,
                last_stdout: r.5,
                updated_at: r.6,
            })
            .collect())
    }
}

// ============================================================================
// Factory
// ============================================================================

pub fn create_cluster_member_repository(pool: DbPool) -> Box<dyn ClusterMemberRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgClusterMemberRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteClusterMemberRepository::new(pool))
    }
}

// ============================================================================
// FSM aggregation
// ============================================================================

/// Derive the parent component's state from a snapshot of member states.
///
/// Policies:
/// - `all_healthy`: all RUNNING → RUNNING; any non-RUNNING and ≥1 RUNNING → DEGRADED;
///   0 RUNNING → FAILED (or STOPPED if all STOPPED).
/// - `any_healthy`: any RUNNING → RUNNING; else FAILED.
/// - `quorum`: count(RUNNING) ≥ ⌈N/2⌉+1 → RUNNING; ≥1 → DEGRADED; 0 → FAILED.
/// - `threshold_pct`: %RUNNING ≥ `min_healthy_pct` → RUNNING; ≥50% → DEGRADED;
///   <50% → FAILED.
///
/// Returns `None` for empty/disabled clusters (caller keeps the component's
/// manually-set state, e.g. STOPPED before first member is added).
pub fn derive_component_state(
    member_states: &[ClusterMemberState],
    policy: &str,
    min_healthy_pct: i16,
) -> Option<&'static str> {
    let total = member_states.len();
    if total == 0 {
        return None;
    }
    let running = member_states
        .iter()
        .filter(|s| s.current_state == "RUNNING")
        .count();
    let stopped = member_states
        .iter()
        .filter(|s| s.current_state == "STOPPED")
        .count();

    if stopped == total {
        return Some("STOPPED");
    }

    match policy {
        "any_healthy" => {
            if running > 0 {
                Some("RUNNING")
            } else {
                Some("FAILED")
            }
        }
        "quorum" => {
            let quorum = total / 2 + 1;
            if running >= quorum {
                Some("RUNNING")
            } else if running > 0 {
                Some("DEGRADED")
            } else {
                Some("FAILED")
            }
        }
        "threshold_pct" => {
            let pct = (running as f64 / total as f64) * 100.0;
            if pct >= min_healthy_pct as f64 {
                Some("RUNNING")
            } else if pct >= 50.0 {
                Some("DEGRADED")
            } else if running == 0 {
                Some("FAILED")
            } else {
                Some("DEGRADED")
            }
        }
        _ => {
            // all_healthy (default)
            if running == total {
                Some("RUNNING")
            } else if running == 0 {
                Some("FAILED")
            } else {
                Some("DEGRADED")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(s: &str) -> ClusterMemberState {
        ClusterMemberState {
            cluster_member_id: Uuid::new_v4(),
            current_state: s.to_string(),
            last_check_at: None,
            last_check_exit_code: None,
            last_check_duration_ms: None,
            last_stdout: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn all_healthy_all_running() {
        let states = vec![make_state("RUNNING"), make_state("RUNNING")];
        assert_eq!(
            derive_component_state(&states, "all_healthy", 100),
            Some("RUNNING")
        );
    }

    #[test]
    fn all_healthy_partial_running_is_degraded() {
        let states = vec![make_state("RUNNING"), make_state("FAILED")];
        assert_eq!(
            derive_component_state(&states, "all_healthy", 100),
            Some("DEGRADED")
        );
    }

    #[test]
    fn all_healthy_none_running_is_failed() {
        let states = vec![make_state("FAILED"), make_state("FAILED")];
        assert_eq!(
            derive_component_state(&states, "all_healthy", 100),
            Some("FAILED")
        );
    }

    #[test]
    fn all_stopped_is_stopped() {
        let states = vec![make_state("STOPPED"), make_state("STOPPED")];
        assert_eq!(
            derive_component_state(&states, "all_healthy", 100),
            Some("STOPPED")
        );
    }

    #[test]
    fn any_healthy_one_running_is_running() {
        let states = vec![make_state("RUNNING"), make_state("FAILED")];
        assert_eq!(
            derive_component_state(&states, "any_healthy", 100),
            Some("RUNNING")
        );
    }

    #[test]
    fn quorum_majority_running_is_running() {
        let states = vec![
            make_state("RUNNING"),
            make_state("RUNNING"),
            make_state("FAILED"),
        ];
        assert_eq!(
            derive_component_state(&states, "quorum", 100),
            Some("RUNNING")
        );
    }

    #[test]
    fn quorum_split_is_degraded() {
        let states = vec![
            make_state("RUNNING"),
            make_state("FAILED"),
            make_state("FAILED"),
        ];
        assert_eq!(
            derive_component_state(&states, "quorum", 100),
            Some("DEGRADED")
        );
    }

    #[test]
    fn threshold_pct_meets_threshold() {
        let states = vec![
            make_state("RUNNING"),
            make_state("RUNNING"),
            make_state("RUNNING"),
            make_state("FAILED"),
        ];
        // 75% running, threshold 75 → RUNNING
        assert_eq!(
            derive_component_state(&states, "threshold_pct", 75),
            Some("RUNNING")
        );
    }

    #[test]
    fn threshold_pct_below_threshold_above_50_degraded() {
        let states = vec![
            make_state("RUNNING"),
            make_state("RUNNING"),
            make_state("FAILED"),
            make_state("FAILED"),
        ];
        // 50% — below threshold 80 but ≥ 50% → DEGRADED
        assert_eq!(
            derive_component_state(&states, "threshold_pct", 80),
            Some("DEGRADED")
        );
    }

    #[test]
    fn empty_members_returns_none() {
        assert_eq!(derive_component_state(&[], "all_healthy", 100), None);
    }
}
