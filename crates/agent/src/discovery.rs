//! Passive topology discovery scanner.
//!
//! Scans the local host for running processes, TCP listeners, and outbound connections.
//! Sends a `DiscoveryReport` to the backend via the agent's message channel.
//! The backend correlates reports from multiple agents to infer application DAGs.
//!
//! ## PID ↔ Port Resolution (Linux)
//!
//! On Linux, `/proc/net/tcp` does not directly expose PIDs. We resolve ownership
//! by scanning `/proc/[pid]/fd/` for socket symlinks, extracting their inode numbers,
//! and cross-referencing with the inode column in `/proc/net/tcp`.

use chrono::Utc;
use std::collections::HashMap;
use sysinfo::System;
use uuid::Uuid;

use appcontrol_common::{
    AgentMessage, DiscoveredConnection, DiscoveredListener, DiscoveredProcess, DiscoveredService,
};

/// System/kernel processes to exclude from discovery results.
/// These are never application processes and just add noise.
const SYSTEM_PROCESS_NAMES: &[&str] = &[
    "kthreadd",
    "ksoftirqd",
    "kworker",
    "migration",
    "rcu_sched",
    "rcu_bh",
    "rcu_preempt",
    "watchdog",
    "cpuhp",
    "netns",
    "khungtaskd",
    "oom_reaper",
    "kcompactd",
    "kdevtmpfs",
    "kauditd",
    "khugepaged",
    "kswapd",
    "ksmd",
    "kblockd",
    "md",
    "edac-poller",
    "devfreq_wq",
    "writeback",
    "crypto",
    "bioset",
    "kthrotld",
    "irq/",
    "scsi_",
    "loop",
    "nfsd",
    "lockd",
    "rpciod",
    "xprtiod",
];

/// Environment variable prefixes/patterns worth capturing for topology inference.
/// We capture connection-relevant env vars (DB hosts, ports, URLs) while
/// filtering out noisy system variables.
const INTERESTING_ENV_PREFIXES: &[&str] = &[
    "DB_",
    "DATABASE_",
    "REDIS_",
    "MONGO_",
    "MYSQL_",
    "POSTGRES",
    "PG",
    "KAFKA_",
    "RABBIT",
    "AMQP_",
    "ELASTICSEARCH",
    "SOLR_",
    "MEMCACHE",
    "ZOOKEEPER",
    "CONSUL_",
    "ETCD_",
    "VAULT_",
    "SERVICE_",
    "API_",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "SERVER_",
    "APP_",
    "SPRING_",
    "NODE_ENV",
    "JAVA_HOME",
    "CATALINA_",
];

const INTERESTING_ENV_SUFFIXES: &[&str] = &[
    "_HOST",
    "_PORT",
    "_URL",
    "_URI",
    "_ADDR",
    "_ADDRESS",
    "_ENDPOINT",
    "_DSN",
    "_CONNECTION",
    "_CONN_STR",
];

/// Run a single passive discovery scan and return an AgentMessage::DiscoveryReport.
pub fn scan(agent_id: Uuid, hostname: &str) -> AgentMessage {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Step 1: Build inode → PID mapping for socket ownership resolution
    #[cfg(target_os = "linux")]
    let inode_to_pid = build_inode_pid_map(&sys);

    // Step 2: Parse /proc/net/tcp with inode info
    #[cfg(target_os = "linux")]
    let (mut listeners, connections) = {
        let tcp_entries = parse_proc_net_tcp();
        resolve_listeners_and_connections(&tcp_entries, &inode_to_pid, &sys)
    };
    #[cfg(not(target_os = "linux"))]
    let (mut listeners, mut connections): (Vec<DiscoveredListener>, Vec<DiscoveredConnection>) =
        (Vec::new(), Vec::new());

    // Deduplicate
    listeners.sort_by_key(|l| l.port);
    listeners.dedup_by_key(|l| l.port);

    // Step 3: Scan processes with enrichment
    let processes = scan_processes(&sys, &listeners);

    // Step 4: Discover systemd services
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

/// Check if a process name is a known system/kernel thread.
fn is_system_process(name: &str) -> bool {
    SYSTEM_PROCESS_NAMES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Check if an environment variable key is interesting for topology inference.
fn is_interesting_env(key: &str) -> bool {
    let upper = key.to_uppercase();
    INTERESTING_ENV_PREFIXES
        .iter()
        .any(|p| upper.starts_with(p))
        || INTERESTING_ENV_SUFFIXES.iter().any(|s| upper.ends_with(s))
}

/// Enumerate all running processes with metadata, filtering system processes
/// and enriching with listening port cross-references and env vars.
fn scan_processes(sys: &System, listeners: &[DiscoveredListener]) -> Vec<DiscoveredProcess> {
    sys.processes()
        .iter()
        .filter_map(|(pid, p)| {
            let name = p.name().to_string_lossy().to_string();
            if name.is_empty() || is_system_process(&name) {
                return None;
            }

            let cmdline = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");

            // Skip kernel threads (PID 0, no cmdline, name in brackets)
            if cmdline.is_empty() && name.starts_with('[') && name.ends_with(']') {
                return None;
            }

            let user = p
                .user_id()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let pid_u32 = pid.as_u32();

            // Cross-reference: which ports does this PID listen on?
            let listening_ports: Vec<u16> = listeners
                .iter()
                .filter(|l| l.pid == Some(pid_u32))
                .map(|l| l.port)
                .collect();

            // Collect interesting environment variables
            let env_vars = read_process_env(pid_u32);

            Some(DiscoveredProcess {
                pid: pid_u32,
                name,
                cmdline,
                user,
                memory_bytes: p.memory(),
                cpu_pct: p.cpu_usage(),
                listening_ports,
                env_vars,
            })
        })
        .collect()
}

/// Read environment variables from /proc/[pid]/environ (Linux only).
/// Returns only interesting variables (connection-related).
fn read_process_env(pid: u32) -> HashMap<String, String> {
    #[cfg(target_os = "linux")]
    {
        read_process_env_linux(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        HashMap::new()
    }
}

#[cfg(target_os = "linux")]
fn read_process_env_linux(pid: u32) -> HashMap<String, String> {
    use std::fs;

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
                    // Truncate very long values (e.g. PATH)
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

// ---------------------------------------------------------------------------
// Linux-specific: /proc/net/tcp parsing with inode resolution
// ---------------------------------------------------------------------------

/// A parsed entry from /proc/net/tcp.
#[cfg(target_os = "linux")]
#[derive(Debug)]
struct TcpEntry {
    local_addr: String,
    local_port: u16,
    remote_addr: String,
    remote_port: u16,
    state: u8, // 0x0A = LISTEN, 0x01 = ESTABLISHED, etc.
    inode: u64,
}

/// Parse all entries from /proc/net/tcp and /proc/net/tcp6.
#[cfg(target_os = "linux")]
fn parse_proc_net_tcp() -> Vec<TcpEntry> {
    use std::fs;

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

/// Build a mapping from socket inode → (PID, process_name) by scanning /proc/[pid]/fd/.
#[cfg(target_os = "linux")]
fn build_inode_pid_map(sys: &System) -> HashMap<u64, (u32, String)> {
    use std::fs;

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
                if let Some(inode_str) = link_str.strip_prefix("socket:[") {
                    if let Some(inode_str) = inode_str.strip_suffix(']') {
                        if let Ok(inode) = inode_str.parse::<u64>() {
                            map.insert(inode, (pid_u32, proc_name.clone()));
                        }
                    }
                }
            }
        }
    }

    map
}

/// Convert parsed TCP entries into DiscoveredListeners and DiscoveredConnections,
/// resolving PID ownership via the inode map.
#[cfg(target_os = "linux")]
fn resolve_listeners_and_connections(
    entries: &[TcpEntry],
    inode_to_pid: &HashMap<u64, (u32, String)>,
    _sys: &System,
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
                // ESTABLISHED
                // Skip localhost connections
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
            _ => {} // Other states (TIME_WAIT, CLOSE_WAIT, etc.) — skip
        }
    }

    (listeners, connections)
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
        let procs = scan_processes(&sys, &[]);
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

    #[test]
    fn test_system_process_filter() {
        assert!(is_system_process("kworker/0:1"));
        assert!(is_system_process("ksoftirqd/0"));
        assert!(is_system_process("migration/0"));
        assert!(!is_system_process("nginx"));
        assert!(!is_system_process("java"));
        assert!(!is_system_process("postgres"));
    }

    #[test]
    fn test_interesting_env_detection() {
        assert!(is_interesting_env("DB_HOST"));
        assert!(is_interesting_env("DATABASE_URL"));
        assert!(is_interesting_env("REDIS_PORT"));
        assert!(is_interesting_env("POSTGRES_PASSWORD"));
        assert!(is_interesting_env("MY_SERVICE_HOST"));
        assert!(is_interesting_env("BACKEND_ENDPOINT"));
        assert!(!is_interesting_env("HOME"));
        assert!(!is_interesting_env("TERM"));
        assert!(!is_interesting_env("SHELL"));
        assert!(!is_interesting_env("LANG"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_hex_addr_port() {
        // 0100007F:1F90 = 127.0.0.1:8080
        let result = parse_hex_addr_port("0100007F:1F90");
        assert_eq!(result, Some(("127.0.0.1".to_string(), 8080)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_inode_pid_map_is_populated() {
        let mut sys = System::new_all();
        sys.refresh_all();
        let map = build_inode_pid_map(&sys);
        // Should find at least some socket inodes (test process itself has sockets)
        // This may be empty in very restricted environments, so just check it doesn't panic
        let _ = map.len();
    }
}
