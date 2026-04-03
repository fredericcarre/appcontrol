//! Incidents, switchovers, and DRP exercise report endpoints.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::repository::report_queries as repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

pub async fn incidents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let from = params.from.unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let incidents_data = repo::fetch_incidents(&state.db, app_id, from, to).await?;

    let data: Vec<Value> = incidents_data.iter().map(|(cid, name, state, at)| {
        json!({"component_id": cid, "component_name": name, "state": state, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "incidents", "data": data })))
}

pub async fn switchovers(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(_params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let logs = repo::fetch_switchover_logs(&state.db, app_id).await?;

    let data: Vec<Value> = logs.iter().map(|(id, phase, status, details, at)| {
        json!({"id": id, "phase": phase, "status": status, "details": details, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "switchovers", "data": data })))
}

/// DRP Exercise Report - Detailed switchover history for DORA compliance
pub async fn drp_report(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View { return Err(ApiError::Forbidden); }

    let app_info = repo::get_app_info_for_report(&state.db, app_id).await?;
    let (app_name, _site_id, site_name) = app_info.unwrap_or(("Unknown".to_string(), None, None));

    let logs = repo::get_switchover_log_entries(&state.db, app_id).await?;
    let sites: std::collections::HashMap<DbUuid, String> = repo::get_all_sites(&state.db).await?.into_iter().collect();

    // Group by switchover_id
    type PhaseEntry = (String, String, Value, chrono::DateTime<chrono::Utc>);
    let mut switchovers_map: std::collections::HashMap<DbUuid, Vec<PhaseEntry>> = std::collections::HashMap::new();

    for (switchover_id, phase, status, details, created_at) in logs {
        switchovers_map.entry(switchover_id).or_default().push((phase, status, details, created_at));
    }

    let mut switchover_list: Vec<Value> = Vec::new();

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
            if started_at.is_none() { started_at = Some(*at); }

            if phase == "PREPARE" && status == "in_progress" {
                if let Some(tid) = details["target_site_id"].as_str().and_then(|s| s.parse::<Uuid>().ok()) {
                    target_site_id = Some(tid);
                    target_site = sites.get(&tid).cloned();
                }
                if initiated_by_user_id.is_none() {
                    initiated_by_user_id = details["initiated_by"].as_str().map(String::from);
                }
            }

            if phase == "START_TARGET" && status == "completed" {
                if source_site.is_none() { source_site = details["source_profile"].as_str().map(String::from); }
                if target_site.is_none() { target_site = details["target_profile"].as_str().map(String::from); }
            }

            if let Some(count) = details["components_impacted"].as_i64() { components_count = Some(count); }
            if let Some(count) = details["components_swapped"].as_i64() { components_count = Some(count); }

            if status == "in_progress" { current_phase_start = Some(*at); }
            else if status == "completed" || status == "failed" {
                let phase_started = current_phase_start.unwrap_or(*at);
                let duration_ms = (*at - phase_started).num_milliseconds();
                phase_details.push(json!({
                    "phase": phase, "status": status, "started_at": phase_started,
                    "completed_at": at, "duration_ms": duration_ms, "details": details
                }));
                current_phase_start = None;
            }

            if phase == "COMMIT" && status == "completed" { final_status = "completed".to_string(); completed_at = Some(*at); }
            else if phase == "ROLLBACK" && status == "completed" { final_status = "rolled_back".to_string(); completed_at = Some(*at); }
            else if status == "failed" { final_status = "failed".to_string(); completed_at = Some(*at); }
        }

        let rto_seconds = match (started_at, completed_at) {
            (Some(start), Some(end)) => Some((end - start).num_seconds()),
            _ => None,
        };

        let component_sequence = if let (Some(start), Some(end)) = (started_at, completed_at) {
            let transitions = repo::get_transitions_in_range(&state.db, app_id, start, end, &["STOPPED", "RUNNING"]).await.unwrap_or_default();
            let seq: Vec<Value> = transitions.into_iter()
                .map(|(name, from, to, at)| json!({"component": name, "from_state": from, "to_state": to, "at": at}))
                .collect();
            Some(seq)
        } else { None };

        let initiated_by_email: Option<String> = if let Some(ref user_id_str) = initiated_by_user_id {
            if let Ok(user_id) = uuid::Uuid::parse_str(user_id_str) {
                repo::get_user_email(&state.db, user_id).await
            } else { None }
        } else { None };

        let commands_executed = if let (Some(start), Some(end)) = (started_at, completed_at) {
            let cmds = repo::get_commands_in_range(&state.db, app_id, start, end).await.unwrap_or_default();
            let cmd_list: Vec<Value> = cmds.into_iter()
                .map(|(to_state, comp_name, start_cmd, stop_cmd, agent, gateway, at)| {
                    let (action, command) = match to_state.as_str() {
                        "STARTING" => ("start", start_cmd),
                        "STOPPING" => ("stop", stop_cmd),
                        _ => ("unknown", None),
                    };
                    json!({"action": action, "component": comp_name, "command": command, "agent": agent, "gateway": gateway, "at": at})
                }).collect();
            Some(cmd_list)
        } else { None };

        switchover_list.push(json!({
            "switchover_id": switchover_id, "started_at": started_at, "completed_at": completed_at,
            "rto_seconds": rto_seconds, "status": final_status, "initiated_by": initiated_by_email,
            "source_site": source_site, "target_site": target_site, "target_site_id": target_site_id,
            "components_count": components_count, "phases": phase_details,
            "component_sequence": component_sequence, "commands_executed": commands_executed
        }));
    }

    switchover_list.sort_by(|a, b| {
        let a_time = a["started_at"].as_str().unwrap_or("");
        let b_time = b["started_at"].as_str().unwrap_or("");
        b_time.cmp(a_time)
    });

    let components = repo::fetch_topology_components(&state.db, app_id).await.unwrap_or_default();
    let component_list: Vec<Value> = components.into_iter()
        .map(|(id, name, comp_type, x, y)| json!({"id": id, "name": name, "type": comp_type, "position": {"x": x, "y": y}}))
        .collect();

    let dependencies = repo::get_app_dependencies(&state.db, app_id).await.unwrap_or_default();
    let edge_list: Vec<Value> = dependencies.into_iter()
        .map(|(source, target)| json!({"source": source, "target": target})).collect();

    Ok(Json(json!({
        "report": "drp_exercises",
        "application": {"id": app_id, "name": app_name, "current_site": site_name},
        "total_exercises": switchover_list.len(),
        "exercises": switchover_list,
        "topology": {"nodes": component_list, "edges": edge_list},
        "generated_at": chrono::Utc::now()
    })))
}
