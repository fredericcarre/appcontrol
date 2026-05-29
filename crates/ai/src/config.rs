//! Build the sovereign router from environment variables.
//!
//! Mirrors the `AI_*` variables in the action plan. No secrets or defaults are
//! baked in: a missing frontier key simply means "local only", which is the
//! safest behaviour. With nothing configured at all, a [`MockProvider`] local
//! back-end is used so the demo still runs.

use crate::provider::{InferenceProvider, MockProvider, OpenAiCompatProvider, Placement};
use crate::router::{FrontierPolicy, InferenceRouter};

/// Construct an [`InferenceRouter`] from the process environment.
///
/// * `AI_LOCAL_BASE_URL` + `AI_LOCAL_MODEL` → an on-prem OpenAI-compatible local
///   provider (vLLM/Ollama/TGI). If unset, a deterministic mock is used.
/// * `AI_FRONTIER_API_KEY` + `AI_FRONTIER_BASE_URL` + `AI_FRONTIER_MODEL` → a
///   hosted frontier provider. If unset, the router is local-only.
/// * `AI_INFERENCE_MODE=local` forces local-only regardless of frontier config.
pub fn router_from_env() -> InferenceRouter {
    let local: Box<dyn InferenceProvider> = match (
        std::env::var("AI_LOCAL_BASE_URL").ok(),
        std::env::var("AI_LOCAL_MODEL").ok(),
    ) {
        (Some(url), Some(model)) if !url.is_empty() && !model.is_empty() => {
            Box::new(OpenAiCompatProvider::new(
                "local",
                url,
                std::env::var("AI_LOCAL_API_KEY").ok(),
                model,
                Placement::Local,
            ))
        }
        _ => Box::new(MockProvider),
    };

    let mode = std::env::var("AI_INFERENCE_MODE").unwrap_or_else(|_| "hybrid".to_string());
    let frontier: Option<Box<dyn InferenceProvider>> = if mode == "local" {
        None
    } else {
        match (
            std::env::var("AI_FRONTIER_BASE_URL").ok(),
            std::env::var("AI_FRONTIER_MODEL").ok(),
            std::env::var("AI_FRONTIER_API_KEY").ok(),
        ) {
            (Some(url), Some(model), Some(key))
                if !url.is_empty() && !model.is_empty() && !key.is_empty() =>
            {
                Some(Box::new(OpenAiCompatProvider::new(
                    "frontier",
                    url,
                    Some(key),
                    model,
                    Placement::Frontier,
                )))
            }
            _ => None,
        }
    };

    let policy = if frontier.is_some() {
        FrontierPolicy::RedactedInternal
    } else {
        FrontierPolicy::LocalOnly
    };

    InferenceRouter::new(local, frontier, policy)
}
