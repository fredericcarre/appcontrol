use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct WaitQuery {
    pub timeout: Option<u64>,
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<Option<StartRequest>>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(StatusCode::FORBIDDEN);
    }

    let dry_run = body.and_then(|b| b.dry_run).unwrap_or(false);

    log_action(
        &state.db,
        user.user_id,
        "orchestration_start",
        "application",
        app_id,
        json!({"dry_run": dry_run}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let plan = crate::core::sequencer::build_start_plan(&state.db, app_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !dry_run {
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::core::sequencer::execute_start(&state_clone, app_id).await {
                tracing::error!("Orchestration start failed for {}: {}", app_id, e);
            }
        });
    }

    Ok(Json(
        json!({ "status": if dry_run { "dry_run" } else { "starting" }, "plan": plan }),
    ))
}

pub async fn stop(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(
        &state.db,
        user.user_id,
        "orchestration_stop",
        "application",
        app_id,
        json!({}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::execute_stop(&state_clone, app_id).await {
            tracing::error!("Orchestration stop failed for {}: {}", app_id, e);
        }
    });

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    let components = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"
        SELECT c.id, c.name,
               COALESCE((SELECT st.to_state FROM state_transitions st WHERE st.component_id = c.id ORDER BY st.created_at DESC LIMIT 1), 'UNKNOWN') as state
        FROM components c
        WHERE c.application_id = $1
        ORDER BY c.name
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data: Vec<Value> = components
        .iter()
        .map(|(id, name, state)| json!({"component_id": id, "name": name, "state": state}))
        .collect();

    let all_running = components.iter().all(|(_, _, s)| s == "RUNNING");

    Ok(Json(json!({
        "app_id": app_id,
        "components": data,
        "all_running": all_running,
    })))
}

pub async fn wait_running(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Query(params): Query<WaitQuery>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    let timeout = std::time::Duration::from_secs(params.timeout.unwrap_or(300));
    let start_time = std::time::Instant::now();

    loop {
        let components = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT COALESCE(
                (SELECT st.to_state FROM state_transitions st WHERE st.component_id = c.id ORDER BY st.created_at DESC LIMIT 1),
                'UNKNOWN'
            ) as state
            FROM components c
            WHERE c.application_id = $1 AND c.is_optional = false
            "#,
        )
        .bind(app_id)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let all_running = components.iter().all(|(s,)| s == "RUNNING");
        let any_failed = components.iter().any(|(s,)| s == "FAILED");

        if all_running {
            return Ok(Json(json!({ "status": "running" })));
        }

        if any_failed {
            return Ok(Json(json!({ "status": "failed" })));
        }

        if start_time.elapsed() > timeout {
            return Ok(Json(json!({ "status": "timeout" })));
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
