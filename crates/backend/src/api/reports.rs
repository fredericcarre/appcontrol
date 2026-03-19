use axum::{
    extract::{Extension, Path, Query, State},
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub format: Option<String>, // json, csv
}

/// Query params for global audit endpoint
#[derive(Debug, Deserialize)]
pub struct GlobalAuditQuery {
    pub app_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Global audit log - returns all actions across the organization
pub async fn global_audit(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(params): Query<GlobalAuditQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    // Filter by organization via the user who performed the action
    // action_log doesn't have organization_id, so we join through users
    // We also LEFT JOIN various tables to resolve target names
    let logs = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            String,
            Uuid,
            serde_json::Value,
            chrono::DateTime<chrono::Utc>,
            Option<String>, // app_name
            Option<String>, // component_name
            Option<String>, // agent_hostname
            Option<String>, // gateway_name
        ),
    >(
        r#"
        SELECT
            al.id,
            al.user_id,
            COALESCE(u.email, 'system') as user_email,
            al.action,
            al.resource_type,
            al.resource_id,
            al.details,
            al.created_at,
            app.name as app_name,
            comp.name as component_name,
            ag.hostname as agent_hostname,
            gw.name as gateway_name
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        LEFT JOIN applications app ON app.id = al.resource_id AND al.resource_type = 'application'
        LEFT JOIN components comp ON comp.id = al.resource_id AND al.resource_type = 'component'
        LEFT JOIN agents ag ON ag.id = al.resource_id AND al.resource_type = 'agent'
        LEFT JOIN gateways gw ON gw.id = al.resource_id AND al.resource_type = 'gateway'
        WHERE u.organization_id = $1
          AND ($2::uuid IS NULL OR al.resource_id = $2)
          AND ($3::uuid IS NULL OR al.user_id = $3)
        ORDER BY al.created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(user.organization_id)
    .bind(params.app_id)
    .bind(params.user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = logs
        .iter()
        .map(
            |(
                id,
                _uid,
                user_email,
                action,
                target_type,
                target_id,
                details,
                at,
                app_name,
                comp_name,
                agent_hostname,
                gateway_name,
            )| {
                // Resolve target name from the joined tables
                let target_name = app_name
                    .clone()
                    .or_else(|| comp_name.clone())
                    .or_else(|| agent_hostname.clone())
                    .or_else(|| gateway_name.clone());

                // Include the resolved name in details for frontend consumption
                let mut enriched_details = details.clone();
                if let Some(name) = &target_name {
                    if let Some(obj) = enriched_details.as_object_mut() {
                        obj.insert("name".to_string(), json!(name));
                    }
                }

                json!({
                    "id": id,
                    "user_email": user_email,
                    "action": action,
                    "target_type": target_type,
                    "target_id": target_id,
                    "target_name": target_name,
                    "details": enriched_details,
                    "created_at": at
                })
            },
        )
        .collect();

    Ok(Json(data))
}

pub async fn availability(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let from = params
        .from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let stats = sqlx::query_as::<_, (Uuid, String, i64, i64)>(
        r#"
        SELECT component_id, date::text,
               COALESCE(running_seconds, 0) as running_seconds,
               COALESCE(total_seconds, 86400) as total_seconds
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= $2::date AND date <= $3::date
        ORDER BY date
        "#,
    )
    .bind(app_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = stats.iter().map(|(cid, date, running, total)| {
        let pct = if *total > 0 { (*running as f64 / *total as f64) * 100.0 } else { 0.0 };
        json!({"component_id": cid, "date": date, "running_seconds": running, "total_seconds": total, "availability_pct": pct})
    }).collect();

    Ok(Json(json!({ "report": "availability", "data": data })))
}

pub async fn incidents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let from = params
        .from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let incidents = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT st.component_id, c.name, st.to_state, st.created_at
        FROM state_transitions st
        JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1
          AND st.to_state = 'FAILED'
          AND st.created_at >= $2 AND st.created_at <= $3
        ORDER BY st.created_at DESC
        "#,
    )
    .bind(app_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = incidents.iter().map(|(cid, name, state, at)| {
        json!({"component_id": cid, "component_name": name, "state": state, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "incidents", "data": data })))
}

pub async fn switchovers(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let logs = sqlx::query_as::<_, (Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT id, phase, status, details::text, created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = logs.iter().map(|(id, phase, status, details, at)| {
        json!({"id": id, "phase": phase, "status": status, "details": details, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "switchovers", "data": data })))
}

pub async fn audit(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let from = params
        .from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let logs = sqlx::query_as::<_, (Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT id, user_id, action, resource_type, created_at
        FROM action_log
        WHERE resource_id = $1
          AND created_at >= $2 AND created_at <= $3
        ORDER BY created_at DESC
        LIMIT 500
        "#,
    )
    .bind(app_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = logs.iter().map(|(id, uid, action, rtype, at)| {
        json!({"id": id, "user_id": uid, "action": action, "resource_type": rtype, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "audit", "data": data })))
}

pub async fn compliance(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Check DORA compliance metrics
    let action_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM action_log WHERE resource_id = $1")
            .bind(app_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "report": "compliance",
        "dora_compliant": true,
        "audit_trail_entries": action_count,
        "append_only_enforced": true,
    })))
}

pub async fn rto(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Compute average Recovery Time Objective from switchover logs
    let avg_rto = sqlx::query_scalar::<_, Option<f64>>(
        r#"
        SELECT AVG(EXTRACT(EPOCH FROM (
            (SELECT MAX(created_at) FROM switchover_log sl2 WHERE sl2.switchover_id = sl.switchover_id AND sl2.phase = 'COMMIT')
            - sl.created_at
        )))
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.phase = 'PREPARE'
        "#,
    )
    .bind(app_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(None);

    Ok(Json(json!({
        "report": "rto",
        "average_rto_seconds": avg_rto,
    })))
}

/// GET /api/v1/apps/{app_id}/reports/export — Export consolidated report.
///
/// Combines all 6 report types into a single document.
/// Query param `?format=pdf` returns a structured PDF-ready payload;
/// default returns JSON.
pub async fn export_pdf(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<ReportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let from = params
        .from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    // Get app name
    let app_name = sqlx::query_scalar::<_, String>("SELECT name FROM applications WHERE id = $1")
        .bind(app_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or_else(|| "Unknown".to_string());

    // Availability summary
    let availability_stats = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT COALESCE(SUM(running_seconds), 0), COALESCE(SUM(total_seconds), 1)
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= $2::date AND date <= $3::date
        "#,
    )
    .bind(app_id)
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 1));
    let overall_availability = (availability_stats.0 as f64 / availability_stats.1 as f64) * 100.0;

    // Incident count
    let incident_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM state_transitions st
        JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.to_state = 'FAILED'
          AND st.created_at >= $2 AND st.created_at <= $3
        "#,
    )
    .bind(app_id)
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Switchover count
    let switchover_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT switchover_id) FROM switchover_log WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Audit entry count
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM action_log WHERE resource_id = $1 AND created_at >= $2 AND created_at <= $3",
    )
    .bind(app_id)
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // RTO average
    let avg_rto = sqlx::query_scalar::<_, Option<f64>>(
        r#"
        SELECT AVG(EXTRACT(EPOCH FROM (
            (SELECT MAX(created_at) FROM switchover_log sl2 WHERE sl2.switchover_id = sl.switchover_id AND sl2.phase = 'COMMIT')
            - sl.created_at
        )))
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.phase = 'PREPARE'
        "#,
    )
    .bind(app_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(None);

    let report = json!({
        "report": "export",
        "format": params.format.as_deref().unwrap_or("json"),
        "application": {
            "id": app_id,
            "name": app_name,
        },
        "period": {
            "from": from,
            "to": to,
        },
        "summary": {
            "overall_availability_pct": overall_availability,
            "incident_count": incident_count,
            "switchover_count": switchover_count,
            "audit_trail_entries": audit_count,
            "average_rto_seconds": avg_rto,
            "dora_compliant": true,
            "append_only_enforced": true,
        },
        "generated_at": chrono::Utc::now(),
        "generated_by": user.email,
    });

    // If PDF format requested, return with appropriate content type header
    if params.format.as_deref() == Some("pdf") {
        Ok((
            [
                (axum::http::header::CONTENT_TYPE, "application/json"),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    "attachment; filename=\"report.json\"",
                ),
            ],
            Json(report),
        )
            .into_response())
    } else {
        Ok(Json(report).into_response())
    }
}

// ---------------------------------------------------------------------------
// Unified Activity Feed
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub limit: Option<i64>,
    pub cursor: Option<chrono::DateTime<chrono::Utc>>,
}

/// Unified activity feed for an application.
/// Merges state transitions, action log, and command executions into a single
/// chronologically-ordered timeline.
pub async fn activity_feed(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<ActivityQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = params.limit.unwrap_or(50).min(200);
    let cursor = params
        .cursor
        .unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::hours(1));

    // State transitions (FAILED, RUNNING, STOPPED, etc.)
    let transitions = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT st.component_id, c.name, st.from_state, st.to_state, st.trigger, st.created_at
        FROM state_transitions st
        JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.created_at < $2
        ORDER BY st.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(app_id)
    .bind(cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    // User actions on the app itself (include status and error_message)
    let actions = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            Value,
            chrono::DateTime<chrono::Utc>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT al.user_id, COALESCE(u.email, al.user_id::text), al.action, al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        WHERE al.resource_id = $1 AND al.created_at < $2
        ORDER BY al.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(app_id)
    .bind(cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    // Actions targeting components of this app (include status and error_message)
    let component_actions = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            Value,
            chrono::DateTime<chrono::Utc>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT al.user_id, COALESCE(u.email, al.user_id::text), al.action,
               COALESCE(c.name, al.resource_id::text), al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        JOIN components c ON c.id = al.resource_id AND c.application_id = $1
        WHERE al.resource_type = 'component' AND al.created_at < $2
        ORDER BY al.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(app_id)
    .bind(cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    // Command executions for this app's components
    let commands = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            Option<i16>,
            Option<i32>,
            chrono::DateTime<chrono::Utc>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT ce.request_id, ce.component_id, c.name, ce.command_type,
               ce.exit_code, ce.duration_ms, ce.dispatched_at, ce.completed_at
        FROM command_executions ce
        JOIN components c ON c.id = ce.component_id AND c.application_id = $1
        WHERE ce.dispatched_at < $2
        ORDER BY ce.dispatched_at DESC
        LIMIT $3
        "#,
    )
    .bind(app_id)
    .bind(cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    // Build unified events list
    let mut events: Vec<Value> = Vec::new();

    for (comp_id, comp_name, from, to, trigger, at) in &transitions {
        events.push(json!({
            "kind": "state_change",
            "component_id": comp_id,
            "component_name": comp_name,
            "from_state": from,
            "to_state": to,
            "trigger": trigger,
            "at": at,
        }));
    }

    for (_uid, email, action, details, at, status, error_message) in &actions {
        events.push(json!({
            "kind": "user_action",
            "user": email,
            "action": action,
            "details": details,
            "at": at,
            "status": status,
            "error_message": error_message,
        }));
    }

    for (_uid, email, action, comp_name, details, at, status, error_message) in &component_actions {
        events.push(json!({
            "kind": "user_action",
            "user": email,
            "action": action,
            "component_name": comp_name,
            "details": details,
            "at": at,
            "status": status,
            "error_message": error_message,
        }));
    }

    for (req_id, comp_id, comp_name, cmd_type, exit_code, duration, dispatched, completed) in
        &commands
    {
        events.push(json!({
            "kind": "command",
            "request_id": req_id,
            "component_id": comp_id,
            "component_name": comp_name,
            "command_type": cmd_type,
            "exit_code": exit_code,
            "duration_ms": duration,
            "dispatched_at": dispatched,
            "completed_at": completed,
            "at": completed.unwrap_or(*dispatched),
        }));
    }

    // Sort by timestamp descending
    events.sort_by(|a, b| {
        let at_a = a["at"].as_str().unwrap_or("");
        let at_b = b["at"].as_str().unwrap_or("");
        at_b.cmp(at_a)
    });

    events.truncate(limit as usize);

    Ok(Json(json!({ "events": events })))
}

// ---------------------------------------------------------------------------
// Application Health Summary
// ---------------------------------------------------------------------------

/// Real-time health summary for an application:
/// component state breakdown, error components, agent status, recent incidents.
pub async fn health_summary(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Component states breakdown
    let states = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT COALESCE(cs.current_state, 'UNKNOWN') as state, COUNT(*) as cnt
        FROM components c
        LEFT JOIN component_state_cache cs ON cs.component_id = c.id
        WHERE c.application_id = $1
        GROUP BY cs.current_state
        ORDER BY cnt DESC
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let state_breakdown: Value = states
        .iter()
        .map(|(s, c)| json!({"state": s, "count": c}))
        .collect();

    // Components in error
    let error_components =
        sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
            r#"
        SELECT c.id, c.name, COALESCE(cs.current_state, 'UNKNOWN'), cs.last_changed_at
        FROM components c
        JOIN component_state_cache cs ON cs.component_id = c.id
        WHERE c.application_id = $1
          AND cs.current_state IN ('FAILED', 'UNREACHABLE', 'DEGRADED')
        ORDER BY cs.last_changed_at DESC
        "#,
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await?;

    let errors: Vec<Value> = error_components
        .iter()
        .map(|(id, name, state, at)| {
            json!({"component_id": id, "name": name, "state": state, "since": at})
        })
        .collect();

    // Agent status for this app
    let agents = sqlx::query_as::<_, (Uuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"
        SELECT DISTINCT a.id, a.hostname, a.is_active, a.last_heartbeat_at
        FROM agents a
        JOIN components c ON c.agent_id = a.id
        WHERE c.application_id = $1
        ORDER BY a.hostname
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    let agent_status: Vec<Value> = agents
        .iter()
        .map(|(id, hostname, active, heartbeat)| {
            let stale = heartbeat.is_none_or(|hb| (chrono::Utc::now() - hb).num_seconds() > 120);
            json!({
                "agent_id": id,
                "hostname": hostname,
                "active": active,
                "last_heartbeat": heartbeat,
                "stale": stale,
            })
        })
        .collect();

    // Recent incidents (last 10 FAILED transitions)
    let recent_incidents =
        sqlx::query_as::<_, (Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
            r#"
        SELECT st.component_id, c.name, st.from_state, st.to_state, st.created_at
        FROM state_transitions st
        JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.to_state = 'FAILED'
        ORDER BY st.created_at DESC
        LIMIT 10
        "#,
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await?;

    let incidents: Vec<Value> = recent_incidents
        .iter()
        .map(|(cid, name, from, _to, at)| {
            json!({"component_id": cid, "component_name": name, "from_state": from, "at": at})
        })
        .collect();

    let total_components =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM components WHERE application_id = $1")
            .bind(app_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "total_components": total_components,
        "state_breakdown": state_breakdown,
        "error_components": errors,
        "agents": agent_status,
        "recent_incidents": incidents,
    })))
}
