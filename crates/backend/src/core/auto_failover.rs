//! Auto-Failover Background Task
//!
//! Monitors agent health for profiles with auto_failover enabled.
//! When the active profile's agents become unreachable, automatically
//! activates the DR profile.

use crate::db::{DbJson, DbPool, DbUuid};
use chrono::{Duration, Utc};
use serde_json::json;
use std::sync::Arc;
use tokio::time;
use uuid::Uuid;

use crate::repository::core_queries;
use crate::websocket::Hub;

/// Configuration for the auto-failover monitor
pub struct AutoFailoverConfig {
    pub check_interval_secs: u64,
    pub unreachable_threshold_secs: i64,
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
    let candidates = core_queries::get_failover_candidates(pool).await?;

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

    // Get all agents for the active profile
    let agents = core_queries::get_profile_agents(pool, *active_profile_id).await?;

    if agents.is_empty() {
        return Ok(());
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

        core_queries::upsert_failover_health(pool, active_profile_id, agent.agent_id, is_reachable, now)
            .await?;
    }

    // Check if failover should be triggered
    if unreachable_ratio >= config.unreachable_percentage {
        let all_unreachable_long_enough =
            core_queries::check_unreachable_duration(pool, *active_profile_id, threshold).await?;

        if all_unreachable_long_enough {
            tracing::warn!(
                "Auto-failover triggered for app {} ({}): {}% agents unreachable ({:?})",
                app_name,
                app_id,
                (unreachable_ratio * 100.0) as u32,
                unreachable_agents
            );

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

    // Log to action_log
    let system_user_id = DbUuid::nil();
    let failover_details = DbJson::from(json!({
        "switchover_id": switchover_id,
        "trigger": "auto_failover",
        "from_profile": active_profile_name,
        "to_profile": dr_profile_name,
        "unreachable_agents": unreachable_agents
    }));

    core_queries::log_auto_failover_action(pool, system_user_id, *app_id, failover_details).await?;

    // Deactivate current profile
    core_queries::deactivate_profile(pool, *active_profile_id).await?;

    // Activate DR profile
    core_queries::activate_profile(pool, *dr_profile_id).await?;

    // Update component agent_ids based on DR profile mappings
    core_queries::apply_profile_mappings(pool, *app_id, *dr_profile_id).await?;

    // Log to switchover_log
    core_queries::log_switchover_event(
        pool,
        switchover_id,
        *app_id,
        "COMMIT",
        "completed",
        DbJson::from(json!({
            "type": "auto_failover",
            "from_profile": active_profile_name,
            "to_profile": dr_profile_name,
            "unreachable_agents": unreachable_agents
        })),
    )
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
