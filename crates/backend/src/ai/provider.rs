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
///
/// Supported values:
///   * `stub` — deterministic placeholder (default)
///   * `anthropic` — Anthropic Messages API. Env: `ANTHROPIC_API_KEY`,
///     optional `ANTHROPIC_MODEL` (default `claude-sonnet-4-6`),
///     optional `ANTHROPIC_BASE_URL`.
///   * `openai` — OpenAI Chat Completions API. Env: `OPENAI_API_KEY`,
///     optional `OPENAI_MODEL` (default `gpt-4o`),
///     optional `OPENAI_BASE_URL` (Azure / on-prem gateways).
///   * any other — falls back to stub with a warning.
///
/// Network failures bubble up as `ProviderError::Provider` so callers
/// (api/ai.rs handlers) can audit-log the cause.
pub fn select_default_provider() -> Box<dyn Provider> {
    match std::env::var("AI_PROVIDER")
        .unwrap_or_else(|_| "stub".to_string())
        .to_lowercase()
        .as_str()
    {
        "anthropic" => match AnthropicProvider::from_env() {
            Ok(p) => Box::new(p),
            Err(e) => {
                tracing::warn!("AI_PROVIDER=anthropic but configuration is incomplete ({}). Falling back to stub.", e);
                Box::new(StubProvider)
            }
        },
        "openai" => match OpenAiProvider::from_env() {
            Ok(p) => Box::new(p),
            Err(e) => {
                tracing::warn!("AI_PROVIDER=openai but configuration is incomplete ({}). Falling back to stub.", e);
                Box::new(StubProvider)
            }
        },
        "on-prem" | "onprem" => {
            tracing::warn!(
                "AI_PROVIDER=on-prem requires customer-specific wiring. \
                 Falling back to stub. Implement OnPremProvider or use \
                 AI_PROVIDER=openai with OPENAI_BASE_URL pointing at your gateway."
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

// ----------------------------------------------------------------------------
// Anthropic Messages API provider
// ----------------------------------------------------------------------------

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| ProviderError::NotConfigured("ANTHROPIC_API_KEY".to_string()))?;
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        Ok(Self { api_key, model, base_url })
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    async fn generate_json(
        &self,
        prompt: &str,
        context: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| ProviderError::Provider(e.to_string()))?;

        let user_content = format!(
            "{}\n\nContext (JSON):\n```json\n{}\n```\n\nReturn STRICT JSON only — no prose, no markdown fences.",
            prompt,
            serde_json::to_string_pretty(&context).unwrap_or_default(),
        );

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [
                {"role": "user", "content": user_content}
            ],
        });

        let resp = client
            .post(format!("{}/v1/messages", self.base_url.trim_end_matches('/')))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Provider(format!("HTTP: {}", e)))?;

        let status = resp.status();
        let raw: serde_json::Value = resp.json().await.map_err(|e| {
            ProviderError::Provider(format!("invalid JSON response: {}", e))
        })?;
        if !status.is_success() {
            return Err(ProviderError::Provider(format!(
                "Anthropic returned HTTP {}: {}",
                status, raw
            )));
        }

        // Anthropic responses: { "content": [ {"type": "text", "text": "..."} ] }
        let text = raw["content"]
            .as_array()
            .and_then(|arr| arr.iter().find_map(|c| c["text"].as_str()))
            .unwrap_or("");

        // Try to parse the text as JSON; if it isn't pure JSON, wrap it.
        let parsed = serde_json::from_str::<serde_json::Value>(text.trim())
            .unwrap_or_else(|_| serde_json::json!({"raw_text": text}));

        let mut result = parsed;
        if let serde_json::Value::Object(ref mut map) = result {
            map.insert("provider".to_string(), serde_json::Value::String("anthropic".into()));
            map.insert(
                "model".to_string(),
                serde_json::Value::String(self.model.clone()),
            );
            map.insert(
                "prompt_fingerprint".to_string(),
                serde_json::Value::String(fingerprint(prompt)),
            );
        }
        Ok(result)
    }
}

// ----------------------------------------------------------------------------
// OpenAI Chat Completions provider (works on api.openai.com,
// Azure OpenAI via base_url, and any OpenAI-compatible gateway).
// ----------------------------------------------------------------------------

pub struct OpenAiProvider {
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| ProviderError::NotConfigured("OPENAI_API_KEY".to_string()))?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com".to_string());
        Ok(Self { api_key, model, base_url })
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Openai
    }

    async fn generate_json(
        &self,
        prompt: &str,
        context: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| ProviderError::Provider(e.to_string()))?;

        let user_content = format!(
            "{}\n\nContext (JSON):\n```json\n{}\n```",
            prompt,
            serde_json::to_string_pretty(&context).unwrap_or_default(),
        );

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": "Return STRICT JSON only. No prose, no markdown fences."},
                {"role": "user", "content": user_content},
            ],
            "response_format": {"type": "json_object"},
            "max_tokens": 4096,
        });

        let resp = client
            .post(format!(
                "{}/v1/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Provider(format!("HTTP: {}", e)))?;

        let status = resp.status();
        let raw: serde_json::Value = resp.json().await.map_err(|e| {
            ProviderError::Provider(format!("invalid JSON response: {}", e))
        })?;
        if !status.is_success() {
            return Err(ProviderError::Provider(format!(
                "OpenAI returned HTTP {}: {}",
                status, raw
            )));
        }

        let text = raw["choices"][0]["message"]["content"].as_str().unwrap_or("");
        let parsed = serde_json::from_str::<serde_json::Value>(text.trim())
            .unwrap_or_else(|_| serde_json::json!({"raw_text": text}));

        let mut result = parsed;
        if let serde_json::Value::Object(ref mut map) = result {
            map.insert("provider".to_string(), serde_json::Value::String("openai".into()));
            map.insert(
                "model".to_string(),
                serde_json::Value::String(self.model.clone()),
            );
            map.insert(
                "prompt_fingerprint".to_string(),
                serde_json::Value::String(fingerprint(prompt)),
            );
        }
        Ok(result)
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
