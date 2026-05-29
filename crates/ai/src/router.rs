//! The sovereign inference router.
//!
//! This is the moat, not a config knob: AppControl is the abstraction that picks
//! the right model for each task based on data sensitivity, and that survives
//! every new model generation. Sensitive context stays on a local model;
//! redacted, abstract context can use the best hosted frontier model.

use sha2::{Digest, Sha256};

use crate::provider::{
    CompletionRequest, CompletionResponse, InferenceError, InferenceProvider, Placement,
};
use crate::redactor::Redactor;
use crate::sensitivity::{Sensitivity, SensitivityClassifier};
use crate::types::AiDecision;

/// Policy: the maximum sensitivity allowed to reach a frontier provider.
///
/// Default `Internal`: redacted topology may go to frontier, anything more
/// sensitive (or any secret) is pinned local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierPolicy {
    /// Never use frontier; everything local. Maximum sovereignty.
    LocalOnly,
    /// Public/Internal may go to frontier (after redaction). The default.
    RedactedInternal,
}

/// Outcome of a routed completion, with the audit record attached.
pub struct RoutedCompletion {
    pub response: CompletionResponse,
    pub decision: AiDecision,
}

/// Routes completions across a local and an optional frontier provider.
pub struct InferenceRouter {
    local: Box<dyn InferenceProvider>,
    frontier: Option<Box<dyn InferenceProvider>>,
    policy: FrontierPolicy,
    classifier: SensitivityClassifier,
    redactor: Redactor,
}

impl InferenceRouter {
    pub fn new(
        local: Box<dyn InferenceProvider>,
        frontier: Option<Box<dyn InferenceProvider>>,
        policy: FrontierPolicy,
    ) -> Self {
        Self {
            local,
            frontier,
            policy,
            classifier: SensitivityClassifier,
            redactor: Redactor,
        }
    }

    /// Decide where a given sensitivity must run.
    fn target(&self, sensitivity: Sensitivity) -> Placement {
        match self.policy {
            FrontierPolicy::LocalOnly => Placement::Local,
            FrontierPolicy::RedactedInternal => {
                if self.frontier.is_some() && sensitivity <= Sensitivity::Internal {
                    Placement::Frontier
                } else {
                    Placement::Local
                }
            }
        }
    }

    /// Run a completion for a `kind` of task, routed by the sensitivity of the
    /// combined prompt. When routed to frontier, the user prompt is redacted
    /// first. Always returns an [`AiDecision`] for the audit trail.
    pub async fn complete(
        &self,
        kind: &str,
        req: &CompletionRequest,
    ) -> Result<RoutedCompletion, InferenceError> {
        let sensitivity = self
            .classifier
            .classify(&req.user)
            .max(self.classifier.classify(&req.system));
        let target = self.target(sensitivity);

        let (provider, effective_req): (&dyn InferenceProvider, CompletionRequest) = match target {
            Placement::Frontier => {
                let f = self
                    .frontier
                    .as_ref()
                    .expect("target=Frontier implies frontier present");
                // Redact before it leaves the machine.
                let redacted = CompletionRequest {
                    system: req.system.clone(),
                    user: self.redactor.redact(&req.user),
                    max_tokens: req.max_tokens,
                };
                (f.as_ref(), redacted)
            }
            Placement::Local => (self.local.as_ref(), req.clone()),
        };

        let prompt_hash = hash_prompt(&effective_req);
        let response = provider.complete(&effective_req).await?;

        let decision = AiDecision {
            kind: kind.to_string(),
            model_provider: provider.id().to_string(),
            model_name: provider.model().to_string(),
            sensitivity: format!("{sensitivity:?}").to_lowercase(),
            routed_to: match target {
                Placement::Local => "local".to_string(),
                Placement::Frontier => "frontier".to_string(),
            },
            prompt_hash,
            created_at: chrono::Utc::now(),
        };

        Ok(RoutedCompletion { response, decision })
    }
}

fn hash_prompt(req: &CompletionRequest) -> String {
    let mut h = Sha256::new();
    h.update(req.system.as_bytes());
    h.update(b"\x00");
    h.update(req.user.as_bytes());
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockProvider;

    fn local_only_router() -> InferenceRouter {
        InferenceRouter::new(Box::new(MockProvider), None, FrontierPolicy::LocalOnly)
    }

    #[tokio::test]
    async fn secrets_pin_to_local_even_with_frontier() {
        // A "frontier" mock that would record if it were called.
        let router = InferenceRouter::new(
            Box::new(MockProvider),
            Some(Box::new(MockProvider)),
            FrontierPolicy::RedactedInternal,
        );
        let req = CompletionRequest {
            system: "classify".into(),
            user: "db password=hunter2".into(),
            max_tokens: 32,
        };
        let out = router.complete("test", &req).await.unwrap();
        assert_eq!(out.decision.routed_to, "local");
        assert_eq!(out.decision.sensitivity, "secret");
    }

    #[tokio::test]
    async fn internal_goes_to_frontier_when_available() {
        let router = InferenceRouter::new(
            Box::new(MockProvider),
            Some(Box::new(MockProvider)),
            FrontierPolicy::RedactedInternal,
        );
        let req = CompletionRequest {
            system: "name apps".into(),
            user: "a postgres on port 5432".into(),
            max_tokens: 32,
        };
        let out = router.complete("test", &req).await.unwrap();
        assert_eq!(out.decision.routed_to, "frontier");
    }

    #[tokio::test]
    async fn local_only_policy_never_routes_frontier() {
        let router = local_only_router();
        let req = CompletionRequest {
            system: "x".into(),
            user: "a postgres on port 5432".into(),
            max_tokens: 32,
        };
        let out = router.complete("test", &req).await.unwrap();
        assert_eq!(out.decision.routed_to, "local");
    }
}
