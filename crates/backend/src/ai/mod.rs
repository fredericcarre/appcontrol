//! Backend wiring for the AI layer.
//!
//! Phase 1 of the AI action plan: a **read-only** operations copilot built on the
//! `appcontrol-ai` sovereign inference router. The AI has no privileges of its
//! own — this module persists every interaction to the append-only `ai_decisions`
//! table (DORA reproducibility) and honours the global kill-switch.
//!
//! The router is built per request from the environment. With no model
//! configured it falls back to a deterministic mock, so the endpoint never hard
//! fails for lack of an LLM.

use std::sync::Arc;

use appcontrol_ai::config::router_from_env;
use appcontrol_ai::provider::CompletionRequest;
use serde_json::json;
use uuid::Uuid;

use crate::config::AiSettings;
use crate::error::ApiError;
use crate::AppState;

/// Result of a copilot turn, surfaced to the API layer.
pub struct ChatOutcome {
    pub answer: String,
    /// Where the inference ran: "local" or "frontier" (sovereignty transparency).
    pub routed_to: String,
    pub model: String,
    /// Data-sensitivity classification of the prompt.
    pub sensitivity: String,
}

const COPILOT_SYSTEM_PROMPT: &str = "You are AppControl's operations copilot for a \
regulated production environment. Answer concisely and factually about the state, \
topology and health of the managed system. You are STRICTLY READ-ONLY: you may \
explain and recommend, but you never claim to have started, stopped, or changed \
anything — any action requires a human-approved operation through AppControl.";

/// Run one read-only copilot turn and record it in the append-only audit trail.
pub async fn chat(
    state: &Arc<AppState>,
    organization_id: Uuid,
    actor_user_id: Uuid,
    question: &str,
) -> Result<ChatOutcome, ApiError> {
    // Global kill-switch: when set, all AI features are off.
    if AiSettings::from_env().kill_switch {
        return Err(ApiError::ServiceUnavailable);
    }

    let router = router_from_env();
    let req = CompletionRequest {
        system: COPILOT_SYSTEM_PROMPT.to_string(),
        user: question.to_string(),
        max_tokens: 800,
    };

    let routed = router
        .complete("chat", &req)
        .await
        .map_err(|e| ApiError::Internal(format!("inference failed: {e}")))?;

    // Append-only audit record (no secrets stored — only a prompt hash + size).
    let d = &routed.decision;
    crate::repository::misc_queries::insert_ai_decision(
        &state.db,
        Uuid::new_v4(),
        organization_id,
        Some(actor_user_id),
        &d.kind,
        &d.model_provider,
        &d.model_name,
        &d.sensitivity,
        &d.routed_to,
        &d.prompt_hash,
        &json!({ "question_chars": question.len() }),
        None,
        "completed",
        d.created_at,
    )
    .await?;

    Ok(ChatOutcome {
        answer: routed.response.text,
        routed_to: d.routed_to.clone(),
        model: d.model_name.clone(),
        sensitivity: d.sensitivity.clone(),
    })
}
