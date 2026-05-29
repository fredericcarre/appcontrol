//! Human-readable rendering of an [`ArchitectureView`] for the terminal demo.
//!
//! The goal of the whole architect pass is that this output reads like an
//! architecture diagram, not a process list — so the renderer leads with L0
//! applications, then L1 components and their dependencies, and finishes with a
//! one-line count of the system noise that was filtered out.

use crate::types::{ArchitectureView, Confidence, EdgeSource};

fn badge(c: Confidence) -> &'static str {
    match c {
        Confidence::High => "●high",
        Confidence::Medium => "◐med",
        Confidence::Low => "○low",
    }
}

fn via(s: EdgeSource) -> &'static str {
    match s {
        EdgeSource::ConfigFile => "config",
        EdgeSource::TcpConnection => "tcp",
        EdgeSource::PortTyping => "port",
    }
}

/// Render the view as an indented, architect-level tree.
pub fn to_text(view: &ArchitectureView) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "AppControl — Architecture map ({} application(s), {} component(s) across {} host(s))\n",
        view.groups.len(),
        view.application_nodes().count(),
        view.hosts.len()
    ));
    s.push_str("================================================================\n\n");

    for g in &view.groups {
        let named = if g.named_by_ai { " [named by AI]" } else { "" };
        s.push_str(&format!(
            "▣ APPLICATION: {}  ({}){}\n",
            g.name,
            badge(g.confidence),
            named
        ));
        for &nid in &g.member_nodes {
            let n = &view.nodes[nid];
            let ports = if n.ports.is_empty() {
                String::new()
            } else {
                let p: Vec<String> = n.ports.iter().map(|p| format!(":{p}")).collect();
                format!(" {}", p.join(","))
            };
            let tech = n.technology.clone().unwrap_or_else(|| "-".to_string());
            s.push_str(&format!(
                "   ├─ {:<16} [{}] {}{}  {}  @{}\n",
                n.name,
                n.layer,
                tech,
                ports,
                badge(n.confidence),
                n.host
            ));
            // Dependencies originating from this node.
            for e in view.edges.iter().filter(|e| e.from == nid) {
                let target = &view.nodes[e.to];
                let detail = e
                    .detail
                    .as_ref()
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "   │     └─▶ depends on {} (via {}{})\n",
                    target.name,
                    via(e.via),
                    detail
                ));
            }
            // Operational commands discovered (proof the map is actionable).
            if let Some(cmd) = &n.start_cmd {
                s.push_str(&format!("   │       start: {cmd}\n"));
            }
        }
        s.push('\n');
    }

    s.push_str(&format!(
        "— SYSTEM (filtered): {} process(es) hidden (systemd, sshd, cron, agent, …)\n",
        view.system_processes_hidden
    ));
    s.push_str(
        "  This is no longer 'just an agent on a box' — it is an agentic view of the system.\n",
    );
    s
}
