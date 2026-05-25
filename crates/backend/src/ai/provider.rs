//! Pluggable AI provider abstraction.
//!
//! Selects an implementation based on the `AI_PROVIDER` environment
//! variable. Default (and only built-in) implementation is
//! `StubProvider` — it returns deterministic, well-typed placeholder
//! responses so the rest of the system can be exercised without an
//! actual LLM behind it.
//!
//! Adding a real provider (Anthropic, OpenAI, on-prem) is a matter of
//! implementing the `Provider` trait below and wiring it in
//! `select_default_provider`. The contract is deliberately small:
//!
//!   * the provider receives a typed request and a free-form text
//!     prompt that the caller has already framed;
//!   * it returns a typed response plus a `provider` discriminator so
//!     downstream code can tell stub from real;
//!   * errors are stringly typed for now — they will surface in the
//!     audit log and the HTTP response.
//!
//! No business logic lives here: the schema / map / incident modules
//! decide *what* to ask. The provider only handles *how* the call is
//! routed.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Stub,
    Anthropic,
    Openai,
    OnPrem,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Stub => "stub",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Openai => "openai",
            ProviderKind::OnPrem => "on-prem",
        }
    }
}

/// Confidence bucket attached to every AI proposal so the human reviewer
/// knows how much weight to put on it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Minimal contract every provider must honour.
#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    /// Generate a JSON response from a free-form prompt. The caller
    /// (schema / map_gen / incident) is responsible for framing the
    /// prompt and parsing the returned JSON.
    async fn generate_json(
        &self,
        prompt: &str,
        context: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Provider not configured: {0}")]
    NotConfigured(String),
}

/// Select a provider based on the `AI_PROVIDER` environment variable.
/// Currently `stub` is the only implementation that ships; the rest
/// return `NotConfigured` so the system fails loudly rather than
/// silently invoking the wrong model.
pub fn select_default_provider() -> Box<dyn Provider> {
    match std::env::var("AI_PROVIDER")
        .unwrap_or_else(|_| "stub".to_string())
        .to_lowercase()
        .as_str()
    {
        "anthropic" | "openai" | "on-prem" | "onprem" => {
            tracing::warn!(
                "AI_PROVIDER requested a real provider but only the stub is built in. \
                 Falling back to stub. Implement crate::ai::provider::Provider to wire \
                 a real LLM."
            );
            Box::new(StubProvider)
        }
        _ => Box::new(StubProvider),
    }
}

/// Deterministic stub used until a real LLM is wired in. Returns the
/// context echoed back with a `provider: "stub"` marker so callers can
/// detect the placeholder unambiguously.
pub struct StubProvider;

#[async_trait]
impl Provider for StubProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Stub
    }

    async fn generate_json(
        &self,
        prompt: &str,
        context: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        Ok(serde_json::json!({
            "provider": "stub",
            "prompt_fingerprint": fingerprint(prompt),
            "context_echo": context,
            "confidence": "low",
            "note": "This is a stub response. Set AI_PROVIDER and wire a real \
                     adapter in crates/backend/src/ai/provider.rs to enable \
                     actual AI generation.",
        }))
    }
}

/// Short, stable fingerprint of a prompt — used to deduplicate calls and
/// to correlate audit log entries with provider replies. SHA-256 first
/// 12 hex chars is plenty for this purpose.
pub fn fingerprint(prompt: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let bytes = hasher.finalize();
    hex::encode(&bytes[..6])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_provider_echoes_context() {
        let p = StubProvider;
        let resp = p
            .generate_json("hello", serde_json::json!({"k": "v"}))
            .await
            .unwrap();
        assert_eq!(resp["provider"], "stub");
        assert_eq!(resp["context_echo"]["k"], "v");
        assert_eq!(resp["confidence"], "low");
    }

    #[test]
    fn fingerprint_is_stable_and_short() {
        let a = fingerprint("hello world");
        let b = fingerprint("hello world");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
        assert_ne!(fingerprint("hello world"), fingerprint("goodbye"));
    }

    #[test]
    fn kind_strings_match_serde() {
        assert_eq!(ProviderKind::Stub.as_str(), "stub");
        assert_eq!(ProviderKind::Anthropic.as_str(), "anthropic");
        assert_eq!(ProviderKind::Openai.as_str(), "openai");
        assert_eq!(ProviderKind::OnPrem.as_str(), "on-prem");
    }
}
