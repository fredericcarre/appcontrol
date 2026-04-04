//! Discovery draft CRUD and apply operations.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::IntArray;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::repository::discovery_queries as repo;
use crate::AppState;

// ============================================================================
// List / Get drafts
// ============================================================================

/// List discovery drafts.
pub async fn list_drafts(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = repo::list_drafts(&state.db, *user.organization_id).await?;

    let drafts: Vec<Value> = rows
        .iter()
        .map(|(id, name, status, inferred_at)| {
            json!({ "id": id, "name": name, "status": status, "inferred_at": inferred_at })
        })
        .collect();

    Ok(Json(json!({ "drafts": drafts })))
}

/// Get full draft details: components + dependencies.
pub async fn get_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let draft = repo::get_draft_header(&state.db, draft_id).await?;
    let (id, name, status, inferred_at) = draft.ok_or(ApiError::NotFound)?;

    let components = repo::get_draft_components(&state.db, draft_id).await?;
    let deps = repo::get_draft_dependencies(&state.db, draft_id).await?;

    let comp_json: Vec<Value> = components
        .iter()
        .map(
            |(
                cid,
                comp_name,
                proc,
                host,
                ctype,
                meta,
                check,
                start,
                stop,
                restart,
                confidence,
                source,
                configs,
                logs,
                matched_svc,
            )| {
                json!({
                    "id": cid, "name": comp_name, "process_name": proc,
                    "host": host, "component_type": ctype, "metadata": meta,
                    "check_cmd": check, "start_cmd": start, "stop_cmd": stop,
                    "restart_cmd": restart, "command_confidence": confidence,
                    "command_source": source, "config_files": configs,
                    "log_files": logs, "matched_service": matched_svc,
                })
            },
        )
        .collect();

    let dep_json: Vec<Value> = deps
        .iter()
        .map(|(dep_id, from, to, via)| {
            json!({ "id": dep_id, "from_component": from, "to_component": to, "inferred_via": via })
        })
        .collect();

    Ok(Json(json!({
        "id": id, "name": name, "status": status, "inferred_at": inferred_at,
        "components": comp_json, "dependencies": dep_json,
    })))
}

// ============================================================================
// Create draft
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateDraftRequest {
    pub name: String,
    pub components: Vec<DraftComponentInput>,
    pub dependencies: Vec<DraftDependencyInput>,
}

#[derive(Debug, Deserialize)]
pub struct DraftComponentInput {
    pub temp_id: String,
    pub name: String,
    pub process_name: Option<String>,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub listening_ports: Vec<i32>,
    pub component_type: String,
    #[serde(default)]
    pub check_cmd: Option<String>,
    #[serde(default)]
    pub start_cmd: Option<String>,
    #[serde(default)]
    pub stop_cmd: Option<String>,
    #[serde(default)]
    pub restart_cmd: Option<String>,
    #[serde(default)]
    pub command_confidence: Option<String>,
    #[serde(default)]
    pub command_source: Option<String>,
    #[serde(default)]
    pub config_files: Option<serde_json::Value>,
    #[serde(default)]
    pub log_files: Option<serde_json::Value>,
    #[serde(default)]
    pub matched_service: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DraftDependencyInput {
    pub from_temp_id: String,
    pub to_temp_id: String,
    pub inferred_via: String,
}

pub async fn create_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateDraftRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let org_id = repo::get_user_org_id(&state.db, *user.user_id).await?;

    log_action(
        &state.db, user.user_id, "discovery_create_draft", "discovery", Uuid::nil(),
        json!({ "name": &body.name, "components": body.components.len(), "dependencies": body.dependencies.len() }),
    ).await?;

    let draft_id = Uuid::new_v4();
    repo::insert_draft(&state.db, draft_id, org_id, &body.name).await?;

    let mut temp_to_real: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::new();

    for comp in &body.components {
        let comp_id = Uuid::new_v4();
        temp_to_real.insert(comp.temp_id.clone(), comp_id);

        repo::insert_draft_component(
            &state.db,
            comp_id,
            draft_id,
            comp.agent_id,
            &comp.name,
            &comp.process_name,
            &comp.host,
            IntArray::from(comp.listening_ports.clone()),
            &comp.component_type,
            &comp.check_cmd,
            &comp.start_cmd,
            &comp.stop_cmd,
            &comp.restart_cmd,
            comp.command_confidence.as_deref().unwrap_or("low"),
            &comp.command_source,
            comp.config_files.as_ref().unwrap_or(&json!([])),
            comp.log_files.as_ref().unwrap_or(&json!([])),
            &comp.matched_service,
        )
        .await?;
    }

    let mut dep_count = 0u32;
    for dep in &body.dependencies {
        if let (Some(&from_id), Some(&to_id)) = (
            temp_to_real.get(&dep.from_temp_id),
            temp_to_real.get(&dep.to_temp_id),
        ) {
            repo::insert_draft_dependency(&state.db, draft_id, from_id, to_id, &dep.inferred_via)
                .await?;
            dep_count += 1;
        }
    }

    Ok(Json(json!({
        "draft_id": draft_id, "name": body.name,
        "components_created": body.components.len(),
        "dependencies_created": dep_count, "status": "pending",
    })))
}

// ============================================================================
// Update draft components/dependencies
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UpdateComponentsRequest {
    pub components: Vec<UpdateComponentInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateComponentInput {
    pub id: Uuid,
    pub name: String,
    pub component_type: String,
    #[serde(default)]
    pub check_cmd: Option<String>,
    #[serde(default)]
    pub start_cmd: Option<String>,
    #[serde(default)]
    pub stop_cmd: Option<String>,
    #[serde(default)]
    pub restart_cmd: Option<String>,
}

pub async fn update_draft_components(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
    Json(body): Json<UpdateComponentsRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let status = repo::get_draft_status(&state.db, draft_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if status != "pending" {
        return Err(ApiError::Conflict(format!("Draft is already {}", status)));
    }

    let mut updated = 0u32;
    for comp in &body.components {
        let rows = repo::update_draft_component(
            &state.db,
            comp.id,
            draft_id,
            &comp.name,
            &comp.component_type,
            &comp.check_cmd,
            &comp.start_cmd,
            &comp.stop_cmd,
            &comp.restart_cmd,
        )
        .await?;
        updated += rows as u32;
    }

    Ok(Json(json!({ "updated": updated, "draft_id": draft_id })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateDependenciesRequest {
    pub add: Vec<AddDependencyInput>,
    pub remove: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AddDependencyInput {
    pub from_component: Uuid,
    pub to_component: Uuid,
    pub inferred_via: String,
}

pub async fn update_draft_dependencies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
    Json(body): Json<UpdateDependenciesRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let status = repo::get_draft_status(&state.db, draft_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if status != "pending" {
        return Err(ApiError::Conflict(format!("Draft is already {}", status)));
    }

    let mut removed = 0u32;
    for dep_id in &body.remove {
        let rows = repo::delete_draft_dependency(&state.db, *dep_id, draft_id).await?;
        removed += rows as u32;
    }

    let mut added = 0u32;
    for dep in &body.add {
        repo::insert_draft_dependency(
            &state.db,
            draft_id,
            dep.from_component,
            dep.to_component,
            &dep.inferred_via,
        )
        .await?;
        added += 1;
    }

    Ok(Json(
        json!({ "draft_id": draft_id, "added": added, "removed": removed }),
    ))
}

// ============================================================================
// Apply draft
// ============================================================================

/// Apply a draft: create a real application from the discovery draft.
pub async fn apply_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let draft = repo::get_draft_for_apply(&state.db, draft_id).await?;
    let (_, org_id, name, status) = draft.ok_or(ApiError::NotFound)?;
    if status != "pending" {
        return Err(ApiError::Conflict(format!("Draft is already {}", status)));
    }

    log_action(
        &state.db,
        user.user_id,
        "discovery_apply",
        "discovery_draft",
        draft_id,
        json!({ "name": &name }),
    )
    .await?;

    let site_id = repo::get_first_site_id(&state.db, org_id).await?;
    let site_id = site_id.ok_or(ApiError::Validation(
        "Organization has no sites -- create a site first".to_string(),
    ))?;

    let app_id = Uuid::new_v4();
    repo::create_app_from_draft(&state.db, app_id, org_id, site_id, &name).await?;

    let draft_comps = repo::get_draft_comps_for_apply(&state.db, draft_id).await?;

    let mut draft_to_real: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();

    for (
        draft_comp_id,
        comp_name,
        _process_name,
        host,
        comp_type,
        agent_id,
        check_cmd,
        start_cmd,
        stop_cmd,
        config_files,
        log_files,
    ) in &draft_comps
    {
        let real_comp_id = Uuid::new_v4();
        let agent_uuid = agent_id.map(|a| *a);
        repo::insert_component_from_draft(
            &state.db,
            real_comp_id,
            app_id,
            comp_name,
            comp_type,
            host,
            &agent_uuid,
            check_cmd,
            start_cmd,
            stop_cmd,
        )
        .await?;
        draft_to_real.insert(**draft_comp_id, real_comp_id);

        // Create custom commands for log files
        if let Some(logs) = log_files.as_array() {
            for log_entry in logs {
                if let Some(log_path) = log_entry.get("path").and_then(|p| p.as_str()) {
                    let _ = repo::insert_component_command(
                        &state.db,
                        real_comp_id,
                        &format!("Logs: {}", log_path.rsplit('/').next().unwrap_or(log_path)),
                        &format!("tail -100 {}", log_path),
                    )
                    .await;
                }
            }
        }

        // Create custom commands for config files
        if let Some(configs) = config_files.as_array() {
            for config_entry in configs {
                if let Some(config_path) = config_entry.get("path").and_then(|p| p.as_str()) {
                    let _ = repo::insert_component_command(
                        &state.db,
                        real_comp_id,
                        &format!(
                            "Config: {}",
                            config_path.rsplit('/').next().unwrap_or(config_path)
                        ),
                        &format!("cat {}", config_path),
                    )
                    .await;
                }
            }
        }
    }

    // Create dependencies
    let draft_deps = repo::get_draft_deps_for_apply(&state.db, draft_id).await?;

    let mut dep_count = 0u32;
    for (from_draft, to_draft) in &draft_deps {
        if let (Some(&from_real), Some(&to_real)) =
            (draft_to_real.get(&**from_draft), draft_to_real.get(&**to_draft))
        {
            repo::insert_dependency(&state.db, app_id, from_real, to_real).await?;
            dep_count += 1;
        }
    }

    repo::mark_draft_applied(&state.db, draft_id, app_id).await?;

    Ok(Json(json!({
        "application_id": app_id, "name": name, "mode": "advisory",
        "components_created": draft_comps.len(), "dependencies_created": dep_count,
    })))
}
