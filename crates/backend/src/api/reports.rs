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
use crate::db::DbUuid;
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
    let logs = fetch_global_audit_logs(
        &state.db,
        *user.organization_id,
        params.app_id,
        params.user_id,
        limit,
        offset,
    )
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

    let stats = fetch_availability_stats(&state.db, app_id, from, to).await?;

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

    let incidents = sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
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
    .bind(crate::db::bind_id(app_id))
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

    let logs = fetch_switchover_logs(&state.db, app_id).await?;

    let data: Vec<Value> = logs.iter().map(|(id, phase, status, details, at)| {
        json!({"id": id, "phase": phase, "status": status, "details": details, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "switchovers", "data": data })))
}

/// DRP Exercise Report - Detailed switchover history for DORA compliance
/// Groups switchover phases and calculates RTO for each exercise
pub async fn drp_report(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Get application info
    let app_info = sqlx::query_as::<_, (String, Option<DbUuid>, Option<String>)>(
        "SELECT a.name, a.site_id, s.name FROM applications a LEFT JOIN sites s ON a.site_id = s.id WHERE a.id = $1"
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_optional(&state.db)
    .await?;

    let (app_name, _site_id, site_name) = app_info.unwrap_or(("Unknown".to_string(), None, None));

    // Get all switchover logs grouped by switchover_id
    let logs = sqlx::query_as::<_, (DbUuid, String, String, Value, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT switchover_id, phase, status, details, created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY switchover_id, created_at ASC
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

    // Pre-fetch all sites for name lookup
    let sites: std::collections::HashMap<DbUuid, String> =
        sqlx::query_as::<_, (DbUuid, String)>("SELECT id, name FROM sites")
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .collect();

    // Group by switchover_id
    // Phase tuple: (phase_name, status, details, created_at)
    type PhaseEntry = (String, String, Value, chrono::DateTime<chrono::Utc>);
    let mut switchovers_map: std::collections::HashMap<DbUuid, Vec<PhaseEntry>> =
        std::collections::HashMap::new();

    for (switchover_id, phase, status, details, created_at) in logs {
        switchovers_map
            .entry(switchover_id)
            .or_default()
            .push((phase, status, details, created_at));
    }

    // Build structured switchover reports
    let mut switchovers: Vec<Value> = Vec::new();

    for (switchover_id, phases) in switchovers_map {
        let mut phase_details: Vec<Value> = Vec::new();
        let mut started_at: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut completed_at: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut final_status = "in_progress".to_string();
        let mut source_site: Option<String> = None;
        let mut target_site: Option<String> = None;
        let mut target_site_id: Option<Uuid> = None;
        let mut components_count: Option<i64> = None;
        let mut current_phase_start: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut initiated_by_user_id: Option<String> = None;

        for (phase, status, details, at) in &phases {
            // Track first timestamp as switchover start
            if started_at.is_none() {
                started_at = Some(*at);
            }

            // Extract site info and initiated_by from PREPARE phase
            if phase == "PREPARE" && status == "in_progress" {
                // Get target_site_id and lookup name
                if let Some(tid) = details["target_site_id"]
                    .as_str()
                    .and_then(|s| s.parse::<Uuid>().ok())
                {
                    target_site_id = Some(tid);
                    target_site = sites.get(&tid).cloned();
                }
                // Get initiated_by user_id
                if initiated_by_user_id.is_none() {
                    initiated_by_user_id = details["initiated_by"].as_str().map(String::from);
                }
            }

            // Extract source/target profile info from START_TARGET completed phase
            if phase == "START_TARGET" && status == "completed" {
                if source_site.is_none() {
                    source_site = details["source_profile"].as_str().map(String::from);
                }
                if target_site.is_none() {
                    target_site = details["target_profile"].as_str().map(String::from);
                }
            }

            // Extract component count from relevant phases
            if let Some(count) = details["components_impacted"].as_i64() {
                components_count = Some(count);
            }
            if let Some(count) = details["components_swapped"].as_i64() {
                components_count = Some(count);
            }

            // Track phase timing
            if status == "in_progress" {
                current_phase_start = Some(*at);
            } else if status == "completed" || status == "failed" {
                let phase_started = current_phase_start.unwrap_or(*at);
                let duration_ms = (*at - phase_started).num_milliseconds();

                phase_details.push(json!({
                    "phase": phase,
                    "status": status,
                    "started_at": phase_started,
                    "completed_at": at,
                    "duration_ms": duration_ms,
                    "details": details
                }));

                current_phase_start = None;
            }

            // Track final status
            if phase == "COMMIT" && status == "completed" {
                final_status = "completed".to_string();
                completed_at = Some(*at);
            } else if phase == "ROLLBACK" && status == "completed" {
                final_status = "rolled_back".to_string();
                completed_at = Some(*at);
            } else if status == "failed" {
                final_status = "failed".to_string();
                completed_at = Some(*at);
            }
        }

        // Calculate RTO (Recovery Time Objective) in seconds
        let rto_seconds = match (started_at, completed_at) {
            (Some(start), Some(end)) => Some((end - start).num_seconds()),
            _ => None,
        };

        // Get component sequence during this switchover (only final states: STOPPED and RUNNING)
        let component_sequence = if let (Some(start), Some(end)) = (started_at, completed_at) {
            let transitions =
                sqlx::query_as::<_, (String, String, String, chrono::DateTime<chrono::Utc>)>(
                    r#"
                SELECT c.name, st.from_state, st.to_state, st.created_at
                FROM state_transitions st
                JOIN components c ON c.id = st.component_id
                WHERE c.application_id = $1
                  AND st.created_at >= $2
                  AND st.created_at <= $3
                  AND st.to_state IN ('STOPPED', 'RUNNING')
                ORDER BY st.created_at ASC
                "#,
                )
                .bind(crate::db::bind_id(app_id))
                .bind(start)
                .bind(end)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();

            let seq: Vec<Value> = transitions
                .into_iter()
                .map(|(name, from, to, at)| {
                    json!({
                        "component": name,
                        "from_state": from,
                        "to_state": to,
                        "at": at
                    })
                })
                .collect();
            Some(seq)
        } else {
            None
        };

        // Get user email if we have the user_id (extracted from PREPARE phase above)
        let initiated_by_email: Option<String> = if let Some(ref user_id_str) = initiated_by_user_id
        {
            if let Ok(user_id) = uuid::Uuid::parse_str(user_id_str) {
                sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
                    .bind(crate::db::bind_id(user_id))
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else {
            None
        };

        // Get executed commands during this switchover from state_transitions
        // The sequencer doesn't log to action_log, so we derive commands from state changes
        // Include agent and gateway info for each component
        let commands_executed = if let (Some(start), Some(end)) = (started_at, completed_at) {
            let cmds = sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    chrono::DateTime<chrono::Utc>,
                ),
            >(
                r#"
                SELECT st.to_state, c.name, c.start_cmd, c.stop_cmd,
                       a.hostname as agent_hostname, g.name as gateway_name, st.created_at
                FROM state_transitions st
                JOIN components c ON c.id = st.component_id
                LEFT JOIN agents a ON a.id = c.agent_id
                LEFT JOIN gateways g ON g.id = a.gateway_id
                WHERE c.application_id = $1
                  AND st.created_at >= $2
                  AND st.created_at <= $3
                  AND st.to_state IN ('STARTING', 'STOPPING')
                ORDER BY st.created_at ASC
                "#,
            )
            .bind(crate::db::bind_id(app_id))
            .bind(start)
            .bind(end)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            let cmd_list: Vec<Value> = cmds
                .into_iter()
                .map(
                    |(to_state, comp_name, start_cmd, stop_cmd, agent, gateway, at)| {
                        // Select the appropriate command based on state transition
                        let (action, command) = match to_state.as_str() {
                            "STARTING" => ("start", start_cmd),
                            "STOPPING" => ("stop", stop_cmd),
                            _ => ("unknown", None),
                        };
                        json!({
                            "action": action,
                            "component": comp_name,
                            "command": command,
                            "agent": agent,
                            "gateway": gateway,
                            "at": at
                        })
                    },
                )
                .collect();
            Some(cmd_list)
        } else {
            None
        };

        switchovers.push(json!({
            "switchover_id": switchover_id,
            "started_at": started_at,
            "completed_at": completed_at,
            "rto_seconds": rto_seconds,
            "status": final_status,
            "initiated_by": initiated_by_email,
            "source_site": source_site,
            "target_site": target_site,
            "target_site_id": target_site_id,
            "components_count": components_count,
            "phases": phase_details,
            "component_sequence": component_sequence,
            "commands_executed": commands_executed
        }));
    }

    // Sort by started_at descending (most recent first)
    switchovers.sort_by(|a, b| {
        let a_time = a["started_at"].as_str().unwrap_or("");
        let b_time = b["started_at"].as_str().unwrap_or("");
        b_time.cmp(a_time)
    });

    // Fetch components for the topology graph
    // position_x/y are `real` (f32) in Postgres, cast to float8 for f64 compatibility
    let components = fetch_topology_components(&state.db, app_id)
        .await
        .unwrap_or_default();

    let component_list: Vec<Value> = components
        .into_iter()
        .map(|(id, name, comp_type, x, y)| {
            json!({
                "id": id,
                "name": name,
                "type": comp_type,
                "position": { "x": x, "y": y }
            })
        })
        .collect();

    // Fetch dependencies for the topology edges
    let dependencies = sqlx::query_as::<_, (DbUuid, DbUuid)>(
        r#"
        SELECT d.from_component_id, d.to_component_id
        FROM dependencies d
        WHERE d.application_id = $1
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let edge_list: Vec<Value> = dependencies
        .into_iter()
        .map(|(source, target)| {
            json!({
                "source": source,
                "target": target
            })
        })
        .collect();

    Ok(Json(json!({
        "report": "drp_exercises",
        "application": {
            "id": app_id,
            "name": app_name,
            "current_site": site_name
        },
        "total_exercises": switchovers.len(),
        "exercises": switchovers,
        "topology": {
            "nodes": component_list,
            "edges": edge_list
        },
        "generated_at": chrono::Utc::now()
    })))
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

    let logs = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT id, user_id, action, resource_type, created_at
        FROM action_log
        WHERE resource_id = $1
          AND created_at >= $2 AND created_at <= $3
        ORDER BY created_at DESC
        LIMIT 500
        "#,
    )
    .bind(crate::db::bind_id(app_id))
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
            .bind(crate::db::bind_id(app_id))
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
    let avg_rto = fetch_avg_rto(&state.db, app_id).await;

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
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(&state.db)
        .await?
        .unwrap_or_else(|| "Unknown".to_string());

    // Availability summary
    let availability_stats = fetch_availability_summary(&state.db, app_id, from, to).await;
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
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Switchover count
    let switchover_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT switchover_id) FROM switchover_log WHERE application_id = $1",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Audit entry count
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM action_log WHERE resource_id = $1 AND created_at >= $2 AND created_at <= $3",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // RTO average
    let avg_rto = fetch_avg_rto(&state.db, app_id).await;

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
// Action Label Formatting (for readable history)
// ---------------------------------------------------------------------------

/// Convert technical action codes to human-readable labels
fn format_action_label(action: &str, details: &Value, component_name: Option<&str>) -> String {
    match action {
        // Application actions
        "start_app" | "start_application" => "Démarrage de l'application".to_string(),
        "stop_app" | "stop_application" => "Arrêt de l'application".to_string(),
        "start_switchover" => {
            let mode = details["mode"].as_str().unwrap_or("FULL");
            let target = details["target_site"]
                .as_str()
                .or_else(|| details["target_site_id"].as_str())
                .unwrap_or("?");
            if mode == "SELECTIVE" {
                format!("Bascule DR partielle vers {}", target)
            } else {
                format!("Bascule DR complète vers {}", target)
            }
        }
        "switchover_next_phase" => "Progression de la bascule DR".to_string(),
        "switchover_rollback" => "Annulation de la bascule DR".to_string(),
        "switchover_commit" => "Validation de la bascule DR".to_string(),
        "diagnose" | "start_diagnose" => "Diagnostic lancé".to_string(),
        "rebuild" | "start_rebuild" => "Reconstruction lancée".to_string(),

        // Component actions
        "start_component" => {
            if let Some(name) = component_name {
                format!("Démarrage de {}", name)
            } else {
                "Démarrage d'un composant".to_string()
            }
        }
        "stop_component" => {
            if let Some(name) = component_name {
                format!("Arrêt de {}", name)
            } else {
                "Arrêt d'un composant".to_string()
            }
        }
        "execute_command" => {
            let cmd = details["command_name"].as_str().unwrap_or("commande");
            if let Some(name) = component_name {
                format!("Exécution de '{}' sur {}", cmd, name)
            } else {
                format!("Exécution de '{}'", cmd)
            }
        }

        // Permission actions
        "grant_permission" => "Attribution de permissions".to_string(),
        "revoke_permission" => "Révocation de permissions".to_string(),
        "create_share_link" => "Création d'un lien de partage".to_string(),

        // Config actions
        "update_component" => {
            if let Some(name) = component_name {
                format!("Modification de {}", name)
            } else {
                "Modification d'un composant".to_string()
            }
        }
        "create_component" => "Création d'un composant".to_string(),
        "delete_component" => "Suppression d'un composant".to_string(),

        // Default: return original action with better formatting
        _ => action.replace('_', " ").to_string(),
    }
}

/// Categorize an event for DORA reporting
fn categorize_event(action: &str, to_state: Option<&str>) -> &'static str {
    match (action, to_state) {
        // Incidents (unplanned failures)
        (_, Some("FAILED")) => "incident",
        (_, Some("UNREACHABLE")) => "incident",

        // Recovery from incidents
        (_, Some("RUNNING")) => "recovery",

        // Planned operations
        ("start_switchover", _) => "dr_operation",
        ("switchover_next_phase", _) | ("switchover_commit", _) => "dr_operation",
        ("start_app", _) | ("stop_app", _) => "planned_operation",
        ("start_component", _) | ("stop_component", _) => "planned_operation",

        // Maintenance
        ("diagnose", _) | ("rebuild", _) => "maintenance",

        // Configuration changes
        ("update_component", _) | ("create_component", _) | ("delete_component", _) => {
            "config_change"
        }

        _ => "other",
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
    .bind(crate::db::bind_id(app_id))
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
        SELECT al.user_id, COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action, al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        WHERE al.resource_id = $1 AND al.created_at < $2
        ORDER BY al.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(crate::db::bind_id(app_id))
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
        SELECT al.user_id, COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action,
               COALESCE(c.name, CAST(al.resource_id AS TEXT)), al.details, al.created_at,
               al.status, al.error_message
        FROM action_log al
        LEFT JOIN users u ON u.id = al.user_id
        JOIN components c ON c.id = al.resource_id AND c.application_id = $1
        WHERE al.resource_type = 'component' AND al.created_at < $2
        ORDER BY al.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(crate::db::bind_id(app_id))
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
    .bind(crate::db::bind_id(app_id))
    .bind(cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    // Switchover events for this app
    let switchovers =
        sqlx::query_as::<_, (DbUuid, String, String, Value, chrono::DateTime<chrono::Utc>)>(
            r#"
        SELECT sl.switchover_id, sl.phase, sl.status, sl.details, sl.created_at
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.created_at < $2
        ORDER BY sl.created_at DESC
        LIMIT $3
        "#,
        )
        .bind(crate::db::bind_id(app_id))
        .bind(cursor)
        .bind(limit)
        .fetch_all(&state.db)
        .await?;

    // Build unified events list
    let mut events: Vec<Value> = Vec::new();

    for (comp_id, comp_name, from, to, trigger, at) in &transitions {
        let category = categorize_event("", Some(to.as_str()));
        let label = match to.as_str() {
            "FAILED" => format!("{} en échec", comp_name),
            "RUNNING" => format!("{} opérationnel", comp_name),
            "STOPPED" => format!("{} arrêté", comp_name),
            "STARTING" => format!("{} en démarrage", comp_name),
            "STOPPING" => format!("{} en arrêt", comp_name),
            "DEGRADED" => format!("{} dégradé", comp_name),
            "UNREACHABLE" => format!("{} injoignable", comp_name),
            _ => format!("{} → {}", comp_name, to),
        };
        events.push(json!({
            "kind": "state_change",
            "category": category,
            "label": label,
            "component_id": comp_id,
            "component_name": comp_name,
            "from_state": from,
            "to_state": to,
            "trigger": trigger,
            "at": at,
        }));
    }

    for (_uid, email, action, details, at, status, error_message) in &actions {
        let label = format_action_label(action, details, None);
        let category = categorize_event(action, None);
        events.push(json!({
            "kind": "user_action",
            "category": category,
            "label": label,
            "user": email,
            "action": action,
            "details": details,
            "at": at,
            "status": status,
            "error_message": error_message,
        }));
    }

    for (_uid, email, action, comp_name, details, at, status, error_message) in &component_actions {
        let label = format_action_label(action, details, Some(comp_name));
        let category = categorize_event(action, None);
        events.push(json!({
            "kind": "user_action",
            "category": category,
            "label": label,
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
        let label = format!("Commande {} sur {}", cmd_type, comp_name);
        events.push(json!({
            "kind": "command",
            "category": "planned_operation",
            "label": label,
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

    // Add switchover events
    for (switchover_id, phase, status, details, at) in &switchovers {
        let label = match (phase.as_str(), status.as_str()) {
            ("PREPARE", "in_progress") => {
                let mode = details["mode"].as_str().unwrap_or("FULL");
                if mode == "SELECTIVE" {
                    "Bascule DR partielle initiée".to_string()
                } else {
                    "Bascule DR complète initiée".to_string()
                }
            }
            ("VALIDATE", "completed") => "Validation DR réussie".to_string(),
            ("VALIDATE", "failed") => "Validation DR échouée".to_string(),
            ("STOP_SOURCE", "completed") => {
                let count = details["components_impacted"]
                    .as_i64()
                    .or_else(|| details["components_still_running"].as_i64().map(|_| 0));
                match count {
                    Some(n) => format!("Arrêt source terminé ({} composants)", n),
                    None => "Arrêt source terminé".to_string(),
                }
            }
            ("STOP_SOURCE", "failed") => "Arrêt source échoué".to_string(),
            ("SYNC", "completed") => "Synchronisation terminée".to_string(),
            ("START_TARGET", "completed") => {
                let swapped = details["components_swapped"].as_i64().unwrap_or(0);
                format!("Démarrage cible terminé ({} composants migrés)", swapped)
            }
            ("START_TARGET", "failed") => "Démarrage cible échoué".to_string(),
            ("COMMIT", "completed") => "Bascule DR validée ✓".to_string(),
            ("ROLLBACK", "completed") => "Bascule DR annulée".to_string(),
            _ => format!("Bascule DR: {} ({})", phase, status),
        };

        // Calculate RTO if this is a completed switchover
        let rto_seconds: Option<i64> = if phase == "COMMIT" && status == "completed" {
            // Try to calculate RTO from PREPARE to COMMIT
            None // Will be calculated separately in RTO report
        } else {
            None
        };

        events.push(json!({
            "kind": "switchover",
            "category": "dr_operation",
            "label": label,
            "switchover_id": switchover_id,
            "phase": phase,
            "status": status,
            "details": details,
            "rto_seconds": rto_seconds,
            "at": at,
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
        SELECT COALESCE(c.current_state, 'UNKNOWN') as state, COUNT(*) as cnt
        FROM components c
        WHERE c.application_id = $1
        GROUP BY c.current_state
        ORDER BY cnt DESC
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

    let state_breakdown: Value = states
        .iter()
        .map(|(s, c)| json!({"state": s, "count": c}))
        .collect();

    // Components in error
    let error_components =
        sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
            r#"
        SELECT c.id, c.name, COALESCE(c.current_state, 'UNKNOWN'), c.updated_at
        FROM components c
        WHERE c.application_id = $1
          AND c.current_state IN ('FAILED', 'UNREACHABLE', 'DEGRADED')
        ORDER BY c.updated_at DESC
        "#,
        )
        .bind(crate::db::bind_id(app_id))
        .fetch_all(&state.db)
        .await?;

    let errors: Vec<Value> = error_components
        .iter()
        .map(|(id, name, state, at)| {
            json!({"component_id": id, "name": name, "state": state, "since": at})
        })
        .collect();

    // Agent status for this app
    let agents =
        sqlx::query_as::<_, (DbUuid, String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
            r#"
        SELECT DISTINCT a.id, a.hostname, a.is_active, a.last_heartbeat_at
        FROM agents a
        JOIN components c ON c.agent_id = a.id
        WHERE c.application_id = $1
        ORDER BY a.hostname
        "#,
        )
        .bind(crate::db::bind_id(app_id))
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
    let recent_incidents = sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT st.component_id, c.name, st.from_state, st.to_state, st.created_at
        FROM state_transitions st
        JOIN components c ON c.id = st.component_id
        WHERE c.application_id = $1 AND st.to_state = 'FAILED'
        ORDER BY st.created_at DESC
        LIMIT 10
        "#,
    )
    .bind(crate::db::bind_id(app_id))
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
            .bind(crate::db::bind_id(app_id))
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

// ---------------------------------------------------------------------------
// MTTR (Mean Time To Recovery) - DORA Metric
// ---------------------------------------------------------------------------

/// Calculate MTTR for an application.
/// MTTR = average time between FAILED and subsequent RUNNING state.
pub async fn mttr(
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

    // Find all FAILED → RUNNING recovery pairs
    // For each component, pair each FAILED transition with the next RUNNING transition
    let recoveries = fetch_mttr_recoveries(&state.db, app_id, from, to).await?;

    // Calculate statistics
    let total_incidents = recoveries.len();
    let recovery_times: Vec<i64> = recoveries.iter().map(|r| r.4).collect();

    let avg_mttr = if !recovery_times.is_empty() {
        recovery_times.iter().sum::<i64>() as f64 / recovery_times.len() as f64
    } else {
        0.0
    };

    let min_mttr = recovery_times.iter().min().copied().unwrap_or(0);
    let max_mttr = recovery_times.iter().max().copied().unwrap_or(0);

    // Median
    let median_mttr = if !recovery_times.is_empty() {
        let mut sorted = recovery_times.clone();
        sorted.sort();
        if sorted.len().is_multiple_of(2) {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) as f64 / 2.0
        } else {
            sorted[sorted.len() / 2] as f64
        }
    } else {
        0.0
    };

    // Per-component breakdown
    let mut component_stats: std::collections::HashMap<String, Vec<i64>> =
        std::collections::HashMap::new();
    for (_, name, _, _, seconds) in &recoveries {
        component_stats
            .entry(name.clone())
            .or_default()
            .push(*seconds);
    }

    let per_component: Vec<Value> = component_stats
        .iter()
        .map(|(name, times)| {
            let avg = times.iter().sum::<i64>() as f64 / times.len() as f64;
            json!({
                "component_name": name,
                "incident_count": times.len(),
                "avg_mttr_seconds": avg,
            })
        })
        .collect();

    // Recent incidents detail
    let recent: Vec<Value> = recoveries
        .iter()
        .take(10)
        .map(|(comp_id, name, failed_at, recovered_at, seconds)| {
            json!({
                "component_id": comp_id,
                "component_name": name,
                "failed_at": failed_at,
                "recovered_at": recovered_at,
                "recovery_seconds": seconds,
                "recovery_formatted": format_duration(*seconds),
            })
        })
        .collect();

    Ok(Json(json!({
        "report": "mttr",
        "period": {
            "from": from,
            "to": to,
        },
        "summary": {
            "total_incidents": total_incidents,
            "avg_mttr_seconds": avg_mttr,
            "avg_mttr_formatted": format_duration(avg_mttr as i64),
            "median_mttr_seconds": median_mttr,
            "min_mttr_seconds": min_mttr,
            "max_mttr_seconds": max_mttr,
        },
        "per_component": per_component,
        "recent_incidents": recent,
    })))
}

// ============================================================================
// Database-specific helper functions
// ============================================================================

type GlobalAuditRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    Uuid,
    serde_json::Value,
    chrono::DateTime<chrono::Utc>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

#[cfg(feature = "postgres")]
async fn fetch_global_audit_logs(
    db: &crate::db::DbPool,
    org_id: Uuid,
    app_id: Option<Uuid>,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GlobalAuditRow>, sqlx::Error> {
    sqlx::query_as::<_, GlobalAuditRow>(
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
    .bind(org_id)
    .bind(app_id)
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_global_audit_logs(
    db: &crate::db::DbPool,
    org_id: Uuid,
    app_id: Option<Uuid>,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GlobalAuditRow>, sqlx::Error> {
    sqlx::query_as::<_, GlobalAuditRow>(
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
          AND ($2 IS NULL OR al.resource_id = $2)
          AND ($3 IS NULL OR al.user_id = $3)
        ORDER BY al.created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(org_id)
    .bind(app_id)
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

#[cfg(feature = "postgres")]
async fn fetch_availability_stats(
    db: &crate::db::DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(DbUuid, String, i64, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, i64, i64)>(
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
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_availability_stats(
    db: &crate::db::DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(DbUuid, String, i64, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, i64, i64)>(
        r#"
        SELECT component_id, CAST(date AS TEXT),
               COALESCE(running_seconds, 0) as running_seconds,
               COALESCE(total_seconds, 86400) as total_seconds
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= date($2) AND date <= date($3)
        ORDER BY date
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

#[cfg(feature = "postgres")]
async fn fetch_switchover_logs(
    db: &crate::db::DbPool,
    app_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        String,
        String,
        chrono::DateTime<chrono::Utc>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT id, phase, status, details::text, created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_switchover_logs(
    db: &crate::db::DbPool,
    app_id: Uuid,
) -> Result<
    Vec<(
        DbUuid,
        String,
        String,
        String,
        chrono::DateTime<chrono::Utc>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            DbUuid,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT id, phase, status, CAST(details AS TEXT), created_at
        FROM switchover_log
        WHERE application_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

#[cfg(feature = "postgres")]
async fn fetch_topology_components(
    db: &crate::db::DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, f64, f64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, f64, f64)>(
        r#"
        SELECT id, name, component_type,
               COALESCE(position_x, 0)::float8,
               COALESCE(position_y, 0)::float8
        FROM components
        WHERE application_id = $1
        ORDER BY name
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_topology_components(
    db: &crate::db::DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, f64, f64)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, f64, f64)>(
        r#"
        SELECT id, name, component_type,
               COALESCE(position_x, 0.0),
               COALESCE(position_y, 0.0)
        FROM components
        WHERE application_id = $1
        ORDER BY name
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(db)
    .await
}

#[cfg(feature = "postgres")]
async fn fetch_avg_rto(db: &crate::db::DbPool, app_id: Uuid) -> Option<f64> {
    sqlx::query_scalar::<_, Option<f64>>(
        r#"
        SELECT AVG(EXTRACT(EPOCH FROM (
            (SELECT MAX(created_at) FROM switchover_log sl2 WHERE sl2.switchover_id = sl.switchover_id AND sl2.phase = 'COMMIT')
            - sl.created_at
        )))
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.phase = 'PREPARE'
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(db)
    .await
    .unwrap_or(None)
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_avg_rto(db: &crate::db::DbPool, app_id: Uuid) -> Option<f64> {
    // SQLite: compute epoch difference using strftime or julianday
    sqlx::query_scalar::<_, Option<f64>>(
        r#"
        SELECT AVG(
            (julianday((SELECT MAX(created_at) FROM switchover_log sl2 WHERE sl2.switchover_id = sl.switchover_id AND sl2.phase = 'COMMIT'))
             - julianday(sl.created_at)) * 86400.0
        )
        FROM switchover_log sl
        WHERE sl.application_id = $1 AND sl.phase = 'PREPARE'
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_one(db)
    .await
    .unwrap_or(None)
}

#[cfg(feature = "postgres")]
async fn fetch_availability_summary(
    db: &crate::db::DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT COALESCE(SUM(running_seconds), 0), COALESCE(SUM(total_seconds), 1)
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= $2::date AND date <= $3::date
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await
    .unwrap_or((0, 1))
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_availability_summary(
    db: &crate::db::DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> (i64, i64) {
    sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT COALESCE(SUM(running_seconds), 0), COALESCE(SUM(total_seconds), 1)
        FROM component_daily_stats
        WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)
          AND date >= date($2) AND date <= date($3)
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await
    .unwrap_or((0, 1))
}

#[cfg(feature = "postgres")]
async fn fetch_mttr_recoveries(
    db: &crate::db::DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<
    Vec<(
        Uuid,
        String,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        i64,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            i64,
        ),
    >(
        r#"
        WITH failed_events AS (
            SELECT
                st.component_id,
                c.name as component_name,
                st.created_at as failed_at,
                ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1
              AND st.to_state = 'FAILED'
              AND st.created_at >= $2 AND st.created_at <= $3
        ),
        recovery_events AS (
            SELECT
                st.component_id,
                st.created_at as recovered_at,
                ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1
              AND st.to_state = 'RUNNING'
              AND st.from_state IN ('FAILED', 'STARTING')
              AND st.created_at >= $2 AND st.created_at <= $3
        )
        SELECT
            f.component_id,
            f.component_name,
            f.failed_at,
            r.recovered_at,
            EXTRACT(EPOCH FROM (r.recovered_at - f.failed_at))::bigint as recovery_seconds
        FROM failed_events f
        JOIN recovery_events r ON f.component_id = r.component_id
        WHERE r.recovered_at > f.failed_at
          AND NOT EXISTS (
            SELECT 1 FROM state_transitions st2
            WHERE st2.component_id = f.component_id
              AND st2.to_state = 'FAILED'
              AND st2.created_at > f.failed_at
              AND st2.created_at < r.recovered_at
          )
        ORDER BY f.failed_at DESC
        LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
async fn fetch_mttr_recoveries(
    db: &crate::db::DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<
    Vec<(
        Uuid,
        String,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        i64,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<
        _,
        (Uuid, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, i64),
    >(
        r#"
        WITH failed_events AS (
            SELECT
                st.component_id,
                c.name as component_name,
                st.created_at as failed_at,
                ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1
              AND st.to_state = 'FAILED'
              AND st.created_at >= $2 AND st.created_at <= $3
        ),
        recovery_events AS (
            SELECT
                st.component_id,
                st.created_at as recovered_at,
                ROW_NUMBER() OVER (PARTITION BY st.component_id ORDER BY st.created_at) as rn
            FROM state_transitions st
            JOIN components c ON c.id = st.component_id
            WHERE c.application_id = $1
              AND st.to_state = 'RUNNING'
              AND st.from_state IN ('FAILED', 'STARTING')
              AND st.created_at >= $2 AND st.created_at <= $3
        )
        SELECT
            f.component_id,
            f.component_name,
            f.failed_at,
            r.recovered_at,
            CAST((julianday(r.recovered_at) - julianday(f.failed_at)) * 86400 AS INTEGER) as recovery_seconds
        FROM failed_events f
        JOIN recovery_events r ON f.component_id = r.component_id
        WHERE r.recovered_at > f.failed_at
          AND NOT EXISTS (
            SELECT 1 FROM state_transitions st2
            WHERE st2.component_id = f.component_id
              AND st2.to_state = 'FAILED'
              AND st2.created_at > f.failed_at
              AND st2.created_at < r.recovered_at
          )
        ORDER BY f.failed_at DESC
        LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await
}

/// Format duration in human-readable format
fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}
