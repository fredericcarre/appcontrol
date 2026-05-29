//! Public types for the AppControl AI layer.
//!
//! These describe (a) the *architecture view* produced by the architect pass —
//! the readable, architect-level map that distinguishes real applications from
//! system noise — and (b) the *AI decision* audit record (DORA reproducibility).

use serde::{Deserialize, Serialize};

/// What a node in the architecture map fundamentally is.
///
/// The whole point of the architect pass is to put every discovered process in
/// one of these buckets so the default view shows *applications*, not a process
/// dump. `SystemProcess` nodes are hidden by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A real business/application component worth showing (a service, a DB, a
    /// cache, a queue, a web server...).
    Application,
    /// Operating-system / infrastructure plumbing (systemd, sshd, cron, the
    /// AppControl agent itself...). Noise — hidden by default.
    SystemProcess,
}

/// The level of detail at which a node is meant to be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// L0 — business applications (groups of components). The "architect" view.
    Application,
    /// L1 — individual components (a service, a database, a cache...).
    Component,
    /// L2 — raw processes & ports. The full detail, on demand.
    Process,
}

/// How confident we are about a classification or a dependency edge.
///
/// Kept as a coarse enum (rather than only a float) so it renders as a badge and
/// matches the `confidence` strings already used in `CommandSuggestion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
        }
    }

    /// A numeric score in [0,1] for sorting / aggregation.
    pub fn score(&self) -> f32 {
        match self {
            Confidence::Low => 0.4,
            Confidence::Medium => 0.7,
            Confidence::High => 0.92,
        }
    }
}

/// A single component in the architecture map (L1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchNode {
    /// Stable index used by edges (position in the `nodes` vec).
    pub id: usize,
    /// Display name (business name once the LLM has named it, else the process
    /// or service name).
    pub name: String,
    pub kind: NodeKind,
    /// Functional layer: "Database", "Cache", "Middleware", "Web", "Service"...
    pub layer: String,
    /// Detected technology id (e.g. "postgresql", "redis", "java"), if any.
    pub technology: Option<String>,
    /// Host the component runs on.
    pub host: String,
    /// Ports it listens on.
    pub ports: Vec<u16>,
    /// Underlying process name + pid (L2 detail).
    pub process_name: String,
    pub pid: u32,
    /// Operational commands suggested by discovery (check/start/stop), if any.
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    /// Confidence that this classification (app vs system, technology) is right.
    pub confidence: Confidence,
    /// Which L0 application group this node belongs to (index into `groups`).
    pub group: Option<usize>,
}

/// How a dependency between two components was inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSource {
    /// Confirmed by a config file entry (e.g. spring.datasource.url). Strongest.
    ConfigFile,
    /// Observed TCP connection from one component to another's listener.
    TcpConnection,
    /// Only the destination port type is known (e.g. :5432 ⇒ PostgreSQL).
    PortTyping,
}

/// A directed dependency edge: `from` depends on `to`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchEdge {
    pub from: usize,
    pub to: usize,
    pub via: EdgeSource,
    pub confidence: Confidence,
    /// Optional detail (config key, or "host:port").
    pub detail: Option<String>,
}

/// An L0 business application — a named group of components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchGroup {
    pub id: usize,
    pub name: String,
    /// Whether the name came from the LLM (true) or a deterministic fallback.
    pub named_by_ai: bool,
    pub confidence: Confidence,
    pub member_nodes: Vec<usize>,
}

/// The full architect-level view of a system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureView {
    pub groups: Vec<ArchGroup>,
    pub nodes: Vec<ArchNode>,
    pub edges: Vec<ArchEdge>,
    /// How many processes were classified as system noise and hidden.
    pub system_processes_hidden: usize,
    /// The hosts that contributed fragments to this view (multi-agent).
    pub hosts: Vec<String>,
}

impl ArchitectureView {
    /// Application (non-system) nodes only — the default L1 view.
    pub fn application_nodes(&self) -> impl Iterator<Item = &ArchNode> {
        self.nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Application)
    }
}

/// An append-only audit record of a single AI decision.
///
/// Mirrors the `ai_decisions` table from the action plan: it makes any AI output
/// reproducible (which model, routed where, what it proposed) — a DORA
/// requirement. In this standalone demo we print it; the backend persists it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDecision {
    pub kind: String,
    pub model_provider: String,
    pub model_name: String,
    pub sensitivity: String,
    pub routed_to: String,
    /// SHA-256 of the exact prompt — reproducibility without storing secrets.
    pub prompt_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
