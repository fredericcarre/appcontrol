//! AI-assisted incident causal analysis.
//!
//! Input: an incident reference plus the recent FSM transitions and the
//! relevant subgraph. Output: a ranked list of root-cause hypotheses,
//! each with a confidence and a recommended remediation action (which
//! the human still validates before applying).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::provider::{Confidence, Provider, ProviderError};

#[derive(Debug, Deserialize)]
pub struct IncidentAnalysisRequest {
    pub incident_id: Uuid,
    pub application_id: Uuid,
    /// Component IDs the incident touched directly (from impacted_components).
    pub impacted_component_ids: Vec<Uuid>,
    /// JSON snapshot of recent state_transitions for the impacted
    /// components and their direct neighbours (the caller assembles
    /// this so the AI module stays storage-agnostic).
    #[serde(default)]
    pub transitions_snapshot: serde_json::Value,
    /// Optional list of past incidents that touched the same components
    /// (for correlation).
    #[serde(default)]
    pub similar_past_incidents: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct IncidentAnalysisResponse {
    pub provider: String,
    pub incident_id: Uuid,
    pub hypotheses: Vec<RootCauseHypothesis>,
    pub recommended_actions: Vec<RecommendedAction>,
}

#[derive(Debug, Serialize)]
pub struct RootCauseHypothesis {
    pub summary: String,
    pub suspected_component_ids: Vec<Uuid>,
    pub confidence: Confidence,
    pub evidence: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RecommendedAction {
    pub kind: ActionKind,
    pub target_component_id: Option<Uuid>,
    pub description: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    AddCheck,
    RefineCheck,
    AddComponent,
    AddDependency,
    Restart,
    Rebuild,
    InvestigateManually,
}

pub async fn analyze(
    provider: &dyn Provider,
    req: IncidentAnalysisRequest,
) -> Result<IncidentAnalysisResponse, ProviderError> {
    let context = serde_json::json!({
        "application_id": req.application_id,
        "impacted_component_ids": req.impacted_component_ids,
        "transitions_snapshot": req.transitions_snapshot,
        "similar_past_incidents": req.similar_past_incidents,
    });

    let prompt = format!(
        "Analyse incident {} on application {}. Rank root-cause hypotheses \
         from most to least likely. For each hypothesis suggest a remediation \
         action (add_check, refine_check, add_component, add_dependency, \
         restart, rebuild, investigate_manually).",
        req.incident_id, req.application_id
    );

    let raw = provider.generate_json(&prompt, context).await?;
    let provider_name = raw
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(IncidentAnalysisResponse {
        provider: provider_name,
        incident_id: req.incident_id,
        hypotheses: vec![RootCauseHypothesis {
            summary: "Stub hypothesis — no real provider attached".to_string(),
            suspected_component_ids: req.impacted_component_ids.clone(),
            confidence: Confidence::Low,
            evidence: vec![
                "Replace the stub provider with a real LLM adapter to obtain \
                 actual causal analysis."
                    .to_string(),
            ],
        }],
        recommended_actions: vec![RecommendedAction {
            kind: ActionKind::InvestigateManually,
            target_component_id: req.impacted_component_ids.first().copied(),
            description: "Stub recommendation — investigate manually until AI \
                          provider is wired."
                .to_string(),
        }],
    })
}
