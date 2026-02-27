//! Linux-specific discovery: /proc/net/tcp, /proc/[pid]/fd inode scanning,
//! /proc/[pid]/environ, and systemctl for services.

use std::collections::HashMap;
use std::fs;
use sysinfo::System;

use appcontrol_common::{DiscoveredConnection, DiscoveredListener, DiscoveredService};

use super::is_interesting_env;

/// A parsed entry from /proc/net/tcp.
struct TcpEntry {
    local_addr: String,
    local_port: u16,
    remote_addr: String,
    remote_port: u16,
    state: u8, // 0x0A = LISTEN, 0x01 = ESTABLISHED
    inode: u64,
}

/// Scan TCP listeners and connections with PID resolution via inode mapping.
pub fn scan_network(sys: &System) -> (Vec<DiscoveredListener>, Vec<DiscoveredConnection>) {
    let inode_to_pid = build_inode_pid_map(sys);
    let tcp_entries = parse_proc_net_tcp();
    resolve_entries(&tcp_entries, &inode_to_pid)
}

/// Build a mapping from socket inode → (PID, process_name) by scanning /proc/[pid]/fd/.
fn build_inode_pid_map(sys: &System) -> HashMap<u64, (u32, String)> {
    let mut map = HashMap::new();

    for (pid, p) in sys.processes() {
        let pid_u32 = pid.as_u32();
        let fd_dir = format!("/proc/{}/fd", pid_u32);

        let entries = match fs::read_dir(&fd_dir) {
            Ok(e) => e,
            Err(_) => continue, // Permission denied
        };

        let proc_name = p.name().to_string_lossy().to_string();

        for entry in entries.flatten() {
            if let Ok(link) = fs::read_link(entry.path()) {
                let link_str = link.to_string_lossy();
                // Socket symlinks look like: socket:[12345]
                if let Some(inode_str) = link_str
                    .strip_prefix("socket:[")
                    .and_then(|s| s.strip_suffix(']'))
                {
                    if let Ok(inode) = inode_str.parse::<u64>() {
                        map.insert(inode, (pid_u32, proc_name.clone()));
                    }
                }
            }
        }
    }

    map
}

/// Parse all entries from /proc/net/tcp and /proc/net/tcp6.
fn parse_proc_net_tcp() -> Vec<TcpEntry> {
    let mut entries = Vec::new();

    for path in &["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 10 {
                    continue;
                }

                let state = u8::from_str_radix(fields[3], 16).unwrap_or(0);
                let inode = fields[9].parse::<u64>().unwrap_or(0);

                let local = parse_hex_addr_port(fields[1]);
                let remote = parse_hex_addr_port(fields[2]);

                if let (Some((la, lp)), Some((ra, rp))) = (local, remote) {
                    entries.push(TcpEntry {
                        local_addr: la,
                        local_port: lp,
                        remote_addr: ra,
                        remote_port: rp,
                        state,
                        inode,
                    });
                }
            }
        }
    }

    entries
}

/// Convert parsed TCP entries into listeners + connections with PID resolution.
fn resolve_entries(
    entries: &[TcpEntry],
    inode_to_pid: &HashMap<u64, (u32, String)>,
) -> (Vec<DiscoveredListener>, Vec<DiscoveredConnection>) {
    let mut listeners = Vec::new();
    let mut connections = Vec::new();

    for entry in entries {
        let owner = inode_to_pid.get(&entry.inode);
        let pid = owner.map(|(p, _)| *p);
        let process_name = owner.map(|(_, n)| n.clone());

        match entry.state {
            0x0A => {
                // LISTEN
                listeners.push(DiscoveredListener {
                    port: entry.local_port,
                    protocol: "tcp".to_string(),
                    pid,
                    process_name,
                    address: entry.local_addr.clone(),
                });
            }
            0x01 => {
                // ESTABLISHED — skip localhost connections
                if entry.remote_addr == "127.0.0.1" || entry.remote_addr == "0.0.0.0" {
                    continue;
                }
                connections.push(DiscoveredConnection {
                    local_port: entry.local_port,
                    remote_addr: entry.remote_addr.clone(),
                    remote_port: entry.remote_port,
                    pid,
                    process_name,
                    state: "ESTABLISHED".to_string(),
                });
            }
            _ => {} // TIME_WAIT, CLOSE_WAIT, etc.
        }
    }

    (listeners, connections)
}

/// Parse hex address:port from /proc/net/tcp (e.g. "0100007F:1F90" → ("127.0.0.1", 8080)).
fn parse_hex_addr_port(s: &str) -> Option<(String, u16)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let port = u16::from_str_radix(parts[1], 16).ok()?;
    let addr_hex = parts[0];

    let addr = if addr_hex.len() == 8 {
        // IPv4 — stored little-endian
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

/// Read interesting environment variables from /proc/[pid]/environ.
pub fn read_process_env(pid: u32) -> HashMap<String, String> {
    let path = format!("/proc/{}/environ", pid);
    let content = match fs::read(&path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(), // Permission denied or process gone
    };

    let mut env = HashMap::new();
    // /proc/[pid]/environ uses null bytes as separators
    for entry in content.split(|&b| b == 0) {
        if let Ok(s) = std::str::from_utf8(entry) {
            if let Some((key, value)) = s.split_once('=') {
                if is_interesting_env(key) {
                    let val = if value.len() > 500 {
                        format!("{}...", &value[..500])
                    } else {
                        value.to_string()
                    };
                    env.insert(key.to_string(), val);
                }
            }
        }
    }
    env
}

/// Discover systemd services via systemctl.
pub fn scan_services() -> Vec<DiscoveredService> {
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
            let status = parts[3].to_string();
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
    fn test_parse_hex_addr_port() {
        let result = parse_hex_addr_port("0100007F:1F90");
        assert_eq!(result, Some(("127.0.0.1".to_string(), 8080)));
    }

    #[test]
    fn test_inode_pid_map_is_populated() {
        let mut sys = System::new_all();
        sys.refresh_all();
        let map = build_inode_pid_map(&sys);
        let _ = map.len(); // Should not panic
    }
}
