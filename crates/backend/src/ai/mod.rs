//! AI-assisted operations — scaffolding with stable contracts.
//!
//! The methodology and pricing documents promise three AI-powered
//! capabilities:
//!
//! 1. **Schema validation** — a multimodal LLM parses a user-drawn
//!    architecture diagram (PNG / SVG / extracted text) and returns a
//!    structured component / dependency proposal.
//! 2. **Map suggestion** — an LLM consolidates a corpus of collected
//!    data (CMDB + agents + schemas) into a first AppControl map JSON.
//! 3. **Incident causal analysis** — an LLM combines an incident with
//!    recent FSM transitions and the relevant subgraph to suggest
//!    root-cause hypotheses ranked by confidence.
//!
//! This module exposes those capabilities through **stable HTTP
//! contracts** without binding to a specific provider. The actual
//! generation is delegated to a pluggable adapter (`Provider`). The
//! default adapter (`StubProvider`) returns a deterministic placeholder
//! response that is well-typed and self-describing, so:
//!
//!   * frontend and integrators can build against the API immediately;
//!   * downstream automations get explicit `provider: "stub"` so they
//!     know the answer is not yet AI-grade;
//!   * swapping in a real provider (Anthropic / OpenAI / on-prem)
//!     requires only implementing `Provider` and selecting it via the
//!     `AI_PROVIDER` env var (`stub` | `anthropic` | `openai` | ...).
//!
//! Audit log: every AI call is recorded in `action_log` with the
//! provider name and prompt fingerprint, satisfying the
//! "auditable AI" governance principle described in methodology § 9.1.

pub mod provider;
pub mod schema;
pub mod map_gen;
pub mod incident;

pub use provider::{select_default_provider, Provider};
