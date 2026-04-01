//! Operation time estimation API.
//!
//! Uses historical data from `command_executions` to estimate how long
//! start/stop/restart operations will take, respecting DAG parallelism.
//!
//! Endpoint:
//!   GET /api/v1/apps/:app_id/estimates?operation=start|stop|restart

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

#[derive(Debug, Deserialize)]
pub struct EstimateQuery {
    /// Operation type: "start", "stop", or "restart"
    pub operation: Option<String>,
}

/// GET /api/v1/apps/:app_id/estimates
///
/// Returns estimated wall-clock time for a full operation, broken down by DAG level.
/// Uses P50 (typical) and P95 (worst case) from historical command_executions.
pub async fn get_estimates(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<EstimateQuery>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let operation = params.operation.as_deref().unwrap_or("start");

    // Build the DAG and compute topological levels
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let levels = dag
        .topological_levels()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // For stop, reverse the levels
    let levels = if operation == "stop" {
        levels.into_iter().rev().collect::<Vec<_>>()
    } else {
        levels
    };

    // Fetch historical stats for all components in this app
    let stats = sqlx::query_as::<_, (DbUuid, String, i32, i32, i32, i32, i32)>(
        "SELECT component_id, command_type, sample_count, avg_ms, p50_ms, p95_ms, max_ms
         FROM component_operation_stats
         WHERE component_id IN (SELECT id FROM components WHERE application_id = $1)",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await;

    // Build lookup: component_id -> (command_type -> stats)
    let mut stats_map: HashMap<DbUuid, HashMap<String, ComponentStats>> = HashMap::new();
    if let Ok(rows) = stats {
        for (comp_id, cmd_type, sample_count, avg_ms, p50_ms, p95_ms, max_ms) in rows {
            stats_map.entry(comp_id).or_default().insert(
                cmd_type,
                ComponentStats {
                    sample_count,
                    avg_ms,
                    p50_ms,
                    p95_ms,
                    max_ms,
                },
            );
        }
    }

    // For each DAG level, estimate wall-clock time:
    // - Components at same level run in parallel → take the MAX
    // - Levels are sequential → SUM across levels
    let mut level_details = Vec::new();
    let mut total_p50_ms: i64 = 0;
    let mut total_p95_ms: i64 = 0;
    let mut total_components = 0;
    let mut components_with_data = 0;

    let cmd_type = match operation {
        "stop" => "stop",
        "restart" => "start", // restart ≈ stop + start, simplified to start time
        _ => "start",
    };

    for (level_idx, level) in levels.iter().enumerate() {
        let mut level_max_p50: i64 = 0;
        let mut level_max_p95: i64 = 0;
        let mut level_components = Vec::new();

        for &comp_id in level {
            total_components += 1;

            let name = sqlx::query_scalar::<_, String>("SELECT name FROM components WHERE id = $1")
                .bind(crate::db::bind_id(comp_id))
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| comp_id.to_string());

            if let Some(type_map) = stats_map.get(&comp_id) {
                if let Some(s) = type_map.get(cmd_type) {
                    components_with_data += 1;
                    level_max_p50 = level_max_p50.max(s.p50_ms as i64);
                    level_max_p95 = level_max_p95.max(s.p95_ms as i64);

                    level_components.push(json!({
                        "component_id": comp_id,
                        "name": name,
                        "p50_ms": s.p50_ms,
                        "p95_ms": s.p95_ms,
                        "avg_ms": s.avg_ms,
                        "sample_count": s.sample_count,
                        "confidence": confidence_level(s.sample_count),
                    }));
                    continue;
                }
            }

            // No historical data — use timeout as worst case
            let timeout_ms = sqlx::query_scalar::<_, i32>(
                "SELECT start_timeout_seconds FROM components WHERE id = $1",
            )
            .bind(crate::db::bind_id(comp_id))
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or(120)
                * 1000;

            level_components.push(json!({
                "component_id": comp_id,
                "name": name,
                "p50_ms": null,
                "p95_ms": null,
                "timeout_ms": timeout_ms,
                "sample_count": 0,
                "confidence": "none",
            }));
        }

        total_p50_ms += level_max_p50;
        total_p95_ms += level_max_p95;

        level_details.push(json!({
            "level": level_idx,
            "parallel_components": level_components.len(),
            "estimated_p50_ms": level_max_p50,
            "estimated_p95_ms": level_max_p95,
            "components": level_components,
        }));
    }

    // If doing restart, double the time (stop + start)
    if operation == "restart" {
        total_p50_ms *= 2;
        total_p95_ms *= 2;
    }

    let overall_confidence = if components_with_data == 0 {
        "none"
    } else if components_with_data < total_components / 2 {
        "low"
    } else if components_with_data == total_components {
        "high"
    } else {
        "medium"
    };

    Ok(Json(json!({
        "app_id": app_id,
        "operation": operation,
        "estimate": {
            "typical_ms": total_p50_ms,
            "typical_human": format_duration(total_p50_ms),
            "worst_case_ms": total_p95_ms,
            "worst_case_human": format_duration(total_p95_ms),
        },
        "confidence": overall_confidence,
        "data_coverage": {
            "components_with_history": components_with_data,
            "total_components": total_components,
        },
        "levels": level_details,
    })))
}

#[allow(dead_code)]
struct ComponentStats {
    sample_count: i32,
    avg_ms: i32,
    p50_ms: i32,
    p95_ms: i32,
    max_ms: i32,
}

fn confidence_level(sample_count: i32) -> &'static str {
    if sample_count >= 10 {
        "high"
    } else if sample_count >= 3 {
        "medium"
    } else {
        "low"
    }
}

fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{}m{}s", mins, secs)
    }
}
