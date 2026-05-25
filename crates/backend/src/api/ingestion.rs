//! HTTP entry points for the ingestion connectors.
//!
//! Routes are wired in `api/mod.rs` under `/api/v1/ingestion/*`.
//! Auth: all endpoints expect a valid AuthUser (typically via API key)
//! and Manage permission on the target application (or org admin).
//!
//! Each handler delegates to `crate::integrations::<source>::ingest` and
//! returns the resulting `IngestionReport` as JSON.

use axum::{
    body::Bytes,
    extract::{Extension, Query, State},
    response::Json,
};
use serde::Deserialize;
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

// ----------------------------------------------------------------------------
// CSV ingestion — accepts text/csv for the same connectors.
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CsvIngestQuery {
    pub application_id: Uuid,
    pub source: Option<String>,
}

/// POST /api/v1/ingestion/cmdb/csv?application_id=<uuid>&source=<label>
///
/// CSV columns expected (header row required):
///   name,component_type,host,description,display_name,tags
/// `tags` is a semicolon-separated list (e.g. `java;owner:billing;tier1`).
pub async fn ingest_cmdb_csv(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<CsvIngestQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let components = parse_cmdb_csv(&body)?;
    let payload = cmdb::CmdbPayload {
        application_id: q.application_id,
        source: q.source.unwrap_or_else(|| "cmdb-csv".to_string()),
        components,
    };

    let app_id = payload.application_id;
    let source = payload.source.clone();
    let pool = state.db.clone();
    run_with_audit(state, user, app_id, "ingest.cmdb.csv", source, || async move {
        cmdb::ingest(&pool, payload).await
    })
    .await
}

fn parse_cmdb_csv(body: &[u8]) -> Result<Vec<cmdb::CmdbComponent>, ApiError> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_reader(body);
    let headers = reader
        .headers()
        .map_err(|e| ApiError::Validation(format!("invalid CSV header: {}", e)))?
        .clone();

    let mut out = Vec::new();
    for record in reader.records() {
        let rec = record.map_err(|e| ApiError::Validation(format!("invalid CSV row: {}", e)))?;
        let get = |name: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h.eq_ignore_ascii_case(name))
                .and_then(|idx| rec.get(idx))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let name = get("name").ok_or_else(|| {
            ApiError::Validation("CSV row missing required column 'name'".to_string())
        })?;

        out.push(cmdb::CmdbComponent {
            name,
            component_type: get("component_type").unwrap_or_else(|| "service".to_string()),
            host: get("host"),
            description: get("description"),
            display_name: get("display_name"),
            tags: get("tags")
                .map(|s| {
                    s.split(';')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        });
    }

    if out.is_empty() {
        return Err(ApiError::Validation(
            "CSV payload contained no data rows".to_string(),
        ));
    }
    Ok(out)
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
