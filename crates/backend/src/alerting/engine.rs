//! Alerting engine: evaluates policies against FSM transitions, manages
//! the firing / resolved lifecycle, and dispatches notifications.
//!
//! The engine is invoked from `core::fsm::transition_component` after the
//! `state_transitions` row is committed; it spawns its own task so the
//! FSM hot path never blocks on outbound HTTP.
//!
//! Postgres-only for this MVP — the SQLite mirror lands in the next sprint
//! once the schema stabilises.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use appcontrol_common::alerting::{
    AlertNotificationPayload, AlertSelector, AlertSeverity, AlertStatus, NotificationChannelConfig,
};
use appcontrol_common::ComponentState;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::channels::{build_channel, build_http_client};
use super::AlertingError;
use crate::db::DbPool;

/// Hook called from the FSM after a state transition has been committed.
/// Spawns the policy evaluation on its own task so the FSM never blocks
/// on alert dispatch. Failures are logged, never propagated.
///
/// `org_id` is looked up from `app_id` inside the spawned task so the
/// caller (the FSM, which already has `app_id` in hand) doesn't need to
/// thread it through.
pub fn notify_state_transition(
    pool: DbPool,
    app_id: Uuid,
    app_name: String,
    component_id: Uuid,
    component_name: String,
    from: ComponentState,
    to: ComponentState,
) {
    tokio::spawn(async move {
        let org_id = match lookup_org_id(&pool, app_id).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::debug!(app_id = %app_id, "alerting: app not found, skipping");
                return;
            }
            Err(e) => {
                tracing::warn!(app_id = %app_id, error = %e, "alerting: org_id lookup failed");
                return;
            }
        };
        let ctx = TransitionContext {
            org_id,
            app_id,
            app_name,
            component_id,
            component_name,
            from,
            to,
            at: Utc::now(),
        };
        if let Err(e) = evaluate_transition(&pool, &ctx).await {
            tracing::warn!(
                org_id = %org_id,
                component_id = %component_id,
                error = %e,
                "alerting engine: evaluation failed",
            );
        }
    });
}

#[cfg(feature = "postgres")]
async fn lookup_org_id(pool: &DbPool, app_id: Uuid) -> Result<Option<Uuid>, AlertingError> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT org_id FROM applications WHERE id = $1")
        .bind(app_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id,)| id))
}

#[cfg(not(feature = "postgres"))]
async fn lookup_org_id(_pool: &DbPool, _app_id: Uuid) -> Result<Option<Uuid>, AlertingError> {
    Ok(None)
}

#[derive(Debug, Clone)]
pub(crate) struct TransitionContext {
    pub org_id: Uuid,
    pub app_id: Uuid,
    pub app_name: String,
    pub component_id: Uuid,
    pub component_name: String,
    #[allow(dead_code)]
    pub from: ComponentState,
    pub to: ComponentState,
    pub at: DateTime<Utc>,
}

/// Compact view of a row from `alert_policies` after JSON columns are
/// parsed. Public to the module so tests can construct fixtures.
#[derive(Debug, Clone)]
pub(crate) struct Policy {
    pub id: Uuid,
    pub name: String,
    pub selector: AlertSelector,
    pub trigger_states: Vec<ComponentState>,
    pub sustain_seconds: i32,
    pub severity: AlertSeverity,
    pub cooldown_seconds: i32,
    pub channel_ids: Vec<Uuid>,
}

#[cfg(feature = "postgres")]
async fn evaluate_transition(pool: &DbPool, ctx: &TransitionContext) -> Result<(), AlertingError> {
    // 1. Load enabled policies for this org.
    let policies = load_policies(pool, ctx.org_id).await?;

    // 2. Load component tags so tag-based selectors can match.
    let component_tags = load_component_tags(pool, ctx.component_id).await?;

    // 3. For every policy, decide what to do.
    for p in &policies {
        if !selector_matches(&p.selector, ctx, &component_tags) {
            continue;
        }

        let triggers = p.trigger_states.contains(&ctx.to);
        if triggers {
            if !cooldown_clear(pool, p, ctx).await? {
                continue;
            }
            if !sustain_met(pool, p, ctx).await? {
                continue;
            }
            open_alert(pool, p, ctx).await?;
        } else {
            // Component left a trigger state → resolve any open alert
            // for this (policy, component).
            resolve_open_alerts(pool, p, ctx).await?;
        }
    }
    Ok(())
}

#[cfg(not(feature = "postgres"))]
async fn evaluate_transition(
    _pool: &DbPool,
    _ctx: &TransitionContext,
) -> Result<(), AlertingError> {
    // SQLite path is implemented in a follow-up sprint. The schema
    // already exists (sqlite/V057), only the queries are pending.
    Ok(())
}

// ---------------------------------------------------------------------------
// Selector matching (pure, fully unit-testable)
// ---------------------------------------------------------------------------

/// Decide whether `selector` matches the transition described by `ctx`.
/// Pure function — depends only on inputs, so the unit tests can cover
/// every selector field combination without a database.
pub(crate) fn selector_matches(
    selector: &AlertSelector,
    ctx: &TransitionContext,
    component_tags: &HashMap<String, String>,
) -> bool {
    if let Some(app_id) = selector.app_id {
        if app_id != ctx.app_id {
            return false;
        }
    }
    if let Some(comp_id) = selector.component_id {
        if comp_id != ctx.component_id {
            return false;
        }
    }
    for (k, v) in &selector.tags {
        match component_tags.get(k) {
            Some(actual) if actual == v => {}
            _ => return false,
        }
    }
    true
}

fn fingerprint(policy_id: Uuid, component_id: Uuid) -> String {
    let mut h = Sha256::new();
    h.update(policy_id.as_bytes());
    h.update(b":");
    h.update(component_id.as_bytes());
    hex::encode(h.finalize())
}

// ---------------------------------------------------------------------------
// Postgres queries
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
async fn load_policies(pool: &DbPool, org_id: Uuid) -> Result<Vec<Policy>, AlertingError> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"SELECT id, name, selector, trigger_states, sustain_seconds, severity,
                  cooldown_seconds, channel_ids
           FROM alert_policies
           WHERE org_id = $1 AND enabled = TRUE"#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let mut policies = Vec::with_capacity(rows.len());
    for r in rows {
        let id: Uuid = r.try_get("id")?;
        let name: String = r.try_get("name")?;
        let selector_json: Value = r.try_get("selector")?;
        let trigger_states_raw: Vec<String> = r.try_get("trigger_states")?;
        let sustain_seconds: i32 = r.try_get("sustain_seconds")?;
        let severity_raw: String = r.try_get("severity")?;
        let cooldown_seconds: i32 = r.try_get("cooldown_seconds")?;
        let channel_ids: Vec<Uuid> = r.try_get("channel_ids")?;

        let selector: AlertSelector = serde_json::from_value(selector_json)?;
        let trigger_states = trigger_states_raw
            .into_iter()
            .filter_map(|s| ComponentState::from_str(&s).ok())
            .collect();
        let severity = AlertSeverity::from_str(&severity_raw)
            .map_err(|e| AlertingError::Config(format!("bad severity '{severity_raw}': {e}")))?;

        policies.push(Policy {
            id,
            name,
            selector,
            trigger_states,
            sustain_seconds,
            severity,
            cooldown_seconds,
            channel_ids,
        });
    }
    Ok(policies)
}

#[cfg(feature = "postgres")]
async fn load_component_tags(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<HashMap<String, String>, AlertingError> {
    let row: Option<(Value,)> = sqlx::query_as("SELECT tags FROM components WHERE id = $1")
        .bind(component_id)
        .fetch_optional(pool)
        .await?;
    let Some((tags,)) = row else {
        return Ok(HashMap::new());
    };
    // tags is JSONB; expected shape is an object of string→string. Any other
    // shape simply yields an empty map (selector matching will fail on tag
    // policies, which is the safe default).
    if let Value::Object(map) = tags {
        Ok(map
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect())
    } else {
        Ok(HashMap::new())
    }
}

#[cfg(feature = "postgres")]
async fn cooldown_clear(
    pool: &DbPool,
    p: &Policy,
    ctx: &TransitionContext,
) -> Result<bool, AlertingError> {
    if p.cooldown_seconds <= 0 {
        return Ok(true);
    }
    let fp = fingerprint(p.id, ctx.component_id);
    let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT fired_at FROM alert_instances
           WHERE fingerprint = $1
           ORDER BY fired_at DESC LIMIT 1",
    )
    .bind(&fp)
    .fetch_optional(pool)
    .await?;
    let Some((last,)) = row else {
        return Ok(true);
    };
    let elapsed = ctx.at.signed_duration_since(last);
    Ok(elapsed.num_seconds() >= p.cooldown_seconds as i64)
}

#[cfg(feature = "postgres")]
async fn sustain_met(
    pool: &DbPool,
    p: &Policy,
    ctx: &TransitionContext,
) -> Result<bool, AlertingError> {
    if p.sustain_seconds <= 0 {
        return Ok(true);
    }
    // The component must have been in a trigger state for at least
    // sustain_seconds. Use state_transitions: find the latest transition
    // INTO any trigger state; if it's old enough, fire.
    let trigger_strs: Vec<String> = p.trigger_states.iter().map(|s| s.to_string()).collect();
    let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT created_at FROM state_transitions
           WHERE component_id = $1 AND to_state = ANY($2)
           ORDER BY created_at DESC LIMIT 1",
    )
    .bind(ctx.component_id)
    .bind(&trigger_strs)
    .fetch_optional(pool)
    .await?;
    let Some((entered_at,)) = row else {
        return Ok(false);
    };
    let held = ctx.at.signed_duration_since(entered_at);
    Ok(held.num_seconds() >= p.sustain_seconds as i64)
}

#[cfg(feature = "postgres")]
async fn open_alert(
    pool: &DbPool,
    p: &Policy,
    ctx: &TransitionContext,
) -> Result<(), AlertingError> {
    let fp = fingerprint(p.id, ctx.component_id);
    let alert_id = Uuid::new_v4();
    let summary = format!(
        "{} transitioned to {} (policy '{}')",
        ctx.component_name, ctx.to, p.name
    );

    // ON CONFLICT on the partial unique index — if a firing/acknowledged
    // instance already exists for this fingerprint, we keep it and don't
    // double-fire.
    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"INSERT INTO alert_instances
            (id, org_id, policy_id, component_id, fingerprint, severity,
             status, triggered_state, summary)
           VALUES ($1, $2, $3, $4, $5, $6, 'firing', $7, $8)
           ON CONFLICT (fingerprint) WHERE status IN ('firing','acknowledged')
           DO NOTHING
           RETURNING id"#,
    )
    .bind(alert_id)
    .bind(ctx.org_id)
    .bind(p.id)
    .bind(ctx.component_id)
    .bind(&fp)
    .bind(format!("{}", p.severity))
    .bind(ctx.to.to_string())
    .bind(&summary)
    .fetch_optional(pool)
    .await?;

    let actually_inserted = row.is_some();
    if !actually_inserted {
        return Ok(());
    }

    // Dispatch to every channel attached to the policy. Failures are
    // logged into the instance's notifications_sent array so operators
    // can see which channel didn't deliver.
    let payload = AlertNotificationPayload {
        alert_id,
        policy_id: p.id,
        policy_name: p.name.clone(),
        component_id: ctx.component_id,
        component_name: ctx.component_name.clone(),
        app_id: ctx.app_id,
        app_name: ctx.app_name.clone(),
        severity: p.severity,
        status: AlertStatus::Firing,
        triggered_state: ctx.to.to_string(),
        fired_at: ctx.at,
        summary: Some(summary),
    };

    if !p.channel_ids.is_empty() {
        dispatch_to_channels(pool, alert_id, &p.channel_ids, &payload).await?;
    }
    Ok(())
}

#[cfg(feature = "postgres")]
async fn resolve_open_alerts(
    pool: &DbPool,
    p: &Policy,
    ctx: &TransitionContext,
) -> Result<(), AlertingError> {
    let fp = fingerprint(p.id, ctx.component_id);
    sqlx::query(
        "UPDATE alert_instances
            SET status = 'resolved', resolved_at = $2
          WHERE fingerprint = $1 AND status IN ('firing','acknowledged')",
    )
    .bind(&fp)
    .bind(ctx.at)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn dispatch_to_channels(
    pool: &DbPool,
    alert_id: Uuid,
    channel_ids: &[Uuid],
    payload: &AlertNotificationPayload,
) -> Result<(), AlertingError> {
    let client = build_http_client()?;

    let rows: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, config FROM notification_channels
          WHERE id = ANY($1) AND enabled = TRUE",
    )
    .bind(channel_ids)
    .fetch_all(pool)
    .await?;

    let mut log: Vec<Value> = Vec::with_capacity(rows.len());

    for (channel_id, config_json) in rows {
        let now = Utc::now();
        let entry = match serde_json::from_value::<NotificationChannelConfig>(config_json) {
            Ok(cfg) => {
                let channel = build_channel(&cfg);
                match channel.dispatch(&client, payload).await {
                    Ok(()) => json!({"channel_id": channel_id, "at": now, "ok": true}),
                    Err(e) => {
                        tracing::warn!(channel_id = %channel_id, error = %e, "alerting: dispatch failed");
                        json!({"channel_id": channel_id, "at": now, "ok": false, "error": e.to_string()})
                    }
                }
            }
            Err(e) => {
                tracing::warn!(channel_id = %channel_id, error = %e, "alerting: invalid channel config");
                json!({"channel_id": channel_id, "at": now, "ok": false, "error": format!("bad config: {e}")})
            }
        };
        log.push(entry);
    }

    sqlx::query("UPDATE alert_instances SET notifications_sent = $2 WHERE id = $1")
        .bind(alert_id)
        .bind(Value::Array(log))
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(not(feature = "postgres"))]
#[allow(dead_code)]
async fn dispatch_to_channels(
    _pool: &DbPool,
    _alert_id: Uuid,
    _channel_ids: &[Uuid],
    _payload: &AlertNotificationPayload,
) -> Result<(), AlertingError> {
    Ok(())
}

/// Default per-policy timeout — bound the engine to keep tokio tasks
/// from accumulating if a downstream channel hangs.
#[allow(dead_code)]
pub(crate) const ENGINE_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ctx(app_id: Uuid, component_id: Uuid) -> TransitionContext {
        TransitionContext {
            org_id: Uuid::new_v4(),
            app_id,
            app_name: "app".to_string(),
            component_id,
            component_name: "comp".to_string(),
            from: ComponentState::Running,
            to: ComponentState::Failed,
            at: Utc::now(),
        }
    }

    #[test]
    fn selector_empty_matches_everything() {
        let sel = AlertSelector::default();
        let c = ctx(Uuid::new_v4(), Uuid::new_v4());
        assert!(selector_matches(&sel, &c, &HashMap::new()));
    }

    #[test]
    fn selector_app_id_matches_when_equal() {
        let app = Uuid::new_v4();
        let sel = AlertSelector {
            app_id: Some(app),
            ..Default::default()
        };
        let c = ctx(app, Uuid::new_v4());
        assert!(selector_matches(&sel, &c, &HashMap::new()));
    }

    #[test]
    fn selector_app_id_rejects_when_different() {
        let sel = AlertSelector {
            app_id: Some(Uuid::new_v4()),
            ..Default::default()
        };
        let c = ctx(Uuid::new_v4(), Uuid::new_v4());
        assert!(!selector_matches(&sel, &c, &HashMap::new()));
    }

    #[test]
    fn selector_component_id_matches_exactly() {
        let comp = Uuid::new_v4();
        let sel = AlertSelector {
            component_id: Some(comp),
            ..Default::default()
        };
        let c = ctx(Uuid::new_v4(), comp);
        assert!(selector_matches(&sel, &c, &HashMap::new()));
    }

    #[test]
    fn selector_tags_subset_match() {
        let sel = AlertSelector {
            tags: [
                ("env".to_string(), "prod".to_string()),
                ("tier".to_string(), "db".to_string()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let c = ctx(Uuid::new_v4(), Uuid::new_v4());
        let tags: HashMap<String, String> = [
            ("env".to_string(), "prod".to_string()),
            ("tier".to_string(), "db".to_string()),
            ("zone".to_string(), "PRD".to_string()),
        ]
        .into_iter()
        .collect();
        assert!(selector_matches(&sel, &c, &tags));
    }

    #[test]
    fn selector_tags_partial_mismatch_rejects() {
        let sel = AlertSelector {
            tags: [("env".to_string(), "prod".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let c = ctx(Uuid::new_v4(), Uuid::new_v4());
        let tags: HashMap<String, String> = [("env".to_string(), "staging".to_string())]
            .into_iter()
            .collect();
        assert!(!selector_matches(&sel, &c, &tags));
    }

    #[test]
    fn selector_tags_missing_key_rejects() {
        let sel = AlertSelector {
            tags: [("env".to_string(), "prod".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let c = ctx(Uuid::new_v4(), Uuid::new_v4());
        assert!(!selector_matches(&sel, &c, &HashMap::new()));
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let p = Uuid::new_v4();
        let c = Uuid::new_v4();
        let a = fingerprint(p, c);
        let b = fingerprint(p, c);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64); // sha256 hex
    }

    #[test]
    fn fingerprint_differs_per_policy_or_component() {
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        let c = Uuid::new_v4();
        assert_ne!(fingerprint(p1, c), fingerprint(p2, c));
        assert_ne!(fingerprint(p1, c), fingerprint(p1, Uuid::new_v4()));
    }
}
