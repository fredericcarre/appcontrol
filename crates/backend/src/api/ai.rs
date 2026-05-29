//! AI copilot API (read-only).
//!
//! `POST /api/v1/ai/chat` — ask the operations copilot a question about the
//! system. Requires an authenticated user; the interaction is recorded in the
//! append-only `ai_decisions` table. This endpoint is strictly read-only (no
//! state mutation), so it does not require an operate-level permission.

use std::sync::Arc;

use axum::{extract::State, response::Json, Extension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{validate_length, ApiError};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

pub async fn chat(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_length("message", &body.message, 1, 8000)?;

    let outcome =
        crate::ai::chat(&state, *user.organization_id, *user.user_id, &body.message).await?;

    Ok(Json(json!({
        "answer": outcome.answer,
        "routed_to": outcome.routed_to,
        "model": outcome.model,
        "sensitivity": outcome.sensitivity,
    })))
}
