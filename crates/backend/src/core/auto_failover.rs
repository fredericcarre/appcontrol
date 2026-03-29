//! Auto-Failover Background Task
//!
//! Monitors agent health for profiles with auto_failover enabled.
//! When the active profile's agents become unreachable, automatically
//! activates the DR profile.
//!
//! Check interval: 30 seconds
//! Failover threshold: >50% agents unreachable for >2 minutes

use crate::db::{DbJson, DbPool, DbUuid};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::FromRow;
use std::sync::Arc;
use tokio::time;
use uuid::Uuid;

use crate::websocket::Hub;

/// Configuration for the auto-failover monitor
pub struct AutoFailoverConfig {
    /// How often to check agent health (default: 30 seconds)
    pub check_interval_secs: u64,
    /// Time agents must be unreachable before triggering failover (default: 2 minutes)
    pub unreachable_threshold_secs: i64,
    /// Percentage of agents that must be unreachable to trigger failover (default: 50%)
    pub unreachable_percentage: f32,
}

impl Default for AutoFailoverConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 30,
            unreachable_threshold_secs: 120,
            unreachable_percentage: 0.5,
        }
    }
}

/// Spawn the auto-failover background task
pub fn spawn_auto_failover_task(pool: Arc<DbPool>, ws_hub: Arc<Hub>, config: AutoFailoverConfig) {
    tokio::spawn(async move {
        tracing::info!(
            "Auto-failover monitor started (interval: {}s)",
            config.check_interval_secs
        );

        let mut interval = time::interval(time::Duration::from_secs(config.check_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = check_and_failover(&pool, &ws_hub, &config).await {
                tracing::error!("Auto-failover check failed: {}", e);
            }
        }
    });
}

/// Check all applications with auto-failover enabled and trigger failover if needed
async fn check_and_failover(
    pool: &DbPool,
    ws_hub: &Hub,
    config: &AutoFailoverConfig,
) -> Result<(), sqlx::Error> {
    // Find all applications with an active profile and a DR profile with auto_failover enabled
    #[derive(Debug, FromRow)]
    struct FailoverCandidate {
        application_id: DbUuid,
        application_name: String,
        active_profile_id: DbUuid,
        active_profile_name: String,
        dr_profile_id: DbUuid,
        dr_profile_name: String,
    }

    #[cfg(feature = "postgres")]
    let candidates_sql: &str = r#"
        SELECT
            app.id as application_id,
            app.name as application_name,
            active.id as active_profile_id,
            active.name as active_profile_name,
            dr.id as dr_profile_id,
            dr.name as dr_profile_name
        FROM applications app
        JOIN binding_profiles active ON active.application_id = app.id AND active.is_active = true
        JOIN binding_profiles dr ON dr.application_id = app.id
            AND dr.profile_type = 'dr'
            AND dr.auto_failover = true
            AND dr.is_active = false
        WHERE active.profile_type = 'primary'
        "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let candidates_sql: &str = r#"
        SELECT
            app.id as application_id,
            app.name as application_name,
            active.id as active_profile_id,
            active.name as active_profile_name,
            dr.id as dr_profile_id,
            dr.name as dr_profile_name
        FROM applications app
        JOIN binding_profiles active ON active.application_id = app.id AND active.is_active = 1
        JOIN binding_profiles dr ON dr.application_id = app.id
            AND dr.profile_type = 'dr'
            AND dr.auto_failover = 1
            AND dr.is_active = 0
        WHERE active.profile_type = 'primary'
        "#;

    let candidates: Vec<FailoverCandidate> = sqlx::query_as(candidates_sql)
    .fetch_all(pool)
    .await?;

    for candidate in candidates {
        if let Err(e) = check_profile_health_and_failover(
            pool,
            ws_hub,
            config,
            &candidate.application_id,
            &candidate.application_name,
            &candidate.active_profile_id,
            &candidate.active_profile_name,
            &candidate.dr_profile_id,
            &candidate.dr_profile_name,
        )
        .await
        {
            tracing::warn!(
                "Failed to check health for app {} ({}): {}",
                candidate.application_name,
                candidate.application_id,
                e
            );
        }
    }

    Ok(())
}

/// Check health of a specific profile's agents and trigger failover if needed
#[allow(clippy::too_many_arguments)]
async fn check_profile_health_and_failover(
    pool: &DbPool,
    ws_hub: &Hub,
    config: &AutoFailoverConfig,
    app_id: &Uuid,
    app_name: &str,
    active_profile_id: &Uuid,
    active_profile_name: &str,
    dr_profile_id: &Uuid,
    dr_profile_name: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    let threshold = now - Duration::seconds(config.unreachable_threshold_secs);

    // Get all agents for the active profile and their health status
    #[derive(Debug, FromRow)]
    struct AgentHealth {
        agent_id: DbUuid,
        agent_hostname: String,
        last_heartbeat_at: Option<DateTime<Utc>>,
        is_active: bool,
    }

    let agents: Vec<AgentHealth> = sqlx::query_as(
        r#"
        SELECT DISTINCT
            a.id as agent_id,
            a.hostname as agent_hostname,
            a.last_heartbeat_at,
            a.is_active
        FROM binding_profile_mappings m
        JOIN agents a ON a.id = m.agent_id
        WHERE m.profile_id = $1
        "#,
    )
    .bind(DbUuid::from(*active_profile_id))
    .fetch_all(pool)
    .await?;

    if agents.is_empty() {
        return Ok(()); // No agents to monitor
    }

    // Count reachable vs unreachable agents
    let mut unreachable_count = 0;
    let mut unreachable_agents: Vec<String> = Vec::new();

    for agent in &agents {
        let is_unreachable =
            !agent.is_active || agent.last_heartbeat_at.is_none_or(|hb| hb < threshold);

        if is_unreachable {
            unreachable_count += 1;
            unreachable_agents.push(agent.agent_hostname.clone());
        }
    }

    let unreachable_ratio = unreachable_count as f32 / agents.len() as f32;

    // Update health tracking table
    for agent in &agents {
        let is_reachable =
            agent.is_active && agent.last_heartbeat_at.is_some_and(|hb| hb >= threshold);

        sqlx::query(
            r#"
            INSERT INTO failover_health_status (profile_id, agent_id, is_reachable, last_check_at, unreachable_since)
            VALUES ($1, $2, $3, $4, CASE WHEN $3 THEN NULL ELSE COALESCE(
                (SELECT unreachable_since FROM failover_health_status WHERE profile_id = $1 AND agent_id = $2),
                $4
            ) END)
            ON CONFLICT (profile_id, agent_id) DO UPDATE SET
                is_reachable = EXCLUDED.is_reachable,
                last_check_at = EXCLUDED.last_check_at,
                unreachable_since = CASE WHEN EXCLUDED.is_reachable THEN NULL ELSE COALESCE(failover_health_status.unreachable_since, EXCLUDED.last_check_at) END
            "#
        )
        .bind(active_profile_id)
        .bind(agent.agent_id)
        .bind(is_reachable)
        .bind(now)
        .execute(pool)
        .await?;
    }

    // Check if failover should be triggered
    if unreachable_ratio >= config.unreachable_percentage {
        // Verify agents have been unreachable for the required duration
        #[cfg(feature = "postgres")]
        let unreachable_sql: &str = r#"
            SELECT COUNT(*) = 0
            FROM failover_health_status
            WHERE profile_id = $1
              AND is_reachable = false
              AND unreachable_since > $2
            "#;
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let unreachable_sql: &str = r#"
            SELECT COUNT(*) = 0
            FROM failover_health_status
            WHERE profile_id = $1
              AND is_reachable = 0
              AND unreachable_since > $2
            "#;

        let all_unreachable_long_enough: bool = sqlx::query_scalar(unreachable_sql)
        .bind(DbUuid::from(*active_profile_id))
        .bind(threshold)
        .fetch_one(pool)
        .await?;

        if all_unreachable_long_enough {
            tracing::warn!(
                "Auto-failover triggered for app {} ({}): {}% agents unreachable ({:?})",
                app_name,
                app_id,
                (unreachable_ratio * 100.0) as u32,
                unreachable_agents
            );

            // Execute failover
            trigger_auto_failover(
                pool,
                ws_hub,
                app_id,
                app_name,
                active_profile_id,
                active_profile_name,
                dr_profile_id,
                dr_profile_name,
                &unreachable_agents,
            )
            .await?;
        }
    }

    Ok(())
}

/// Trigger automatic failover from active profile to DR profile
#[allow(clippy::too_many_arguments)]
async fn trigger_auto_failover(
    pool: &DbPool,
    ws_hub: &Hub,
    app_id: &Uuid,
    app_name: &str,
    active_profile_id: &Uuid,
    active_profile_name: &str,
    dr_profile_id: &Uuid,
    dr_profile_name: &str,
    unreachable_agents: &[String],
) -> Result<(), sqlx::Error> {
    let switchover_id = Uuid::new_v4();

    // Log to action_log (use system user ID placeholder)
    let system_user_id = DbUuid::nil(); // System-initiated action
    sqlx::query(
        r#"
        INSERT INTO action_log (user_id, action, resource_type, resource_id, details)
        VALUES ($1, 'auto_failover', 'application', $2, $3)
        "#,
    )
    .bind(system_user_id)
    .bind(DbUuid::from(*app_id))
    .bind(DbJson::from(json!({
        "switchover_id": switchover_id,
        "trigger": "auto_failover",
        "from_profile": active_profile_name,
        "to_profile": dr_profile_name,
        "unreachable_agents": unreachable_agents
    })))
    .execute(pool)
    .await?;

    // Deactivate current profile
    #[cfg(feature = "postgres")]
    let deact_sql: &str = "UPDATE binding_profiles SET is_active = false WHERE id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let deact_sql: &str = "UPDATE binding_profiles SET is_active = 0 WHERE id = $1";

    sqlx::query(deact_sql)
        .bind(DbUuid::from(*active_profile_id))
        .execute(pool)
        .await?;

    // Activate DR profile
    #[cfg(feature = "postgres")]
    let act_sql: &str = "UPDATE binding_profiles SET is_active = true WHERE id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let act_sql: &str = "UPDATE binding_profiles SET is_active = 1 WHERE id = $1";

    sqlx::query(act_sql)
        .bind(DbUuid::from(*dr_profile_id))
        .execute(pool)
        .await?;

    // Update component agent_ids based on DR profile mappings
    #[cfg(feature = "postgres")]
    {
        sqlx::query(
            r#"
            UPDATE components c
            SET agent_id = m.agent_id
            FROM binding_profile_mappings m
            WHERE c.application_id = $1
              AND m.profile_id = $2
              AND c.name = m.component_name
            "#,
        )
        .bind(DbUuid::from(*app_id))
        .bind(DbUuid::from(*dr_profile_id))
        .execute(pool)
        .await?;
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        // SQLite doesn't support UPDATE ... FROM, so use a subquery
        sqlx::query(
            r#"
            UPDATE components
            SET agent_id = (
                SELECT m.agent_id
                FROM binding_profile_mappings m
                WHERE m.profile_id = $2
                  AND m.component_name = components.name
            )
            WHERE application_id = $1
              AND EXISTS (
                SELECT 1 FROM binding_profile_mappings m
                WHERE m.profile_id = $2
                  AND m.component_name = components.name
              )
            "#,
        )
        .bind(DbUuid::from(*app_id))
        .bind(DbUuid::from(*dr_profile_id))
        .execute(pool)
        .await?;
    }

    // Log to switchover_log
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, 'COMMIT', 'completed', $4)
        "#,
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(switchover_id))
    .bind(DbUuid::from(*app_id))
    .bind(DbJson::from(json!({
        "type": "auto_failover",
        "from_profile": active_profile_name,
        "to_profile": dr_profile_name,
        "unreachable_agents": unreachable_agents
    })))
    .execute(pool)
    .await?;

    // Broadcast WebSocket event
    ws_hub.broadcast(
        *app_id,
        appcontrol_common::WsEvent::AutoFailover {
            app_id: *app_id,
            switchover_id,
            from_profile: active_profile_name.to_string(),
            to_profile: dr_profile_name.to_string(),
            unreachable_agents: unreachable_agents.to_vec(),
            timestamp: Utc::now(),
        },
    );

    tracing::info!(
        "Auto-failover completed for app {} ({}): {} -> {}",
        app_name,
        app_id,
        active_profile_name,
        dr_profile_name
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AutoFailoverConfig::default();
        assert_eq!(config.check_interval_secs, 30);
        assert_eq!(config.unreachable_threshold_secs, 120);
        assert!((config.unreachable_percentage - 0.5).abs() < f32::EPSILON);
    }
}
