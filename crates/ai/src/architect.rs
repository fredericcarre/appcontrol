//! The architect pass — turn raw, multi-agent discovery into a readable map.
//!
//! This is the credibility test: the default view must look like an architecture
//! diagram a human would draw, not an `htop` dump. Three jobs the regex layer
//! alone cannot do well:
//!   1. distinguish a real application from a system process,
//!   2. group + name components into business applications (L0),
//!   3. expose tiers L0/L1/L2 with confidence per node and per edge.
//!
//! The core classification is **deterministic and offline** (so the demo always
//! works). An optional LLM pass, routed through the sovereign router, replaces
//! the deterministic group names with business names.

use std::collections::HashMap;

use appcontrol_common::protocol::AgentMessage;
use appcontrol_common::types::{
    DiscoveredConnection, DiscoveredListener, DiscoveredProcess, DiscoveredService,
};

use crate::provider::CompletionRequest;
use crate::router::InferenceRouter;
use crate::types::{
    ArchEdge, ArchGroup, ArchNode, ArchitectureView, Confidence, EdgeSource, NodeKind,
};

/// A per-host discovery fragment (one agent's contribution).
pub struct Fragment {
    pub hostname: String,
    pub processes: Vec<DiscoveredProcess>,
    pub listeners: Vec<DiscoveredListener>,
    pub connections: Vec<DiscoveredConnection>,
    #[allow(dead_code)]
    pub services: Vec<DiscoveredService>,
}

impl Fragment {
    /// Extract a fragment from an agent `DiscoveryReport` message, if that's
    /// what it is.
    pub fn from_message(msg: AgentMessage) -> Option<Fragment> {
        match msg {
            AgentMessage::DiscoveryReport {
                hostname,
                processes,
                listeners,
                connections,
                services,
                ..
            } => Some(Fragment {
                hostname,
                processes,
                listeners,
                connections,
                services,
            }),
            _ => None,
        }
    }
}

/// Process names that are operating-system / infrastructure plumbing, never an
/// application worth showing at L0/L1.
const SYSTEM_PROCS: &[&str] = &[
    "systemd",
    "systemd-journal",
    "systemd-logind",
    "systemd-udevd",
    "systemd-resolve",
    "systemd-network",
    "init",
    "kthreadd",
    "ksoftirqd",
    "kworker",
    "migration",
    "rcu_sched",
    "sshd",
    "cron",
    "crond",
    "atd",
    "rsyslogd",
    "syslog-ng",
    "dbus-daemon",
    "dbus",
    "polkitd",
    "accounts-daemon",
    "snapd",
    "udevd",
    "agetty",
    "login",
    "getty",
    "chronyd",
    "ntpd",
    "networkmanager",
    "wpa_supplicant",
    "irqbalance",
    "rngd",
    "auditd",
    "master",
    "qmgr",
    "pickup",
    "sssd",
    "packagekitd",
    "unattended-upgr",
    "multipathd",
    "lvmetad",
    "containerd",
    "dockerd",
    "containerd-shim",
    "runc",
    "appcontrol-agent",
    "bash",
    "sh",
    "sleep",
    "su",
    "sudo",
    "ps",
    "top",
    "tail",
    "cat",
    "tini",
    "s6-supervise",
    "supervisord",
];

/// Well-known ports → (technology id, functional layer).
fn port_tech(port: u16) -> Option<(&'static str, &'static str)> {
    match port {
        5432 => Some(("postgresql", "Database")),
        3306 => Some(("mysql", "Database")),
        1521 => Some(("oracle", "Database")),
        1433 => Some(("sqlserver", "Database")),
        27017 => Some(("mongodb", "Database")),
        6379 => Some(("redis", "Cache")),
        11211 => Some(("memcached", "Cache")),
        5672 | 15672 => Some(("rabbitmq", "Middleware")),
        9092 => Some(("kafka", "Middleware")),
        1414 => Some(("ibmmq", "Middleware")),
        9200 | 9300 => Some(("elasticsearch", "Database")),
        80 | 443 => Some(("http", "Web")),
        8080 | 8443 | 8000 => Some(("http", "Service")),
        _ => None,
    }
}

fn is_system_process(p: &DiscoveredProcess) -> bool {
    let name = p.name.to_lowercase();
    SYSTEM_PROCS
        .iter()
        .any(|s| name == *s || name.starts_with(s))
}

/// Decide whether a process is an application worth showing, and with what
/// confidence / layer / technology.
///
/// Credibility rule: a process is only surfaced as an application if it has a
/// real **operational anchor** — it listens on a service port, it maps to a
/// system service, or it carries an operable start/stop command. This is what
/// stops the brittle name-based tech matcher from promoting threads ("Bun Pool
/// 2"), CLIs ("bash") or random processes into "databases". A PostgreSQL that
/// listens on nothing is not a serving database.
fn classify(p: &DiscoveredProcess) -> Option<(Confidence, String, Option<String>)> {
    // System plumbing is always noise, even when it listens (sshd, dockerd…).
    if is_system_process(p) {
        return None;
    }

    let serving_port = p
        .listening_ports
        .iter()
        .copied()
        .filter(|&port| port < 32768)
        .min();
    let has_service = p.matched_service.is_some();
    let operable = matches!(
        p.command_suggestion.as_ref().map(|c| c.source.as_str()),
        Some("systemd") | Some("windows-service") | Some("docker")
    );

    // No operational anchor → don't trust it as an application (hide it).
    if serving_port.is_none() && !has_service && !operable {
        return None;
    }

    // Technology: prefer discovery's typed hint, else infer from the port.
    let (tech, layer) = if let Some(hint) = &p.technology_hint {
        (Some(hint.id.clone()), hint.layer.clone())
    } else if let Some((t, l)) = serving_port.and_then(port_tech) {
        (Some(t.to_string()), l.to_string())
    } else {
        (None, "Service".to_string())
    };

    // Confidence: a real listening port is the strongest corroboration.
    let confidence = if serving_port.is_some() {
        Confidence::High
    } else if has_service || operable {
        Confidence::Medium
    } else {
        Confidence::Low
    };

    Some((confidence, layer, tech))
}

/// Build the architect view from one or more discovery fragments (multi-agent).
pub fn build(fragments: &[Fragment]) -> ArchitectureView {
    let mut nodes: Vec<ArchNode> = Vec::new();
    let mut hidden = 0usize;
    let mut hosts: Vec<String> = Vec::new();

    // (host, port) -> node id, to resolve dependency targets.
    let mut port_index: HashMap<(String, u16), usize> = HashMap::new();
    // (host, pid) -> node id, to resolve dependency sources.
    let mut pid_index: HashMap<(String, u32), usize> = HashMap::new();

    for frag in fragments {
        if !hosts.contains(&frag.hostname) {
            hosts.push(frag.hostname.clone());
        }
        for p in &frag.processes {
            match classify(p) {
                Some((confidence, layer, technology)) => {
                    let id = nodes.len();
                    let cs = p.command_suggestion.as_ref();
                    // Prefer the matched service name (e.g. "order-api" from
                    // "order-api.service") over the raw process name ("java").
                    let display = p
                        .matched_service
                        .as_ref()
                        .map(|s| {
                            s.trim_end_matches(".service")
                                .trim_end_matches(".exe")
                                .to_string()
                        })
                        .unwrap_or_else(|| p.name.clone());
                    let node = ArchNode {
                        id,
                        name: display,
                        kind: NodeKind::Application,
                        layer,
                        technology,
                        host: frag.hostname.clone(),
                        ports: p.listening_ports.clone(),
                        process_name: p.name.clone(),
                        pid: p.pid,
                        check_cmd: cs.map(|c| c.check_cmd.clone()),
                        start_cmd: cs.and_then(|c| c.start_cmd.clone()),
                        stop_cmd: cs.and_then(|c| c.stop_cmd.clone()),
                        confidence,
                        group: None,
                    };
                    pid_index.insert((frag.hostname.clone(), p.pid), id);
                    for &port in &p.listening_ports {
                        port_index.insert((frag.hostname.clone(), port), id);
                    }
                    nodes.push(node);
                }
                None => hidden += 1,
            }
        }
        // Listeners may reveal a port→pid mapping for processes we kept.
        for l in &frag.listeners {
            if let Some(pid) = l.pid {
                if let Some(&id) = pid_index.get(&(frag.hostname.clone(), pid)) {
                    port_index
                        .entry((frag.hostname.clone(), l.port))
                        .or_insert(id);
                    if !nodes[id].ports.contains(&l.port) {
                        nodes[id].ports.push(l.port);
                    }
                }
            }
        }
    }

    let edges = build_edges(fragments, &nodes, &pid_index, &port_index);
    let groups = build_groups(&mut nodes, &edges);

    ArchitectureView {
        groups,
        nodes,
        edges,
        system_processes_hidden: hidden,
        hosts,
    }
}

fn build_edges(
    fragments: &[Fragment],
    nodes: &[ArchNode],
    pid_index: &HashMap<(String, u32), usize>,
    port_index: &HashMap<(String, u16), usize>,
) -> Vec<ArchEdge> {
    // Keyed by (from,to) so we keep only the strongest source.
    let mut edges: HashMap<(usize, usize), ArchEdge> = HashMap::new();

    let mut insert = |e: ArchEdge| {
        let key = (e.from, e.to);
        let replace = match edges.get(&key) {
            None => true,
            Some(existing) => edge_rank(e.via) > edge_rank(existing.via),
        };
        if replace {
            edges.insert(key, e);
        }
    };

    for frag in fragments {
        for p in &frag.processes {
            let Some(&from) = pid_index.get(&(frag.hostname.clone(), p.pid)) else {
                continue;
            };

            // 1) config-file confirmed dependencies (strongest).
            for cfg in &p.config_files {
                for ep in &cfg.extracted_endpoints {
                    if let Some(port) = ep.parsed_port {
                        if let Some(to) =
                            resolve_target(nodes, port_index, ep.parsed_host.as_deref(), port)
                        {
                            if to != from {
                                insert(ArchEdge {
                                    from,
                                    to,
                                    via: EdgeSource::ConfigFile,
                                    confidence: Confidence::High,
                                    detail: Some(ep.key.clone()),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2) observed TCP connections.
        for c in &frag.connections {
            let Some(pid) = c.pid else { continue };
            let Some(&from) = pid_index.get(&(frag.hostname.clone(), pid)) else {
                continue;
            };
            if let Some(to) = resolve_target(nodes, port_index, Some(&c.remote_addr), c.remote_port)
            {
                if to != from {
                    insert(ArchEdge {
                        from,
                        to,
                        via: EdgeSource::TcpConnection,
                        confidence: Confidence::Medium,
                        detail: Some(format!("{}:{}", c.remote_addr, c.remote_port)),
                    });
                }
            }
        }
    }

    let mut out: Vec<ArchEdge> = edges.into_values().collect();
    out.sort_by_key(|e| (e.from, e.to));
    out
}

fn edge_rank(s: EdgeSource) -> u8 {
    match s {
        EdgeSource::ConfigFile => 3,
        EdgeSource::TcpConnection => 2,
        EdgeSource::PortTyping => 1,
    }
}

/// Resolve a dependency target node from a host hint + port.
///
/// Tries an exact (host, port) match first; falls back to any node listening on
/// that port (covers loopback / IP-vs-hostname mismatches in a single-host demo).
fn resolve_target(
    nodes: &[ArchNode],
    port_index: &HashMap<(String, u16), usize>,
    host_hint: Option<&str>,
    port: u16,
) -> Option<usize> {
    if let Some(host) = host_hint {
        if let Some(&id) = port_index.get(&(host.to_string(), port)) {
            return Some(id);
        }
    }
    // Fall back: unique node listening on this port anywhere.
    let mut found = None;
    for n in nodes {
        if n.ports.contains(&port) {
            if found.is_some() {
                return None; // ambiguous
            }
            found = Some(n.id);
        }
    }
    found
}

/// Group application nodes into L0 business applications via connected components
/// over the dependency graph, and name them deterministically.
fn build_groups(nodes: &mut [ArchNode], edges: &[ArchEdge]) -> Vec<ArchGroup> {
    let n = nodes.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        // path compression
        let mut cur = x;
        while parent[cur] != r {
            let next = parent[cur];
            parent[cur] = r;
            cur = next;
        }
        r
    }

    for e in edges {
        let a = find(&mut parent, e.from);
        let b = find(&mut parent, e.to);
        if a != b {
            parent[a] = b;
        }
    }

    // Collect members per root.
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        members.entry(r).or_default().push(i);
    }

    // out-degree for anchor selection.
    let mut out_deg = vec![0usize; n];
    for e in edges {
        out_deg[e.from] += 1;
    }

    let mut groups = Vec::new();
    let mut roots: Vec<usize> = members.keys().copied().collect();
    roots.sort();
    for root in roots {
        let mut mem = members.remove(&root).unwrap();
        mem.sort();
        let gid = groups.len();
        // Anchor = the most-depended-from node (front service), tie-break by layer.
        let anchor = *mem
            .iter()
            .max_by_key(|&&i| (out_deg[i], layer_rank(&nodes[i].layer)))
            .unwrap();
        let name = nodes[anchor].name.clone();
        let conf = group_confidence(&mem, nodes);
        for &m in &mem {
            nodes[m].group = Some(gid);
        }
        groups.push(ArchGroup {
            id: gid,
            name,
            named_by_ai: false,
            confidence: conf,
            member_nodes: mem,
        });
    }
    groups
}

fn layer_rank(layer: &str) -> u8 {
    match layer {
        "Web" => 3,
        "Service" => 2,
        _ => 1,
    }
}

fn group_confidence(members: &[usize], nodes: &[ArchNode]) -> Confidence {
    let avg = members
        .iter()
        .map(|&i| nodes[i].confidence.score())
        .sum::<f32>()
        / members.len().max(1) as f32;
    if avg >= 0.85 {
        Confidence::High
    } else if avg >= 0.6 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

/// Optional LLM pass: replace deterministic group names with business names.
///
/// Sends only a **redacted, abstract** summary (roles + tech, no hosts/paths)
/// through the sovereign router, which decides local-vs-frontier. On any error
/// or with the mock provider, the deterministic names are kept.
pub async fn name_groups(view: &mut ArchitectureView, router: &InferenceRouter) -> Option<()> {
    if view.groups.is_empty() {
        return Some(());
    }
    let mut summary = String::from("Name each application group with a concise business name.\n");
    summary.push_str("Reply ONLY with a JSON object mapping group index to name, e.g. {\"0\":\"Payment Platform\"}.\n\n");
    summary.push_str("\"groups\":\n");
    for g in &view.groups {
        let roles: Vec<String> = g
            .member_nodes
            .iter()
            .map(|&i| {
                let nnode = &view.nodes[i];
                match &nnode.technology {
                    Some(t) => format!("{} ({})", t, nnode.layer),
                    None => format!("{} ({})", nnode.process_name, nnode.layer),
                }
            })
            .collect();
        summary.push_str(&format!("  group {}: {}\n", g.id, roles.join(", ")));
    }

    let req = CompletionRequest {
        system: "You are a solutions architect. You name groups of IT components as business applications.".to_string(),
        user: summary,
        max_tokens: 256,
    };
    let routed = router
        .complete("architect_name", &req)
        .await
        .ok()?
        .response
        .text;

    // Parse {"0":"name", ...}
    let parsed: serde_json::Value = serde_json::from_str(routed.trim()).ok()?;
    let obj = parsed.as_object()?;
    if obj.is_empty() {
        return Some(()); // keep deterministic names (mock path)
    }
    for (k, v) in obj {
        if let (Ok(idx), Some(name)) = (k.parse::<usize>(), v.as_str()) {
            if let Some(g) = view.groups.get_mut(idx) {
                g.name = name.to_string();
                g.named_by_ai = true;
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcontrol_common::types::{
        CommandSuggestion, DiscoveredConfigFile, ExtractedEndpoint, TechnologyHint,
    };

    fn proc(pid: u32, name: &str, ports: Vec<u16>) -> DiscoveredProcess {
        DiscoveredProcess {
            pid,
            name: name.to_string(),
            cmdline: name.to_string(),
            user: "app".to_string(),
            domain: None,
            memory_bytes: 0,
            cpu_pct: 0.0,
            listening_ports: ports,
            env_vars: Default::default(),
            working_dir: None,
            config_files: vec![],
            log_files: vec![],
            command_suggestion: None,
            matched_service: None,
            technology_hint: None,
        }
    }

    #[test]
    fn distinguishes_apps_from_system_noise() {
        let mut sshd = proc(10, "sshd", vec![22]);
        sshd.user = "root".to_string();
        let systemd = proc(1, "systemd", vec![]);
        let api = proc(100, "java", vec![8080]);
        let frag = Fragment {
            hostname: "srv-01".to_string(),
            processes: vec![sshd, systemd, api],
            listeners: vec![],
            connections: vec![],
            services: vec![],
        };
        let view = build(&[frag]);
        // Only the java service is an application; sshd + systemd are hidden.
        assert_eq!(view.application_nodes().count(), 1);
        assert_eq!(view.system_processes_hidden, 2);
        assert_eq!(view.nodes[0].layer, "Service");
    }

    #[test]
    fn builds_config_dependency_and_groups() {
        // order-api (java :8080) -> postgres (:5432) via config.
        let mut api = proc(100, "java", vec![8080]);
        api.config_files = vec![DiscoveredConfigFile {
            path: "/opt/order/app.yml".to_string(),
            extracted_endpoints: vec![ExtractedEndpoint {
                key: "spring.datasource.url".to_string(),
                value: "jdbc:postgresql://localhost:5432/orders".to_string(),
                parsed_host: Some("localhost".to_string()),
                parsed_port: Some(5432),
                technology: Some("postgresql".to_string()),
            }],
        }];
        let mut pg = proc(200, "postgres", vec![5432]);
        pg.technology_hint = Some(TechnologyHint {
            id: "postgresql".to_string(),
            display_name: "PostgreSQL".to_string(),
            icon: "postgresql".to_string(),
            layer: "Database".to_string(),
        });
        pg.command_suggestion = Some(CommandSuggestion {
            check_cmd: "systemctl is-active postgresql".to_string(),
            start_cmd: Some("systemctl start postgresql".to_string()),
            stop_cmd: Some("systemctl stop postgresql".to_string()),
            restart_cmd: None,
            logs_cmd: None,
            version_cmd: None,
            confidence: "high".to_string(),
            source: "systemd".to_string(),
        });
        let frag = Fragment {
            hostname: "localhost".to_string(),
            processes: vec![api, pg],
            listeners: vec![],
            connections: vec![],
            services: vec![],
        };
        let view = build(&[frag]);
        assert_eq!(view.application_nodes().count(), 2);
        assert_eq!(view.edges.len(), 1);
        assert_eq!(view.edges[0].via, EdgeSource::ConfigFile);
        // Both components belong to the same application group.
        assert_eq!(view.groups.len(), 1);
        assert_eq!(view.groups[0].member_nodes.len(), 2);
    }
}
