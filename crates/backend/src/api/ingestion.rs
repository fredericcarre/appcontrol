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

#[derive(Debug, Deserialize)]
pub struct CsvIncidentQuery {
    pub organization_id: Uuid,
    pub application_id: Option<Uuid>,
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

/// POST /api/v1/ingestion/xl/csv?application_id=<uuid>&source=<label>
///
/// Two CSV blocks are supported in one upload, separated by a blank line.
/// First block — deployables (columns: name,component_type,host,package,environment).
/// Second block — pipeline dependencies (columns: from,to).
/// If only one block is provided, it is interpreted as deployables.
pub async fn ingest_xl_csv(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<CsvIngestQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let (deployables, pipeline_dependencies) = parse_xl_csv(&body)?;
    let payload = xl::XlPayload {
        application_id: q.application_id,
        source: q.source.unwrap_or_else(|| "xl-csv".to_string()),
        deployables,
        pipeline_dependencies,
    };

    let app_id = payload.application_id;
    let source = payload.source.clone();
    let pool = state.db.clone();
    run_with_audit(state, user, app_id, "ingest.xl.csv", source, || async move {
        xl::ingest(&pool, payload).await
    })
    .await
}

/// POST /api/v1/ingestion/flows/csv?application_id=<uuid>&source=<label>
///
/// CSV columns: from,to,port,protocol
pub async fn ingest_flows_csv(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<CsvIngestQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let flows = parse_flows_csv(&body)?;
    let payload = flow::FlowPayload {
        application_id: q.application_id,
        source: q.source.unwrap_or_else(|| "flow-csv".to_string()),
        flows,
    };
    let app_id = payload.application_id;
    let source = payload.source.clone();
    let pool = state.db.clone();
    run_with_audit(state, user, app_id, "ingest.flows.csv", source, || async move {
        flow::ingest(&pool, payload).await
    })
    .await
}

/// POST /api/v1/ingestion/incidents/csv?organization_id=<uuid>&application_id=<uuid>&source=<label>
///
/// CSV columns: external_id,title,description,severity,status,opened_at,resolved_at,root_cause,impacted_components
/// `opened_at` / `resolved_at` are RFC3339 timestamps.
/// `impacted_components` is a semicolon-separated list of component names.
pub async fn ingest_incidents_csv(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<CsvIncidentQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    if q.organization_id != *user.organization_id {
        return Err(ApiError::Forbidden);
    }
    if let Some(app_id) = q.application_id {
        check_manage(&state, &user, app_id).await?;
    } else if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let incidents = parse_incidents_csv(&body)?;
    let payload = itsm::ItsmPayload {
        organization_id: q.organization_id,
        application_id: q.application_id,
        source: q.source.unwrap_or_else(|| "itsm-csv".to_string()),
        incidents,
    };

    let pool = state.db.clone();
    let source = payload.source.clone();
    let report = itsm::ingest(&pool, payload).await?;
    tracing::info!(
        source = %source,
        created = report.created,
        updated = report.updated,
        skipped = report.skipped,
        errors = report.errors.len(),
        "CSV incident ingestion completed"
    );
    Ok(Json(json!({"status": "ok", "report": report})))
}

fn read_header_indices(headers: &csv::StringRecord) -> std::collections::HashMap<String, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_ascii_lowercase(), i))
        .collect()
}

fn cell(rec: &csv::StringRecord, idx: usize) -> Option<&str> {
    rec.get(idx).map(|s| s.trim()).filter(|s| !s.is_empty())
}

fn parse_cmdb_csv(body: &[u8]) -> Result<Vec<cmdb::CmdbComponent>, ApiError> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_reader(body);
    let headers = reader
        .headers()
        .map_err(|e| ApiError::Validation(format!("invalid CSV header: {}", e)))?
        .clone();
    let idx = read_header_indices(&headers);
    let name_i = idx
        .get("name")
        .copied()
        .ok_or_else(|| ApiError::Validation("CSV is missing required 'name' column".into()))?;

    let mut out = Vec::new();
    for record in reader.records() {
        let rec = record.map_err(|e| ApiError::Validation(format!("invalid CSV row: {}", e)))?;
        let Some(name) = cell(&rec, name_i) else { continue };
        out.push(cmdb::CmdbComponent {
            name: name.to_string(),
            component_type: idx
                .get("component_type")
                .and_then(|&i| cell(&rec, i))
                .unwrap_or("service")
                .to_string(),
            host: idx.get("host").and_then(|&i| cell(&rec, i)).map(String::from),
            description: idx
                .get("description")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            display_name: idx
                .get("display_name")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            tags: idx
                .get("tags")
                .and_then(|&i| cell(&rec, i))
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

fn parse_xl_csv(
    body: &[u8],
) -> Result<(Vec<xl::XlDeployable>, Vec<xl::XlPipelineDep>), ApiError> {
    // Split on blank line. First block = deployables, second = pipeline deps.
    let text = std::str::from_utf8(body)
        .map_err(|e| ApiError::Validation(format!("CSV must be UTF-8: {}", e)))?;
    let mut blocks = text.split("\n\n");
    let dep_block = blocks
        .next()
        .ok_or_else(|| ApiError::Validation("empty CSV".into()))?;
    let pipe_block = blocks.next().unwrap_or("");

    // Block 1 — deployables
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_reader(dep_block.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| ApiError::Validation(format!("invalid CSV header: {}", e)))?
        .clone();
    let idx = read_header_indices(&headers);
    let name_i = idx
        .get("name")
        .copied()
        .ok_or_else(|| ApiError::Validation("deployables CSV is missing 'name' column".into()))?;

    let mut deployables = Vec::new();
    for record in reader.records() {
        let rec = record.map_err(|e| ApiError::Validation(format!("invalid CSV row: {}", e)))?;
        let Some(name) = cell(&rec, name_i) else { continue };
        deployables.push(xl::XlDeployable {
            name: name.to_string(),
            component_type: idx
                .get("component_type")
                .and_then(|&i| cell(&rec, i))
                .unwrap_or("service")
                .to_string(),
            host: idx.get("host").and_then(|&i| cell(&rec, i)).map(String::from),
            package: idx
                .get("package")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            environment: idx
                .get("environment")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
        });
    }

    // Block 2 — pipeline dependencies (optional)
    let mut pipeline_deps = Vec::new();
    if !pipe_block.trim().is_empty() {
        let mut r2 = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(pipe_block.as_bytes());
        let h2 = r2
            .headers()
            .map_err(|e| ApiError::Validation(format!("invalid pipeline CSV header: {}", e)))?
            .clone();
        let i2 = read_header_indices(&h2);
        let from_i = i2.get("from").copied().ok_or_else(|| {
            ApiError::Validation("pipeline CSV missing 'from' column".into())
        })?;
        let to_i = i2
            .get("to")
            .copied()
            .ok_or_else(|| ApiError::Validation("pipeline CSV missing 'to' column".into()))?;
        for record in r2.records() {
            let rec = record.map_err(|e| ApiError::Validation(format!("invalid CSV row: {}", e)))?;
            if let (Some(f), Some(t)) = (cell(&rec, from_i), cell(&rec, to_i)) {
                pipeline_deps.push(xl::XlPipelineDep {
                    from: f.to_string(),
                    to: t.to_string(),
                });
            }
        }
    }

    if deployables.is_empty() {
        return Err(ApiError::Validation(
            "XL CSV contained no deployable rows".into(),
        ));
    }
    Ok((deployables, pipeline_deps))
}

fn parse_flows_csv(body: &[u8]) -> Result<Vec<flow::FlowEntry>, ApiError> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_reader(body);
    let headers = reader
        .headers()
        .map_err(|e| ApiError::Validation(format!("invalid CSV header: {}", e)))?
        .clone();
    let idx = read_header_indices(&headers);
    let from_i = idx
        .get("from")
        .copied()
        .ok_or_else(|| ApiError::Validation("flows CSV missing 'from' column".into()))?;
    let to_i = idx
        .get("to")
        .copied()
        .ok_or_else(|| ApiError::Validation("flows CSV missing 'to' column".into()))?;

    let mut out = Vec::new();
    for record in reader.records() {
        let rec = record.map_err(|e| ApiError::Validation(format!("invalid CSV row: {}", e)))?;
        let Some(from) = cell(&rec, from_i) else { continue };
        let Some(to) = cell(&rec, to_i) else { continue };
        out.push(flow::FlowEntry {
            from: from.to_string(),
            to: to.to_string(),
            port: idx
                .get("port")
                .and_then(|&i| cell(&rec, i))
                .and_then(|s| s.parse().ok()),
            protocol: idx
                .get("protocol")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
        });
    }
    if out.is_empty() {
        return Err(ApiError::Validation("flow CSV contained no rows".into()));
    }
    Ok(out)
}

fn parse_incidents_csv(body: &[u8]) -> Result<Vec<itsm::ItsmIncident>, ApiError> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_reader(body);
    let headers = reader
        .headers()
        .map_err(|e| ApiError::Validation(format!("invalid CSV header: {}", e)))?
        .clone();
    let idx = read_header_indices(&headers);
    let ext_i = idx
        .get("external_id")
        .copied()
        .ok_or_else(|| ApiError::Validation("incidents CSV missing 'external_id'".into()))?;
    let title_i = idx
        .get("title")
        .copied()
        .ok_or_else(|| ApiError::Validation("incidents CSV missing 'title'".into()))?;
    let opened_i = idx
        .get("opened_at")
        .copied()
        .ok_or_else(|| ApiError::Validation("incidents CSV missing 'opened_at'".into()))?;

    let mut out = Vec::new();
    for record in reader.records() {
        let rec = record.map_err(|e| ApiError::Validation(format!("invalid CSV row: {}", e)))?;
        let Some(ext) = cell(&rec, ext_i) else { continue };
        let title = cell(&rec, title_i).unwrap_or("(no title)").to_string();
        let opened_raw = cell(&rec, opened_i).unwrap_or("");
        let opened_at: chrono::DateTime<chrono::Utc> = chrono::DateTime::parse_from_rfc3339(opened_raw)
            .map_err(|e| {
                ApiError::Validation(format!(
                    "invalid opened_at '{}': {} (expected RFC3339)",
                    opened_raw, e
                ))
            })?
            .with_timezone(&chrono::Utc);
        let resolved_at = idx
            .get("resolved_at")
            .and_then(|&i| cell(&rec, i))
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        out.push(itsm::ItsmIncident {
            external_id: ext.to_string(),
            title,
            description: idx
                .get("description")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            severity: idx
                .get("severity")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            status: idx
                .get("status")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            opened_at,
            resolved_at,
            root_cause: idx
                .get("root_cause")
                .and_then(|&i| cell(&rec, i))
                .map(String::from),
            impacted_component_names: idx
                .get("impacted_components")
                .and_then(|&i| cell(&rec, i))
                .map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default(),
            metadata: serde_json::json!({}),
        });
    }
    if out.is_empty() {
        return Err(ApiError::Validation("incidents CSV contained no rows".into()));
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
