//! Alerting API: CRUD for notification channels and alert policies, plus
//! a read-only feed of `alert_instances` with acknowledge / resolve
//! mutations.
//!
//! Permissions: org admin only for write operations (channels and
//! policies are org-wide configuration). Any authenticated user in the
//! org can read alert instances (filtered further by component
//! permissions in a follow-up sprint).
//!
//! Postgres-only for this MVP; SQLite parity arrives once the engine
//! is ported.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::alerting::{
    AlertSelector, AlertSeverity, AlertStatus, NotificationChannelConfig,
};

// ---------------------------------------------------------------------------
// Request/response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    /// Vendor-specific config, tagged with `"kind"` (webhook | slack).
    pub config: NotificationChannelConfig,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ChannelResponse {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    /// Config with secrets masked. Use the original on-create response (or
    /// the database, with admin privileges) to recover the raw value.
    pub config: NotificationChannelConfig,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub selector: AlertSelector,
    /// ComponentState names. Validated server-side.
    pub trigger_states: Vec<String>,
    #[serde(default)]
    pub sustain_seconds: i32,
    #[serde(default = "default_severity")]
    pub severity: AlertSeverity,
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: i32,
    #[serde(default)]
    pub channel_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PolicyResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub selector: AlertSelector,
    pub trigger_states: Vec<String>,
    pub sustain_seconds: i32,
    pub severity: AlertSeverity,
    pub cooldown_seconds: i32,
    pub channel_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AlertInstanceResponse {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub component_id: Uuid,
    pub severity: AlertSeverity,
    pub status: AlertStatus,
    pub triggered_state: String,
    pub summary: Option<String>,
    pub fired_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub notifications_sent: Value,
}

fn default_true() -> bool {
    true
}
fn default_severity() -> AlertSeverity {
    AlertSeverity::Warning
}
fn default_cooldown() -> i32 {
    300
}

// ---------------------------------------------------------------------------
// Channel handlers
// ---------------------------------------------------------------------------

pub async fn list_channels(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<ChannelResponse>>, ApiError> {
    #[cfg(feature = "postgres")]
    {
        let rows: Vec<(Uuid, String, String, Value, bool, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, name, kind, config, enabled, created_at
               FROM notification_channels
              WHERE org_id = $1
              ORDER BY name",
        )
        .bind(*user.organization_id.as_ref())
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("list channels: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for (id, name, kind, config_json, enabled, created_at) in rows {
            let config: NotificationChannelConfig = serde_json::from_value(config_json)
                .map_err(|e| ApiError::Internal(format!("bad channel config row {id}: {e}")))?;
            out.push(ChannelResponse {
                id,
                name,
                kind,
                config: config.redacted(),
                enabled,
                created_at,
            });
        }
        Ok(Json(out))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = (state, user);
        Ok(Json(vec![]))
    }
}

pub async fn create_channel(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>), ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    let id = Uuid::new_v4();
    let kind = match &req.config {
        NotificationChannelConfig::Webhook { .. } => "webhook",
        NotificationChannelConfig::Slack { .. } => "slack",
    };
    let config_json = serde_json::to_value(&req.config)
        .map_err(|e| ApiError::Validation(format!("bad config: {e}")))?;

    log_action(
        &state.db,
        *user.user_id.as_ref(),
        "create_alert_channel",
        "notification_channel",
        id,
        json!({"name": req.name, "kind": kind}),
    )
    .await
    .ok();

    #[cfg(feature = "postgres")]
    {
        let (created_at,): (DateTime<Utc>,) = sqlx::query_as(
            "INSERT INTO notification_channels
                 (id, org_id, name, kind, config, enabled)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING created_at",
        )
        .bind(id)
        .bind(*user.organization_id.as_ref())
        .bind(&req.name)
        .bind(kind)
        .bind(&config_json)
        .bind(req.enabled)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Validation(format!("insert: {e}")))?;

        Ok((
            StatusCode::CREATED,
            Json(ChannelResponse {
                id,
                name: req.name,
                kind: kind.to_string(),
                config: req.config.redacted(),
                enabled: req.enabled,
                created_at,
            }),
        ))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = config_json;
        Err(ApiError::Internal(
            "alerting CRUD is postgres-only in this MVP".into(),
        ))
    }
}

pub async fn delete_channel(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    log_action(
        &state.db,
        *user.user_id.as_ref(),
        "delete_alert_channel",
        "notification_channel",
        id,
        json!({}),
    )
    .await
    .ok();
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query("DELETE FROM notification_channels WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(*user.organization_id.as_ref())
            .execute(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("delete: {e}")))?
            .rows_affected();
        if rows == 0 {
            return Err(ApiError::NotFound);
        }
        Ok(StatusCode::NO_CONTENT)
    }
    #[cfg(not(feature = "postgres"))]
    Err(ApiError::Internal(
        "alerting CRUD is postgres-only in this MVP".into(),
    ))
}

// ---------------------------------------------------------------------------
// Policy handlers
// ---------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
pub async fn list_policies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<PolicyResponse>>, ApiError> {
    #[cfg(feature = "postgres")]
    {
        let rows: Vec<(
            Uuid,
            String,
            Option<String>,
            bool,
            Value,
            Vec<String>,
            i32,
            String,
            i32,
            Vec<Uuid>,
            DateTime<Utc>,
        )> = sqlx::query_as(
            "SELECT id, name, description, enabled, selector, trigger_states,
                    sustain_seconds, severity, cooldown_seconds, channel_ids, created_at
               FROM alert_policies
              WHERE org_id = $1
              ORDER BY name",
        )
        .bind(*user.organization_id.as_ref())
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("list policies: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for (
            id,
            name,
            description,
            enabled,
            selector_json,
            trigger_states,
            sustain_seconds,
            severity_raw,
            cooldown_seconds,
            channel_ids,
            created_at,
        ) in rows
        {
            let selector: AlertSelector = serde_json::from_value(selector_json)
                .map_err(|e| ApiError::Internal(format!("bad selector row {id}: {e}")))?;
            let severity: AlertSeverity = severity_raw
                .parse()
                .map_err(|e| ApiError::Internal(format!("bad severity row {id}: {e}")))?;
            out.push(PolicyResponse {
                id,
                name,
                description,
                enabled,
                selector,
                trigger_states,
                sustain_seconds,
                severity,
                cooldown_seconds,
                channel_ids,
                created_at,
            });
        }
        Ok(Json(out))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = (state, user);
        Ok(Json(vec![]))
    }
}

pub async fn create_policy(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<(StatusCode, Json<PolicyResponse>), ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    // Validate that every trigger state is a known ComponentState.
    for s in &req.trigger_states {
        use std::str::FromStr;
        if appcontrol_common::ComponentState::from_str(s).is_err() {
            return Err(ApiError::Validation(format!(
                "unknown component state '{s}'"
            )));
        }
    }
    let id = Uuid::new_v4();
    let selector_json = serde_json::to_value(&req.selector)
        .map_err(|e| ApiError::Validation(format!("bad selector: {e}")))?;

    log_action(
        &state.db,
        *user.user_id.as_ref(),
        "create_alert_policy",
        "alert_policy",
        id,
        json!({"name": req.name, "severity": req.severity}),
    )
    .await
    .ok();

    #[cfg(feature = "postgres")]
    {
        let (created_at,): (DateTime<Utc>,) = sqlx::query_as(
            "INSERT INTO alert_policies
                 (id, org_id, name, description, enabled, selector,
                  trigger_states, sustain_seconds, severity,
                  cooldown_seconds, channel_ids)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             RETURNING created_at",
        )
        .bind(id)
        .bind(*user.organization_id.as_ref())
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.enabled)
        .bind(&selector_json)
        .bind(&req.trigger_states)
        .bind(req.sustain_seconds)
        .bind(format!("{}", req.severity))
        .bind(req.cooldown_seconds)
        .bind(&req.channel_ids)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Validation(format!("insert: {e}")))?;

        Ok((
            StatusCode::CREATED,
            Json(PolicyResponse {
                id,
                name: req.name,
                description: req.description,
                enabled: req.enabled,
                selector: req.selector,
                trigger_states: req.trigger_states,
                sustain_seconds: req.sustain_seconds,
                severity: req.severity,
                cooldown_seconds: req.cooldown_seconds,
                channel_ids: req.channel_ids,
                created_at,
            }),
        ))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = selector_json;
        Err(ApiError::Internal(
            "alerting CRUD is postgres-only in this MVP".into(),
        ))
    }
}

pub async fn delete_policy(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    log_action(
        &state.db,
        *user.user_id.as_ref(),
        "delete_alert_policy",
        "alert_policy",
        id,
        json!({}),
    )
    .await
    .ok();
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query("DELETE FROM alert_policies WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(*user.organization_id.as_ref())
            .execute(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("delete: {e}")))?
            .rows_affected();
        if rows == 0 {
            return Err(ApiError::NotFound);
        }
        Ok(StatusCode::NO_CONTENT)
    }
    #[cfg(not(feature = "postgres"))]
    Err(ApiError::Internal(
        "alerting CRUD is postgres-only in this MVP".into(),
    ))
}

// ---------------------------------------------------------------------------
// Alert instance handlers
// ---------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
pub async fn list_alerts(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<AlertInstanceResponse>>, ApiError> {
    #[cfg(feature = "postgres")]
    {
        let rows: Vec<(
            Uuid,
            Uuid,
            Uuid,
            String,
            String,
            String,
            Option<String>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            Option<Uuid>,
            Option<DateTime<Utc>>,
            Value,
        )> = sqlx::query_as(
            "SELECT id, policy_id, component_id, severity, status,
                    triggered_state, summary, fired_at, acknowledged_at,
                    acknowledged_by, resolved_at, notifications_sent
               FROM alert_instances
              WHERE org_id = $1
              ORDER BY fired_at DESC
              LIMIT 500",
        )
        .bind(*user.organization_id.as_ref())
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("list alerts: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for (
            id,
            policy_id,
            component_id,
            severity_raw,
            status_raw,
            triggered_state,
            summary,
            fired_at,
            acknowledged_at,
            acknowledged_by,
            resolved_at,
            notifications_sent,
        ) in rows
        {
            let severity: AlertSeverity = severity_raw
                .parse()
                .map_err(|e| ApiError::Internal(format!("bad severity row {id}: {e}")))?;
            let status: AlertStatus = status_raw
                .parse()
                .map_err(|e| ApiError::Internal(format!("bad status row {id}: {e}")))?;
            out.push(AlertInstanceResponse {
                id,
                policy_id,
                component_id,
                severity,
                status,
                triggered_state,
                summary,
                fired_at,
                acknowledged_at,
                acknowledged_by,
                resolved_at,
                notifications_sent,
            });
        }
        Ok(Json(out))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = (state, user);
        Ok(Json(vec![]))
    }
}

pub async fn acknowledge_alert(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    log_action(
        &state.db,
        *user.user_id.as_ref(),
        "acknowledge_alert",
        "alert_instance",
        id,
        json!({}),
    )
    .await
    .ok();
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query(
            "UPDATE alert_instances
                SET status = 'acknowledged',
                    acknowledged_at = NOW(),
                    acknowledged_by = $2
              WHERE id = $1 AND org_id = $3 AND status = 'firing'",
        )
        .bind(id)
        .bind(*user.user_id.as_ref())
        .bind(*user.organization_id.as_ref())
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("ack: {e}")))?
        .rows_affected();
        if rows == 0 {
            return Err(ApiError::NotFound);
        }
        Ok(StatusCode::NO_CONTENT)
    }
    #[cfg(not(feature = "postgres"))]
    Err(ApiError::Internal(
        "alerting CRUD is postgres-only in this MVP".into(),
    ))
}

pub async fn resolve_alert(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    log_action(
        &state.db,
        *user.user_id.as_ref(),
        "resolve_alert",
        "alert_instance",
        id,
        json!({}),
    )
    .await
    .ok();
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query(
            "UPDATE alert_instances
                SET status = 'resolved', resolved_at = NOW()
              WHERE id = $1 AND org_id = $2 AND status IN ('firing','acknowledged')",
        )
        .bind(id)
        .bind(*user.organization_id.as_ref())
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("resolve: {e}")))?
        .rows_affected();
        if rows == 0 {
            return Err(ApiError::NotFound);
        }
        Ok(StatusCode::NO_CONTENT)
    }
    #[cfg(not(feature = "postgres"))]
    Err(ApiError::Internal(
        "alerting CRUD is postgres-only in this MVP".into(),
    ))
}
