use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RebuildError {
    #[error("Component {0} is rebuild-protected")]
    ProtectedComponent(Uuid),
    #[error("Database error: {0}")]
    Database(String),
    #[error("DAG error: {0}")]
    Dag(#[from] super::dag::DagError),
}

/// Build a rebuild plan. Checks protected components and resolves rebuild commands.
pub async fn build_rebuild_plan(
    pool: &sqlx::PgPool,
    app_id: Uuid,
    component_ids: Option<&[Uuid]>,
) -> Result<Value, RebuildError> {
    // Get target components
    let targets = if let Some(ids) = component_ids {
        let mut targets = Vec::new();
        for &id in ids {
            let row = sqlx::query_as::<_, (Uuid, String, bool, Option<String>, Option<String>, Option<Uuid>)>(
                r#"
                SELECT id, name, rebuild_protected,
                       COALESCE(
                           (SELECT so.rebuild_cmd_override FROM site_overrides so WHERE so.component_id = c.id LIMIT 1),
                           rebuild_cmd
                       ) as effective_rebuild_cmd,
                       rebuild_infra_cmd,
                       rebuild_agent_id
                FROM components c WHERE id = $1
                "#,
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| RebuildError::Database(e.to_string()))?;

            if let Some(r) = row {
                targets.push(r);
            }
        }
        targets
    } else {
        sqlx::query_as::<_, (Uuid, String, bool, Option<String>, Option<String>, Option<Uuid>)>(
            r#"
            SELECT id, name, rebuild_protected,
                   COALESCE(
                       (SELECT so.rebuild_cmd_override FROM site_overrides so WHERE so.component_id = c.id LIMIT 1),
                       rebuild_cmd
                   ) as effective_rebuild_cmd,
                   rebuild_infra_cmd,
                   rebuild_agent_id
            FROM components c WHERE application_id = $1
            "#,
        )
        .bind(app_id)
        .fetch_all(pool)
        .await
        .map_err(|e| RebuildError::Database(e.to_string()))?
    };

    // Check for protected components
    for (id, _name, protected, _, _, _) in &targets {
        if *protected {
            return Err(RebuildError::ProtectedComponent(*id));
        }
    }

    // Build DAG order for rebuild
    let dag = super::dag::build_dag(pool, app_id).await?;
    let levels = dag.topological_levels()?;

    let mut plan_levels = Vec::new();
    for level in &levels {
        let mut level_components = Vec::new();
        for &comp_id in level {
            if let Some((_, name, _, rebuild_cmd, infra_cmd, bastion_agent)) =
                targets.iter().find(|(id, _, _, _, _, _)| *id == comp_id)
            {
                level_components.push(serde_json::json!({
                    "component_id": comp_id,
                    "name": name,
                    "rebuild_cmd": rebuild_cmd,
                    "rebuild_infra_cmd": infra_cmd,
                    "rebuild_agent_id": bastion_agent,
                }));
            }
        }
        if !level_components.is_empty() {
            plan_levels.push(level_components);
        }
    }

    Ok(serde_json::json!({
        "levels": plan_levels,
        "total_components": targets.len(),
    }))
}
