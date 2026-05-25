//! Architecture diagram validation by a vision-capable LLM.
//!
//! Input: a user-drawn architecture diagram (referenced by URL or
//! base64-encoded content) plus optional CMDB / agent corpus to
//! cross-check against. Output: a structured list of detected components
//! and dependencies, each with a confidence bucket and a note when the
//! item conflicts with another source.

use serde::{Deserialize, Serialize};

use super::provider::{Confidence, Provider, ProviderError};

#[derive(Debug, Deserialize)]
pub struct SchemaValidationRequest {
    /// Where to fetch the diagram from. The caller is responsible for
    /// making the source reachable from the backend (presigned URL,
    /// base64 data URI, internal path).
    pub diagram_source: String,
    /// Free-form description / context the architect can attach to the
    /// diagram (e.g. "Billing platform — production view, Q2 2026").
    #[serde(default)]
    pub context: String,
    /// Optional corpus the LLM should cross-check against. Typically a
    /// JSON dump of the CMDB + agent observations for the same app.
    #[serde(default)]
    pub corpus: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SchemaValidationResponse {
    pub provider: String,
    pub components: Vec<DetectedComponent>,
    pub dependencies: Vec<DetectedDependency>,
    pub overall_confidence: Confidence,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct DetectedComponent {
    pub name: String,
    pub component_type: String,
    pub host: Option<String>,
    pub confidence: Confidence,
    pub cross_check: CrossCheck,
}

#[derive(Debug, Serialize)]
pub struct DetectedDependency {
    pub from: String,
    pub to: String,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossCheck {
    /// Found in the diagram AND in the corpus.
    Concordant,
    /// Only in the diagram or only in the corpus.
    Partial,
    /// In the diagram but the corpus says the opposite (or vice-versa).
    Contradictory,
    /// No corpus was supplied, so no cross-check was performed.
    NotChecked,
}

pub async fn validate(
    provider: &dyn Provider,
    req: SchemaValidationRequest,
) -> Result<SchemaValidationResponse, ProviderError> {
    let prompt = format!(
        "Parse the architecture diagram at {} and extract the components and dependencies. \
         Cross-check each item against the provided corpus. Return JSON. Context: {}",
        req.diagram_source, req.context
    );

    let raw = provider.generate_json(&prompt, req.corpus.clone()).await?;
    let provider_name = raw
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // The stub does not actually parse anything — return an empty
    // response with a clear note so callers can detect it.
    Ok(SchemaValidationResponse {
        provider: provider_name,
        components: Vec::new(),
        dependencies: Vec::new(),
        overall_confidence: Confidence::Low,
        note: format!(
            "Stub response — wire a real provider to actually parse the diagram at {}. \
             Prompt fingerprint: {}",
            req.diagram_source,
            raw.get("prompt_fingerprint")
                .and_then(|v| v.as_str())
                .unwrap_or("n/a")
        ),
    })
}
