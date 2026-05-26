//! Initial map generation from a consolidated corpus.
//!
//! Input: an application identifier plus a corpus describing what the
//! captation phase produced (CMDB rows, XL deployables, observed agent
//! discovery, validated diagrams). Output: a draft AppControl map (JSON)
//! that a human reviewer can then refine.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::provider::{Confidence, Provider, ProviderError};

#[derive(Debug, Deserialize)]
pub struct MapSuggestRequest {
    pub application_id: Uuid,
    /// Aggregated corpus produced by the captation phase. Free-form JSON
    /// — the LLM is responsible for reading whatever shape the caller
    /// provides.
    pub corpus: serde_json::Value,
    /// Optional list of patterns / templates the LLM should prefer when
    /// proposing commands (e.g. "spring-boot", "postgres", "kafka").
    #[serde(default)]
    pub preferred_patterns: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MapSuggestResponse {
    pub provider: String,
    pub application_id: Uuid,
    pub draft_map: serde_json::Value,
    pub overall_confidence: Confidence,
    pub uncertainty_notes: Vec<String>,
}

pub async fn suggest(
    provider: &dyn Provider,
    req: MapSuggestRequest,
) -> Result<MapSuggestResponse, ProviderError> {
    let prompt = format!(
        "Generate an AppControl map JSON for application {} from the corpus. \
         Prefer the following patterns when proposing commands: {}. \
         Mark every uncertain item in the returned `uncertainty_notes` list.",
        req.application_id,
        if req.preferred_patterns.is_empty() {
            "(none specified)".to_string()
        } else {
            req.preferred_patterns.join(", ")
        }
    );

    let raw = provider.generate_json(&prompt, req.corpus.clone()).await?;
    let provider_name = raw
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(MapSuggestResponse {
        provider: provider_name,
        application_id: req.application_id,
        draft_map: serde_json::json!({
            "application": {
                "id": req.application_id,
                "components": [],
                "dependencies": [],
            },
            "_note": "Stub draft — wire a real provider to populate."
        }),
        overall_confidence: Confidence::Low,
        uncertainty_notes: vec![
            "Stub provider did not generate a real draft. Configure AI_PROVIDER \
             and implement a real adapter in crates/backend/src/ai/provider.rs."
                .to_string(),
        ],
    })
}
