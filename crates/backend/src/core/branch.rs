use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

use super::dag;

#[derive(Debug, thiserror::Error)]
pub enum BranchError {
    #[error("DAG error: {0}")]
    Dag(#[from] dag::DagError),
    #[error("Database error: {0}")]
    Database(String),
}

/// Detect the "error branch" — the subgraph of FAILED components and their dependents
/// that need to be restarted.
pub async fn detect_error_branch(
    pool: &crate::db::DbPool,
    app_id: Uuid,
    failed_component_id: Uuid,
) -> Result<Value, BranchError> {
    let dag = dag::build_dag(pool, app_id).await?;

    // Find all components that depend on the failed one (transitively)
    let mut affected = HashSet::new();
    affected.insert(failed_component_id);

    // Build reverse adjacency (who depends on whom)
    let mut dependents: std::collections::HashMap<Uuid, HashSet<Uuid>> =
        std::collections::HashMap::new();
    for (&node, deps) in &dag.adjacency {
        for &dep in deps {
            dependents.entry(dep).or_default().insert(node);
        }
    }

    // BFS from failed component through dependents
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(failed_component_id);

    while let Some(current) = queue.pop_front() {
        if let Some(deps) = dependents.get(&current) {
            for &dep in deps {
                if affected.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }
    }

    // Get component details
    let mut branch_components = Vec::new();
    let app_repo = crate::repository::apps::create_app_repository(pool.clone());
    for &comp_id in &affected {
        let name = app_repo.get_component_name(comp_id)
            .await
            .map_err(|e| BranchError::Database(e.to_string()))?
            .unwrap_or_default();

        let state = crate::core::fsm::get_current_state(pool, comp_id)
            .await
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        branch_components.push(serde_json::json!({
            "component_id": comp_id,
            "name": name,
            "current_state": state,
            "is_root_failure": comp_id == failed_component_id,
        }));
    }

    Ok(serde_json::json!({
        "root_component_id": failed_component_id,
        "affected_components": branch_components,
        "total_affected": affected.len(),
    }))
}
