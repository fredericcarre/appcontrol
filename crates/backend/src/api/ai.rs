//! HTTP entry points for AI-assisted operations.
//!
//! Routes:
//!
//!   POST /api/v1/ai/schema/validate     — parse architecture diagram
//!   POST /api/v1/ai/map/suggest         — generate initial map JSON
//!   POST /api/v1/ai/incident/analyze    — root-cause analysis
//!
//! Every request goes through:
//!   1. RBAC check (Edit on the application).
//!   2. action_log write before invoking the provider (critical rule #3).
//!   3. Provider selection via `select_default_provider`.
//!   4. Mark success/failure in the audit log.
//!
//! The default (and only built-in) provider is a deterministic stub —
//! switching to a real provider only requires implementing the
//! `Provider` trait in `crate::ai::provider`.

use axum::{
    extract::{Extension, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

use crate::ai;
use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::ApiError;
use crate::middleware::audit::{complete_action_failed, complete_action_success, log_action};
use crate::AppState;

async fn require_edit(state: &Arc<AppState>, user: &AuthUser, app_id: Uuid) -> Result<(), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

/// POST /api/v1/ai/schema/validate
pub async fn schema_validate(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<SchemaRequestBody>,
) -> Result<Json<Value>, ApiError> {
    require_edit(&state, &user, body.application_id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "ai.schema.validate",
        "application",
        body.application_id,
        json!({"diagram_source": body.request.diagram_source}),
    )
    .await?;

    let provider = ai::select_default_provider();
    match ai::schema::validate(&*provider, body.request).await {
        Ok(resp) => {
            let _ = complete_action_success(&state.db, action_id).await;
            Ok(Json(json!({"status": "ok", "response": resp})))
        }
        Err(e) => {
            let _ = complete_action_failed(&state.db, action_id, &e.to_string()).await;
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

/// POST /api/v1/ai/map/suggest
pub async fn map_suggest(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<ai::map_gen::MapSuggestRequest>,
) -> Result<Json<Value>, ApiError> {
    require_edit(&state, &user, req.application_id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "ai.map.suggest",
        "application",
        req.application_id,
        json!({"preferred_patterns": req.preferred_patterns}),
    )
    .await?;

    let provider = ai::select_default_provider();
    match ai::map_gen::suggest(&*provider, req).await {
        Ok(resp) => {
            let _ = complete_action_success(&state.db, action_id).await;
            Ok(Json(json!({"status": "ok", "response": resp})))
        }
        Err(e) => {
            let _ = complete_action_failed(&state.db, action_id, &e.to_string()).await;
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct RagQueryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    5
}

/// POST /api/v1/ai/rag/query
///
/// Looks up the local runbook corpus configured via `RAG_CORPUS_DIR`.
/// Returns matching chunks ranked by token-frequency × IDF. Returns
/// 404 if `RAG_CORPUS_DIR` is unset.
pub async fn rag_query(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<RagQueryRequest>,
) -> Result<Json<Value>, ApiError> {
    // Org admin or any authenticated user with at least one app permission
    // is allowed to query the shared knowledge base. We keep the check
    // light: just require an authenticated user.
    let _ = user;
    let _ = &state;

    let answer = ai::rag::query(&body.query, body.top_k.clamp(1, 50)).ok_or_else(|| {
        ApiError::Validation(
            "RAG_CORPUS_DIR is not configured; set the env var to a directory of \
             markdown runbooks to enable rag/query"
                .to_string(),
        )
    })?;

    Ok(Json(json!({"status": "ok", "response": answer})))
}

/// POST /api/v1/ai/rag/reload
///
/// Forces a re-index of the corpus, typically called after pushing
/// new runbooks. Admin-only.
pub async fn rag_reload(
    State(_state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }
    ai::rag::reload();
    Ok(Json(json!({"status": "ok", "message": "RAG index cleared; next query will rebuild"})))
}

/// POST /api/v1/ai/incident/analyze
pub async fn incident_analyze(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<ai::incident::IncidentAnalysisRequest>,
) -> Result<Json<Value>, ApiError> {
    require_edit(&state, &user, req.application_id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "ai.incident.analyze",
        "application",
        req.application_id,
        json!({"incident_id": req.incident_id}),
    )
    .await?;

    let provider = ai::select_default_provider();
    match ai::incident::analyze(&*provider, req).await {
        Ok(resp) => {
            let _ = complete_action_success(&state.db, action_id).await;
            Ok(Json(json!({"status": "ok", "response": resp})))
        }
        Err(e) => {
            let _ = complete_action_failed(&state.db, action_id, &e.to_string()).await;
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct SchemaRequestBody {
    pub application_id: Uuid,
    #[serde(flatten)]
    pub request: ai::schema::SchemaValidationRequest,
}
