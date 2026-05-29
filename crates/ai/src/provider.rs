//! Inference providers — the pluggable back-ends behind the sovereign router.
//!
//! Every provider implements [`InferenceProvider`]. Two concrete ones ship here:
//!
//! * [`MockProvider`] — deterministic, **no network, no API key**. This is what
//!   makes the whole agentic demo testable in one command and what unit tests
//!   run against.
//! * [`OpenAiCompatProvider`] — talks to any OpenAI-compatible `/chat/completions`
//!   endpoint. That single shape covers **on-prem** servers (vLLM, Ollama, TGI)
//!   *and* hosted frontier APIs (Azure OpenAI, OpenAI). This is why "use the
//!   latest online models OR a sovereign local model" is a config choice, not a
//!   rewrite.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Where a provider physically runs — drives the sovereignty routing decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// Runs inside the customer's own infrastructure. Sensitive data may go here.
    Local,
    /// A hosted, internet-reachable model. Only redacted/abstract data goes here.
    Frontier,
}

/// A minimal chat-completion request (provider-agnostic).
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: String,
    pub user: String,
    /// Soft cap on output tokens (providers may ignore).
    pub max_tokens: u32,
}

/// A provider's response.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("inference transport error: {0}")]
    Transport(String),
    #[error("inference provider error: {0}")]
    Provider(String),
}

/// The abstraction every model back-end implements.
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    /// Stable id of the back-end ("mock", "openai-compat").
    fn id(&self) -> &str;
    /// Model name reported for the audit record.
    fn model(&self) -> &str;
    /// Where this provider runs (sovereignty).
    fn placement(&self) -> Placement;
    /// Run a single completion.
    async fn complete(&self, req: &CompletionRequest)
        -> Result<CompletionResponse, InferenceError>;
}

// ---------------------------------------------------------------------------
// MockProvider — deterministic, offline.
// ---------------------------------------------------------------------------

/// A deterministic provider used for tests and for the zero-dependency demo.
///
/// It does not call any model. For the architect "name this application" prompt
/// it returns a stable, sensible name derived from the input, so the demo always
/// produces a readable map even with no LLM configured.
#[derive(Debug, Default, Clone)]
pub struct MockProvider;

#[async_trait]
impl InferenceProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }
    fn model(&self) -> &str {
        "mock-deterministic"
    }
    fn placement(&self) -> Placement {
        // The mock never leaves the process, so it is effectively local.
        Placement::Local
    }
    async fn complete(
        &self,
        req: &CompletionRequest,
    ) -> Result<CompletionResponse, InferenceError> {
        // The architect asks for a JSON object of group names. We echo a stable
        // structure so callers that parse JSON keep working; otherwise we return
        // a short deterministic summary of the user prompt.
        let text = if req.user.contains("\"groups\"") {
            // Let the caller fall back to its deterministic naming.
            "{}".to_string()
        } else {
            format!("[mock] {} chars analysed", req.user.len())
        };
        Ok(CompletionResponse {
            text,
            model: "mock-deterministic".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// OpenAiCompatProvider — vLLM / Ollama / Azure OpenAI / OpenAI.
// ---------------------------------------------------------------------------

/// Talks to any OpenAI-compatible chat-completions endpoint.
pub struct OpenAiCompatProvider {
    id: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
    placement: Placement,
    http: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        placement: Placement,
    ) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            placement,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[async_trait]
impl InferenceProvider for OpenAiCompatProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn placement(&self) -> Placement {
        self.placement
    }
    async fn complete(
        &self,
        req: &CompletionRequest,
    ) -> Result<CompletionResponse, InferenceError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = ChatRequest {
            model: &self.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &req.system,
                },
                ChatMessage {
                    role: "user",
                    content: &req.user,
                },
            ],
            max_tokens: req.max_tokens,
            temperature: 0.1,
        };
        let mut rb = self.http.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            rb = rb.bearer_auth(key);
        }
        let resp = rb
            .send()
            .await
            .map_err(|e| InferenceError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(InferenceError::Provider(format!("HTTP {}", resp.status())));
        }
        let parsed: ChatResponse = resp
            .json()
            .await
            .map_err(|e| InferenceError::Provider(e.to_string()))?;
        let text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        Ok(CompletionResponse {
            text,
            model: self.model.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_is_deterministic_and_local() {
        let p = MockProvider;
        assert_eq!(p.placement(), Placement::Local);
        let req = CompletionRequest {
            system: "s".into(),
            user: "hello world".into(),
            max_tokens: 64,
        };
        let a = p.complete(&req).await.unwrap();
        let b = p.complete(&req).await.unwrap();
        assert_eq!(a.text, b.text);
    }
}
