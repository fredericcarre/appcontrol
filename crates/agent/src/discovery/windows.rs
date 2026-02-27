//! Windows-specific discovery: netstat -ano parsing for TCP,
//! sc query for services.
//!
//! ## Why netstat instead of Win32 API?
//!
//! The `GetExtendedTcpTable` API requires elevated privileges and unsafe FFI.
//! `netstat -ano` is available on all Windows versions since XP, runs without
//! admin rights, and provides PID→port mapping out of the box. It's the most
//! reliable cross-version approach.

use std::collections::HashMap;
use sysinfo::System;

use appcontrol_common::{DiscoveredConnection, DiscoveredListener, DiscoveredService};

/// Scan TCP listeners and connections by parsing `netstat -ano` output.
pub fn scan_network(_sys: &System) -> (Vec<DiscoveredListener>, Vec<DiscoveredConnection>) {
    use std::process::Command;

    let output = Command::new("netstat").args(["-ano", "-p", "TCP"]).output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return (Vec::new(), Vec::new()),
    };

    let mut listeners = Vec::new();
    let mut connections = Vec::new();

    // Build PID → process name map from sysinfo
    let pid_names: HashMap<u32, String> = _sys
        .processes()
        .iter()
        .map(|(pid, p)| (pid.as_u32(), p.name().to_string_lossy().to_string()))
        .collect();

    // netstat -ano output looks like:
    //   Proto  Local Address          Foreign Address        State           PID
    //   TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1052
    //   TCP    10.0.0.5:49312         10.0.0.1:5432          ESTABLISHED     4128
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with("TCP") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let local = parts[1];
        let remote = parts[2];
        let state = parts[3];
        let pid_str = parts[4];

        let pid: u32 = pid_str.parse().unwrap_or(0);
        let process_name = pid_names.get(&pid).cloned();

        if state == "LISTENING" {
            if let Some((addr, port)) = parse_netstat_addr(local) {
                listeners.push(DiscoveredListener {
                    port,
                    protocol: "tcp".to_string(),
                    pid: if pid > 0 { Some(pid) } else { None },
                    process_name,
                    address: addr,
                });
            }
        } else if state == "ESTABLISHED" {
            let local_parsed = parse_netstat_addr(local);
            let remote_parsed = parse_netstat_addr(remote);

            if let (Some((_, local_port)), Some((remote_addr, remote_port))) =
                (local_parsed, remote_parsed)
            {
                // Skip localhost connections
                if remote_addr == "127.0.0.1" || remote_addr == "0.0.0.0" {
                    continue;
                }
                connections.push(DiscoveredConnection {
                    local_port,
                    remote_addr,
                    remote_port,
                    pid: if pid > 0 { Some(pid) } else { None },
                    process_name,
                    state: "ESTABLISHED".to_string(),
                });
            }
        }
    }

    (listeners, connections)
}

/// Parse "addr:port" from netstat output. Handles IPv4 ("0.0.0.0:135")
/// and IPv6 ("[::]:135" or "[::1]:135").
fn parse_netstat_addr(s: &str) -> Option<(String, u16)> {
    if let Some(bracket_end) = s.rfind(']') {
        // IPv6: [::]:port or [::1]:port
        let addr = &s[1..bracket_end]; // strip [ ]
        let port_str = &s[bracket_end + 2..]; // skip ]:
        let port: u16 = port_str.parse().ok()?;
        Some((addr.to_string(), port))
    } else {
        // IPv4: addr:port — split on last ':'
        let colon = s.rfind(':')?;
        let addr = &s[..colon];
        let port: u16 = s[colon + 1..].parse().ok()?;
        Some((addr.to_string(), port))
    }
}

/// Discover Windows services via `sc query`.
pub fn scan_services() -> Vec<DiscoveredService> {
    use std::process::Command;

    let output = Command::new("sc")
        .args(["query", "type=", "service"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut services = Vec::new();
    let mut current_name = String::new();
    let mut current_display = String::new();
    let mut current_status = String::new();
    let mut current_pid: Option<u32> = None;

    for line in output.lines() {
        let line = line.trim();

        if let Some(name) = line.strip_prefix("SERVICE_NAME: ") {
            // Flush previous service
            if !current_name.is_empty() {
                services.push(DiscoveredService {
                    name: current_name.clone(),
                    display_name: current_display.clone(),
                    status: current_status.clone(),
                    pid: current_pid,
                });
            }
            current_name = name.to_string();
            current_display = name.to_string();
            current_status = String::new();
            current_pid = None;
        } else if let Some(display) = line.strip_prefix("DISPLAY_NAME: ") {
            current_display = display.to_string();
        } else if line.contains("STATE") {
            // "        STATE              : 4  RUNNING"
            if line.contains("RUNNING") {
                current_status = "running".to_string();
            } else if line.contains("STOPPED") {
                current_status = "stopped".to_string();
            } else if line.contains("PAUSED") {
                current_status = "paused".to_string();
            } else {
                current_status = "unknown".to_string();
            }
        } else if let Some(pid_part) = line.strip_prefix("PID") {
            // "        PID                : 1234"
            if let Some(colon_idx) = pid_part.find(':') {
                let pid_str = pid_part[colon_idx + 1..].trim();
                current_pid = pid_str.parse().ok().filter(|&p| p > 0);
            }
        }
    }

    // Flush last service
    if !current_name.is_empty() {
        services.push(DiscoveredService {
            name: current_name,
            display_name: current_display,
            status: current_status,
            pid: current_pid,
        });
    }

    services
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_netstat_addr_ipv4() {
        assert_eq!(
            parse_netstat_addr("0.0.0.0:135"),
            Some(("0.0.0.0".to_string(), 135))
        );
        assert_eq!(
            parse_netstat_addr("10.0.0.5:8080"),
            Some(("10.0.0.5".to_string(), 8080))
        );
    }

    #[test]
    fn test_parse_netstat_addr_ipv6() {
        assert_eq!(parse_netstat_addr("[::]:80"), Some(("::".to_string(), 80)));
        assert_eq!(
            parse_netstat_addr("[::1]:443"),
            Some(("::1".to_string(), 443))
        );
    }
}
