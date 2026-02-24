//! Passive topology discovery scanner.
//!
//! Scans the local host for running processes, TCP listeners, and outbound connections.
//! Sends a `DiscoveryReport` to the backend via the agent's message channel.
//! The backend correlates reports from multiple agents to infer application DAGs.

use chrono::Utc;
use sysinfo::System;
use uuid::Uuid;

use appcontrol_common::{
    AgentMessage, DiscoveredConnection, DiscoveredListener, DiscoveredProcess, DiscoveredService,
};

/// Run a single passive discovery scan and return an AgentMessage::DiscoveryReport.
pub fn scan(agent_id: Uuid, hostname: &str) -> AgentMessage {
    let mut sys = System::new_all();
    sys.refresh_all();

    let processes = scan_processes(&sys);
    let listeners = scan_listeners();
    let connections = scan_connections();
    let services = scan_services();

    AgentMessage::DiscoveryReport {
        agent_id,
        hostname: hostname.to_string(),
        processes,
        listeners,
        connections,
        services,
        scanned_at: Utc::now(),
    }
}

/// Enumerate all running processes with metadata.
fn scan_processes(sys: &System) -> Vec<DiscoveredProcess> {
    sys.processes()
        .iter()
        .map(|(pid, p)| {
            let cmdline = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let name = p.name().to_string_lossy().to_string();
            let user = p
                .user_id()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            DiscoveredProcess {
                pid: pid.as_u32(),
                name,
                cmdline,
                user,
                memory_bytes: p.memory(),
                cpu_pct: p.cpu_usage(),
                listening_ports: Vec::new(), // filled in by cross-referencing listeners
            }
        })
        .filter(|p| !p.name.is_empty())
        .collect()
}

/// Scan TCP listeners by reading /proc/net/tcp (Linux) or using platform APIs.
fn scan_listeners() -> Vec<DiscoveredListener> {
    #[cfg(target_os = "linux")]
    {
        scan_listeners_linux()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn scan_listeners_linux() -> Vec<DiscoveredListener> {
    use std::fs;

    let mut listeners = Vec::new();

    // Parse /proc/net/tcp and /proc/net/tcp6
    for path in &["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 4 {
                    continue;
                }

                // State 0A = LISTEN
                let state = fields[3];
                if state != "0A" {
                    continue;
                }

                // Parse local address:port (hex)
                if let Some((addr, port)) = parse_hex_addr_port(fields[1]) {
                    listeners.push(DiscoveredListener {
                        port,
                        protocol: "tcp".to_string(),
                        pid: None, // would need /proc/*/fd scanning, skip for now
                        process_name: None,
                        address: addr,
                    });
                }
            }
        }
    }

    listeners.sort_by_key(|l| l.port);
    listeners.dedup_by_key(|l| l.port);
    listeners
}

#[cfg(target_os = "linux")]
fn parse_hex_addr_port(s: &str) -> Option<(String, u16)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let port = u16::from_str_radix(parts[1], 16).ok()?;
    let addr_hex = parts[0];

    let addr = if addr_hex.len() == 8 {
        // IPv4
        let bytes = u32::from_str_radix(addr_hex, 16).ok()?;
        let a = bytes & 0xFF;
        let b = (bytes >> 8) & 0xFF;
        let c = (bytes >> 16) & 0xFF;
        let d = (bytes >> 24) & 0xFF;
        format!("{}.{}.{}.{}", a, b, c, d)
    } else {
        "::".to_string()
    };

    Some((addr, port))
}

/// Scan outbound TCP connections.
fn scan_connections() -> Vec<DiscoveredConnection> {
    #[cfg(target_os = "linux")]
    {
        scan_connections_linux()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn scan_connections_linux() -> Vec<DiscoveredConnection> {
    use std::fs;

    let mut connections = Vec::new();

    if let Ok(content) = fs::read_to_string("/proc/net/tcp") {
        for line in content.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 {
                continue;
            }

            // State 01 = ESTABLISHED
            let state = fields[3];
            if state != "01" {
                continue;
            }

            let local = parse_hex_addr_port(fields[1]);
            let remote = parse_hex_addr_port(fields[2]);

            if let (Some((_, local_port)), Some((remote_addr, remote_port))) = (local, remote) {
                // Skip connections to localhost
                if remote_addr == "127.0.0.1" || remote_addr == "0.0.0.0" {
                    continue;
                }

                connections.push(DiscoveredConnection {
                    local_port,
                    remote_addr,
                    remote_port,
                    pid: None,
                    process_name: None,
                    state: "ESTABLISHED".to_string(),
                });
            }
        }
    }

    connections
}

/// Discover systemd services (Linux) or Windows services.
fn scan_services() -> Vec<DiscoveredService> {
    #[cfg(target_os = "linux")]
    {
        scan_services_linux()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn scan_services_linux() -> Vec<DiscoveredService> {
    // List systemd units via /run/systemd/units or systemctl.
    // For safety and simplicity, parse /run/systemd/units if available.
    // Fallback: try running systemctl (non-blocking).
    use std::process::Command;

    let output = Command::new("systemctl")
        .args(["list-units", "--type=service", "--no-pager", "--no-legend"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return None;
            }
            let name = parts[0].trim_end_matches(".service").to_string();
            let status = parts[3].to_string(); // "running", "exited", "dead"
            Some(DiscoveredService {
                name: name.clone(),
                display_name: name,
                status,
                pid: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_processes_returns_results() {
        let mut sys = System::new_all();
        sys.refresh_all();
        let procs = scan_processes(&sys);
        // There should be at least the current test process
        assert!(!procs.is_empty());
    }

    #[test]
    fn test_scan_returns_discovery_report() {
        let agent_id = Uuid::new_v4();
        let msg = scan(agent_id, "test-host");
        match msg {
            AgentMessage::DiscoveryReport {
                agent_id: aid,
                hostname,
                processes,
                ..
            } => {
                assert_eq!(aid, agent_id);
                assert_eq!(hostname, "test-host");
                assert!(!processes.is_empty());
            }
            _ => panic!("Expected DiscoveryReport"),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_hex_addr_port() {
        // 0100007F:1F90 = 127.0.0.1:8080
        let result = parse_hex_addr_port("0100007F:1F90");
        assert_eq!(result, Some(("127.0.0.1".to_string(), 8080)));
    }
}
