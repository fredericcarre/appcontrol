use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::AppState;
use appcontrol_common::PermissionLevel;

// ---------------------------------------------------------------------------
// GET /apps/:id/topology — Export DAG in JSON, YAML, or DOT format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TopologyQuery {
    pub format: Option<String>, // "json" (default), "yaml", "dot"
}

/// Full topology export: components, dependencies, start/stop ordering.
/// Designed for external tools (schedulers, XL Release, Ansible) to consume.
pub async fn get_topology(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<TopologyQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let format = params.format.as_deref().unwrap_or("json");

    // Fetch application name
    let app_name = crate::repository::misc_queries::get_app_name(&state.db, app_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Fetch components with current state
    let components =
        crate::repository::misc_queries::get_components_for_topology(&state.db, app_id).await?;

    // Build name lookup
    let name_map: HashMap<DbUuid, String> = components
        .iter()
        .map(|(id, name, _, _, _)| (*id, name.clone()))
        .collect();

    // Build DAG and compute levels
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let levels = dag
        .topological_levels()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Fetch dependencies
    let deps = crate::repository::misc_queries::get_deps_for_topology(&state.db, app_id).await?;

    match format {
        "dot" => {
            let mut dot = format!("digraph \"{}\" {{\n  rankdir=LR;\n", app_name);
            dot.push_str("  node [shape=box, style=rounded];\n\n");

            for (id, name, comp_type, host, state) in &components {
                let color = match state.as_str() {
                    "RUNNING" => "green",
                    "STOPPED" => "gray",
                    "FAILED" => "red",
                    "DEGRADED" => "orange",
                    _ => "white",
                };
                let label = if let Some(h) = host {
                    format!("{}\\n({}, {})", name, comp_type, h)
                } else {
                    format!("{}\\n({})", name, comp_type)
                };
                dot.push_str(&format!(
                    "  \"{}\" [label=\"{}\", fillcolor={}, style=\"rounded,filled\"];\n",
                    id, label, color
                ));
            }

            dot.push('\n');
            for (from, to) in &deps {
                let from_name = name_map.get(from).map(|s| s.as_str()).unwrap_or("?");
                let to_name = name_map.get(to).map(|s| s.as_str()).unwrap_or("?");
                dot.push_str(&format!(
                    "  \"{}\" -> \"{}\" [label=\"depends on\"];\n",
                    from_name, to_name
                ));
            }

            dot.push_str("}\n");
            Ok(Json(json!({ "format": "dot", "content": dot })))
        }
        "yaml" => {
            let topology =
                build_topology_structure(app_id, &app_name, &components, &deps, &levels, &name_map);
            let yaml_str =
                serde_yaml::to_string(&topology).map_err(|e| ApiError::Internal(e.to_string()))?;
            Ok(Json(json!({ "format": "yaml", "content": yaml_str })))
        }
        _ => {
            // JSON (default)
            let topology =
                build_topology_structure(app_id, &app_name, &components, &deps, &levels, &name_map);
            Ok(Json(topology))
        }
    }
}

fn build_topology_structure(
    app_id: Uuid,
    app_name: &str,
    components: &[(DbUuid, String, String, Option<String>, String)],
    deps: &[(DbUuid, DbUuid)],
    levels: &[Vec<Uuid>],
    name_map: &HashMap<DbUuid, String>,
) -> Value {
    let comp_list: Vec<Value> = components
        .iter()
        .map(|(id, name, comp_type, host, state)| {
            json!({
                "id": id,
                "name": name,
                "type": comp_type,
                "host": host,
                "current_state": state,
            })
        })
        .collect();

    let dep_list: Vec<Value> = deps
        .iter()
        .map(|(from, to)| {
            json!({
                "from_id": from,
                "from_name": name_map.get(from).map(|s| s.as_str()).unwrap_or("?"),
                "to_id": to,
                "to_name": name_map.get(to).map(|s| s.as_str()).unwrap_or("?"),
                "description": format!("{} depends on {}",
                    name_map.get(from).map(|s| s.as_str()).unwrap_or("?"),
                    name_map.get(to).map(|s| s.as_str()).unwrap_or("?")),
            })
        })
        .collect();

    let start_order: Vec<Value> = levels
        .iter()
        .enumerate()
        .map(|(idx, level)| {
            let names: Vec<&str> = level
                .iter()
                .filter_map(|id| name_map.get(id).map(|s| s.as_str()))
                .collect();
            json!({ "level": idx, "components": names, "parallel": true })
        })
        .collect();

    let mut reversed_levels = levels.to_vec();
    reversed_levels.reverse();
    let stop_order: Vec<Value> = reversed_levels
        .iter()
        .enumerate()
        .map(|(idx, level)| {
            let names: Vec<&str> = level
                .iter()
                .filter_map(|id| name_map.get(id).map(|s| s.as_str()))
                .collect();
            json!({ "level": idx, "components": names, "parallel": true })
        })
        .collect();

    json!({
        "app_id": app_id,
        "app_name": app_name,
        "format": "json",
        "components": comp_list,
        "dependencies": dep_list,
        "start_order": start_order,
        "stop_order": stop_order,
        "total_components": components.len(),
        "total_dependencies": deps.len(),
        "total_levels": levels.len(),
    })
}

// ---------------------------------------------------------------------------
// GET /apps/:id/plan — Execution plan without running it
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PlanQuery {
    pub operation: Option<String>, // "start" (default), "stop"
    pub scope: Option<Uuid>,       // Optional: single component scope
}

/// Returns the execution plan for a given operation without executing it.
/// Richer than dry_run: includes component states, estimated actions per level.
pub async fn get_plan(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<PlanQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let operation = params.operation.as_deref().unwrap_or("start");

    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // If a scope component is specified, build a sub-DAG
    let effective_dag = if let Some(scope_id) = params.scope {
        let mut subset = dag.find_all_dependencies(scope_id);
        subset.insert(scope_id);
        dag.sub_dag(&subset)
    } else {
        dag
    };

    let mut levels = effective_dag
        .topological_levels()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if operation == "stop" {
        levels.reverse();
    }

    // Build plan with component details and predicted actions
    let mut plan_levels = Vec::new();
    let mut total_actions = 0;

    for (idx, level) in levels.iter().enumerate() {
        let mut level_components = Vec::new();

        for &comp_id in level {
            let row =
                crate::repository::misc_queries::get_component_plan_detail(&state.db, comp_id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?;

            if let Some((name, current_state, host, is_optional)) = row {
                let action = predict_action(operation, &current_state);
                if action != "skip" {
                    total_actions += 1;
                }

                level_components.push(json!({
                    "component_id": comp_id,
                    "name": name,
                    "host": host,
                    "current_state": current_state,
                    "predicted_action": action,
                    "is_optional": is_optional,
                }));
            }
        }

        plan_levels.push(json!({
            "level": idx,
            "components": level_components,
            "parallel": true,
        }));
    }

    Ok(Json(json!({
        "app_id": app_id,
        "operation": operation,
        "scope": params.scope,
        "plan": {
            "levels": plan_levels,
            "total_levels": levels.len(),
            "total_actions": total_actions,
        },
    })))
}

fn predict_action(operation: &str, current_state: &str) -> &'static str {
    match operation {
        "start" => match current_state {
            "RUNNING" => "skip",
            "STARTING" => "skip",
            "FAILED" => "restart",
            "DEGRADED" => "restart",
            _ => "start",
        },
        "stop" => match current_state {
            "STOPPED" => "skip",
            "STOPPING" => "skip",
            "UNKNOWN" => "skip",
            _ => "stop",
        },
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// POST /apps/:id/validate-sequence — Validate an external sequence
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ValidateSequenceRequest {
    pub sequence: Vec<String>, // Component names in the proposed start order
    pub operation: Option<String>, // "start" (default) or "stop"
}

/// Validate an externally-defined sequence against AppControl's DAG.
/// Returns conflicts where the proposed order violates dependency constraints.
pub async fn validate_sequence(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<ValidateSequenceRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let operation = body.operation.as_deref().unwrap_or("start");

    // Fetch components and build name→id map
    let components =
        crate::repository::misc_queries::get_component_ids_and_names(&state.db, app_id).await?;

    let name_to_id: HashMap<String, Uuid> = components
        .iter()
        .map(|(id, name)| (name.clone(), id.into_inner()))
        .collect();
    let id_to_name: HashMap<DbUuid, String> = components
        .iter()
        .map(|(id, name)| (*id, name.clone()))
        .collect();

    // Build DAG
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Compute correct order
    let mut levels = dag
        .topological_levels()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if operation == "stop" {
        levels.reverse();
    }

    // Build position map from correct order (lower position = earlier in sequence)
    let mut correct_position: HashMap<DbUuid, usize> = HashMap::new();
    for (pos, level) in levels.iter().enumerate() {
        for &comp_id in level {
            correct_position.insert(DbUuid::from(comp_id), pos);
        }
    }

    // Build position map from proposed sequence
    let mut proposed_position: HashMap<DbUuid, usize> = HashMap::new();
    let mut unknown_names: Vec<String> = Vec::new();
    for (idx, name) in body.sequence.iter().enumerate() {
        if let Some(&comp_id) = name_to_id.get(name) {
            proposed_position.insert(DbUuid::from(comp_id), idx);
        } else {
            unknown_names.push(name.clone());
        }
    }

    // Detect conflicts: for each dependency A depends on B,
    // check that B appears before A in the proposed sequence
    let mut conflicts = Vec::new();
    let deps = crate::repository::misc_queries::get_deps_for_topology(&state.db, app_id).await?;

    for (from, to) in &deps {
        // "from" depends on "to" (to must start before from)
        let from_pos = proposed_position.get(from);
        let to_pos = proposed_position.get(to);

        if let (Some(&fp), Some(&tp)) = (from_pos, to_pos) {
            let violation = if operation == "start" {
                tp >= fp // dependency should start BEFORE the dependent
            } else {
                tp <= fp // dependency should stop AFTER the dependent
            };

            if violation {
                let from_name = id_to_name.get(from).map(|s| s.as_str()).unwrap_or("?");
                let to_name = id_to_name.get(to).map(|s| s.as_str()).unwrap_or("?");

                conflicts.push(json!({
                    "type": "dependency_order_violation",
                    "dependent": from_name,
                    "dependency": to_name,
                    "proposed_dependent_position": fp,
                    "proposed_dependency_position": tp,
                    "message": format!(
                        "'{}' depends on '{}' but {} at position {} while {} is at position {}",
                        from_name, to_name,
                        if operation == "start" { "starts" } else { "stops" },
                        fp,
                        to_name,
                        tp
                    ),
                }));
            }
        }
    }

    // Missing components: in AppControl DAG but not in proposed sequence
    let missing: Vec<&str> = id_to_name
        .iter()
        .filter(|(id, _)| !proposed_position.contains_key(*id))
        .map(|(_, name)| name.as_str())
        .collect();

    // Build expected order
    let expected_order: Vec<Vec<&str>> = levels
        .iter()
        .map(|level| {
            level
                .iter()
                .filter_map(|id| id_to_name.get(id).map(|s| s.as_str()))
                .collect()
        })
        .collect();

    let valid = conflicts.is_empty() && unknown_names.is_empty();

    Ok(Json(json!({
        "valid": valid,
        "operation": operation,
        "proposed_sequence": body.sequence,
        "expected_order": expected_order,
        "conflicts": conflicts,
        "unknown_components": unknown_names,
        "missing_components": missing,
        "total_conflicts": conflicts.len(),
    })))
}

// ---------------------------------------------------------------------------
// GET /apps/:id/dependency-history — Changelog of dependency changes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DependencyHistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Returns the history of dependency changes from config_versions.
pub async fn dependency_history(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<DependencyHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    // Query config_versions for dependency-related changes
    let rows =
        crate::repository::misc_queries::get_dependency_history(&state.db, app_id, limit, offset)
            .await?;

    let total = crate::repository::misc_queries::count_dependency_history(&state.db, app_id)
        .await
        .unwrap_or(0);

    let entries: Vec<Value> = rows
        .iter()
        .map(|(id, change_type, before, after, changed_by, created_at)| {
            json!({
                "id": id,
                "change_type": change_type,
                "before": before,
                "after": after,
                "changed_by": changed_by,
                "created_at": created_at,
            })
        })
        .collect();

    Ok(Json(json!({
        "app_id": app_id,
        "history": entries,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predict_action_start() {
        assert_eq!(predict_action("start", "RUNNING"), "skip");
        assert_eq!(predict_action("start", "STARTING"), "skip");
        assert_eq!(predict_action("start", "STOPPED"), "start");
        assert_eq!(predict_action("start", "FAILED"), "restart");
        assert_eq!(predict_action("start", "DEGRADED"), "restart");
        assert_eq!(predict_action("start", "UNKNOWN"), "start");
    }

    #[test]
    fn test_predict_action_stop() {
        assert_eq!(predict_action("stop", "RUNNING"), "stop");
        assert_eq!(predict_action("stop", "STOPPED"), "skip");
        assert_eq!(predict_action("stop", "STOPPING"), "skip");
        assert_eq!(predict_action("stop", "UNKNOWN"), "skip");
        assert_eq!(predict_action("stop", "DEGRADED"), "stop");
    }
}
