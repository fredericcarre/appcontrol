//! AppControl AI layer.
//!
//! This crate is the foundation of AppControl's agentic story (Phase 0 of the
//! action plan): a **sovereign inference router** plus the **architect** pass
//! that turns raw, multi-agent discovery into a readable architecture map.
//!
//! It is deliberately runnable standalone (no backend, no database, no API key)
//! so the concepts are testable in one command:
//!
//! ```text
//! appcontrol-agent discover --json | appcontrol-ai architect
//! ```

pub mod architect;
pub mod config;
pub mod provider;
pub mod redactor;
pub mod render;
pub mod router;
pub mod sensitivity;
pub mod types;

pub use provider::{InferenceProvider, MockProvider, OpenAiCompatProvider, Placement};
pub use router::{FrontierPolicy, InferenceRouter};
pub use sensitivity::{Sensitivity, SensitivityClassifier};
pub use types::ArchitectureView;
