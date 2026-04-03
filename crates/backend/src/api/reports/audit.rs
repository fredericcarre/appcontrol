//! Global audit log and activity feed endpoints.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::repository::report_queries as repo;
use crate::AppState;
use appcontrol_common::PermissionLevel;

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

    let logs = repo::fetch_global_audit_logs(
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
                let target_name = app_name
                    .clone()
                    .or_else(|| comp_name.clone())
                    .or_else(|| agent_hostname.clone())
                    .or_else(|| gateway_name.clone());

                let mut enriched_details = details.clone();
                if let Some(name) = &target_name {
                    if let Some(obj) = enriched_details.as_object_mut() {
                        obj.insert("name".to_string(), json!(name));
                    }
                }

                json!({
                    "id": id, "user_email": user_email, "action": action,
                    "target_type": target_type, "target_id": target_id,
                    "target_name": target_name, "details": enriched_details, "created_at": at
                })
            },
        )
        .collect();

    Ok(Json(data))
}

pub async fn audit(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<super::ReportQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let from = params
        .from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(30));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let logs = repo::fetch_audit_log(&state.db, app_id, from, to).await?;

    let data: Vec<Value> = logs.iter().map(|(id, uid, action, rtype, at)| {
        json!({"id": id, "user_id": uid, "action": action, "resource_type": rtype, "at": at})
    }).collect();

    Ok(Json(json!({ "report": "audit", "data": data })))
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

    let transitions = repo::fetch_activity_transitions(&state.db, app_id, cursor, limit).await?;
    let actions = repo::fetch_activity_actions(&state.db, app_id, cursor, limit).await?;
    let component_actions =
        repo::fetch_activity_component_actions(&state.db, app_id, cursor, limit).await?;
    let commands = repo::fetch_activity_commands(&state.db, app_id, cursor, limit).await?;
    let switchovers = repo::fetch_activity_switchovers(&state.db, app_id, cursor, limit).await?;

    let mut events: Vec<Value> = Vec::new();

    for (comp_id, comp_name, from, to, trigger, at) in &transitions {
        let category = categorize_event("", Some(to.as_str()));
        let label = match to.as_str() {
            "FAILED" => format!("{} en echec", comp_name),
            "RUNNING" => format!("{} operationnel", comp_name),
            "STOPPED" => format!("{} arrete", comp_name),
            "STARTING" => format!("{} en demarrage", comp_name),
            "STOPPING" => format!("{} en arret", comp_name),
            "DEGRADED" => format!("{} degrade", comp_name),
            "UNREACHABLE" => format!("{} injoignable", comp_name),
            _ => format!("{} -> {}", comp_name, to),
        };
        events.push(json!({
            "kind": "state_change", "category": category, "label": label,
            "component_id": comp_id, "component_name": comp_name,
            "from_state": from, "to_state": to, "trigger": trigger, "at": at,
        }));
    }

    for (_uid, email, action, details, at, status, error_message) in &actions {
        let label = format_action_label(action, details, None);
        let category = categorize_event(action, None);
        events.push(json!({
            "kind": "user_action", "category": category, "label": label,
            "user": email, "action": action, "details": details, "at": at,
            "status": status, "error_message": error_message,
        }));
    }

    for (_uid, email, action, comp_name, details, at, status, error_message) in &component_actions {
        let label = format_action_label(action, details, Some(comp_name));
        let category = categorize_event(action, None);
        events.push(json!({
            "kind": "user_action", "category": category, "label": label,
            "user": email, "action": action, "component_name": comp_name,
            "details": details, "at": at, "status": status, "error_message": error_message,
        }));
    }

    for (req_id, comp_id, comp_name, cmd_type, exit_code, duration, dispatched, completed) in
        &commands
    {
        let label = format!("Commande {} sur {}", cmd_type, comp_name);
        events.push(json!({
            "kind": "command", "category": "planned_operation", "label": label,
            "request_id": req_id, "component_id": comp_id, "component_name": comp_name,
            "command_type": cmd_type, "exit_code": exit_code, "duration_ms": duration,
            "dispatched_at": dispatched, "completed_at": completed,
            "at": completed.unwrap_or(*dispatched),
        }));
    }

    for (switchover_id, phase, status, details, at) in &switchovers {
        let label = match (phase.as_str(), status.as_str()) {
            ("PREPARE", "in_progress") => {
                let mode = details["mode"].as_str().unwrap_or("FULL");
                if mode == "SELECTIVE" {
                    "Bascule DR partielle initiee".to_string()
                } else {
                    "Bascule DR complete initiee".to_string()
                }
            }
            ("VALIDATE", "completed") => "Validation DR reussie".to_string(),
            ("VALIDATE", "failed") => "Validation DR echouee".to_string(),
            ("STOP_SOURCE", "completed") => {
                let count = details["components_impacted"]
                    .as_i64()
                    .or_else(|| details["components_still_running"].as_i64().map(|_| 0));
                match count {
                    Some(n) => format!("Arret source termine ({} composants)", n),
                    None => "Arret source termine".to_string(),
                }
            }
            ("STOP_SOURCE", "failed") => "Arret source echoue".to_string(),
            ("SYNC", "completed") => "Synchronisation terminee".to_string(),
            ("START_TARGET", "completed") => {
                let swapped = details["components_swapped"].as_i64().unwrap_or(0);
                format!("Demarrage cible termine ({} composants migres)", swapped)
            }
            ("START_TARGET", "failed") => "Demarrage cible echoue".to_string(),
            ("COMMIT", "completed") => "Bascule DR validee".to_string(),
            ("ROLLBACK", "completed") => "Bascule DR annulee".to_string(),
            _ => format!("Bascule DR: {} ({})", phase, status),
        };

        events.push(json!({
            "kind": "switchover", "category": "dr_operation", "label": label,
            "switchover_id": switchover_id, "phase": phase, "status": status,
            "details": details, "rto_seconds": null, "at": at,
        }));
    }

    events.sort_by(|a, b| {
        let at_a = a["at"].as_str().unwrap_or("");
        let at_b = b["at"].as_str().unwrap_or("");
        at_b.cmp(at_a)
    });

    events.truncate(limit as usize);

    Ok(Json(json!({ "events": events })))
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn format_action_label(action: &str, details: &Value, component_name: Option<&str>) -> String {
    match action {
        "start_app" | "start_application" => "Demarrage de l'application".to_string(),
        "stop_app" | "stop_application" => "Arret de l'application".to_string(),
        "start_switchover" => {
            let mode = details["mode"].as_str().unwrap_or("FULL");
            let target = details["target_site"]
                .as_str()
                .or_else(|| details["target_site_id"].as_str())
                .unwrap_or("?");
            if mode == "SELECTIVE" {
                format!("Bascule DR partielle vers {}", target)
            } else {
                format!("Bascule DR complete vers {}", target)
            }
        }
        "switchover_next_phase" => "Progression de la bascule DR".to_string(),
        "switchover_rollback" => "Annulation de la bascule DR".to_string(),
        "switchover_commit" => "Validation de la bascule DR".to_string(),
        "diagnose" | "start_diagnose" => "Diagnostic lance".to_string(),
        "rebuild" | "start_rebuild" => "Reconstruction lancee".to_string(),
        "start_component" => component_name
            .map(|n| format!("Demarrage de {}", n))
            .unwrap_or_else(|| "Demarrage d'un composant".to_string()),
        "stop_component" => component_name
            .map(|n| format!("Arret de {}", n))
            .unwrap_or_else(|| "Arret d'un composant".to_string()),
        "execute_command" => {
            let cmd = details["command_name"].as_str().unwrap_or("commande");
            component_name
                .map(|n| format!("Execution de '{}' sur {}", cmd, n))
                .unwrap_or_else(|| format!("Execution de '{}'", cmd))
        }
        "grant_permission" => "Attribution de permissions".to_string(),
        "revoke_permission" => "Revocation de permissions".to_string(),
        "create_share_link" => "Creation d'un lien de partage".to_string(),
        "update_component" => component_name
            .map(|n| format!("Modification de {}", n))
            .unwrap_or_else(|| "Modification d'un composant".to_string()),
        "create_component" => "Creation d'un composant".to_string(),
        "delete_component" => "Suppression d'un composant".to_string(),
        _ => action.replace('_', " ").to_string(),
    }
}

fn categorize_event(action: &str, to_state: Option<&str>) -> &'static str {
    match (action, to_state) {
        (_, Some("FAILED")) => "incident",
        (_, Some("UNREACHABLE")) => "incident",
        (_, Some("RUNNING")) => "recovery",
        ("start_switchover", _) => "dr_operation",
        ("switchover_next_phase", _) | ("switchover_commit", _) => "dr_operation",
        ("start_app", _) | ("stop_app", _) => "planned_operation",
        ("start_component", _) | ("stop_component", _) => "planned_operation",
        ("diagnose", _) | ("rebuild", _) => "maintenance",
        ("update_component", _) | ("create_component", _) | ("delete_component", _) => {
            "config_change"
        }
        _ => "other",
    }
}
