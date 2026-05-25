//! HTTP entry points for the ingestion connectors.
//!
//! Routes are wired in `api/mod.rs` under `/api/v1/ingestion/*`.
//! Auth: all endpoints expect a valid AuthUser (typically via API key)
//! and Manage permission on the target application (or org admin).
//!
//! Each handler delegates to `crate::integrations::<source>::ingest` and
//! returns the resulting `IngestionReport` as JSON.

use axum::{
    extract::{Extension, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::integrations::{cmdb, flow, itsm, xl};
use crate::middleware::audit::{complete_action_failed, complete_action_success, log_action};
use crate::AppState;

async fn check_manage(
    state: &Arc<AppState>,
    user: &AuthUser,
    app_id: Uuid,
) -> Result<(), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

async fn run_with_audit<F, Fut>(
    state: Arc<AppState>,
    user: AuthUser,
    app_id: Uuid,
    action: &'static str,
    source_label: String,
    fut: F,
) -> Result<Json<Value>, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<crate::integrations::IngestionReport, ApiError>>,
{
    check_manage(&state, &user, app_id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        action,
        "application",
        app_id,
        json!({"source": source_label}),
    )
    .await?;

    match fut().await {
        Ok(report) => {
            let _ = complete_action_success(&state.db, action_id).await;
            Ok(Json(json!({
                "status": "ok",
                "report": report,
            })))
        }
        Err(e) => {
            let _ = complete_action_failed(&state.db, action_id, &e.to_string()).await;
            Err(e)
        }
    }
}

/// POST /api/v1/ingestion/cmdb
pub async fn ingest_cmdb(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<cmdb::CmdbPayload>,
) -> Result<Json<Value>, ApiError> {
    let app_id = payload.application_id;
    let source = payload.source.clone();
    let pool = state.db.clone();
    run_with_audit(state, user, app_id, "ingest.cmdb", source, || async move {
        cmdb::ingest(&pool, payload).await
    })
    .await
}

/// POST /api/v1/ingestion/xl
pub async fn ingest_xl(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<xl::XlPayload>,
) -> Result<Json<Value>, ApiError> {
    let app_id = payload.application_id;
    let source = payload.source.clone();
    let pool = state.db.clone();
    run_with_audit(state, user, app_id, "ingest.xl", source, || async move {
        xl::ingest(&pool, payload).await
    })
    .await
}

/// POST /api/v1/ingestion/flows
pub async fn ingest_flows(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<flow::FlowPayload>,
) -> Result<Json<Value>, ApiError> {
    let app_id = payload.application_id;
    let source = payload.source.clone();
    let pool = state.db.clone();
    run_with_audit(state, user, app_id, "ingest.flows", source, || async move {
        flow::ingest(&pool, payload).await
    })
    .await
}

/// POST /api/v1/ingestion/incidents
///
/// Note: incidents are org-scoped, not necessarily app-scoped. The handler
/// requires Manage on the application if `application_id` is provided, else
/// org admin.
pub async fn ingest_incidents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<itsm::ItsmPayload>,
) -> Result<Json<Value>, ApiError> {
    if let Some(app_id) = payload.application_id {
        check_manage(&state, &user, app_id).await?;
    } else if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if payload.organization_id != *user.organization_id {
        return Err(ApiError::Forbidden);
    }

    let pool = state.db.clone();
    let source = payload.source.clone();
    let report = itsm::ingest(&pool, payload).await?;
    tracing::info!(
        source = %source,
        created = report.created,
        updated = report.updated,
        skipped = report.skipped,
        errors = report.errors.len(),
        "Incident ingestion completed"
    );
    Ok(Json(json!({
        "status": "ok",
        "report": report,
    })))
}
