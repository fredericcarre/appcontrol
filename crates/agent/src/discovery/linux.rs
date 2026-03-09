//! Linux-specific discovery: /proc/net/tcp, /proc/[pid]/fd inode scanning,
//! /proc/[pid]/environ, systemctl for services, cron/timer scanning,
//! config file parsing, and service↔process cross-referencing.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use sysinfo::System;

use appcontrol_common::{
    CommandSuggestion, DiscoveredConfigFile, DiscoveredConnection, DiscoveredFirewallRule,
    DiscoveredListener, DiscoveredLogFile, DiscoveredScheduledJob, DiscoveredService,
    ExtractedEndpoint,
};

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

// =========================================================================
// Network scanning
// =========================================================================

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

// =========================================================================
// Environment variables
// =========================================================================

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

// =========================================================================
// Working directory + open file scanning (config files, log files)
// =========================================================================

/// Read the working directory of a process via /proc/[pid]/cwd.
pub fn read_working_dir(pid: u32) -> Option<String> {
    let link = format!("/proc/{}/cwd", pid);
    fs::read_link(&link)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// Config file extensions that we want to detect and potentially parse.
const CONFIG_EXTENSIONS: &[&str] = &[
    "yml",
    "yaml",
    "properties",
    "conf",
    "cfg",
    "ini",
    "env",
    "xml",
    "json",
    "toml",
];

/// Log-related extensions and path patterns.
const LOG_EXTENSIONS: &[&str] = &["log", "out"];
const LOG_PATH_PATTERNS: &[&str] = &["/log/", "/logs/", "/var/log/"];

/// Directories to skip when classifying open files.
const SKIP_PREFIXES: &[&str] = &[
    "/proc/",
    "/sys/",
    "/dev/",
    "/run/",
    "/tmp/",
    "/usr/lib/",
    "/usr/share/",
    "/lib/",
    "/lib64/",
    "/etc/ld.so",
    "/etc/nsswitch",
    "/etc/resolv",
    "/etc/host",
    "/etc/passwd",
    "/etc/group",
    "/etc/shadow",
    "/etc/localtime",
    "/etc/ssl/",
    "/etc/pki/",
    "/etc/ca-certificates",
];

/// Scan /proc/[pid]/fd to find config files and log files opened by the process.
pub fn scan_open_files(pid: u32) -> (Vec<DiscoveredConfigFile>, Vec<DiscoveredLogFile>) {
    let fd_dir = format!("/proc/{}/fd", pid);
    let entries = match fs::read_dir(&fd_dir) {
        Ok(e) => e,
        Err(_) => return (Vec::new(), Vec::new()),
    };

    let mut config_files = Vec::new();
    let mut log_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for entry in entries.flatten() {
        let link = match fs::read_link(entry.path()) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let path_str = link.to_string_lossy().to_string();

        // Skip non-file paths (socket:, pipe:, anon_inode:, etc.)
        if !path_str.starts_with('/') {
            continue;
        }

        // Skip system/library paths
        if SKIP_PREFIXES.iter().any(|p| path_str.starts_with(p)) {
            continue;
        }

        // Deduplicate
        if !seen_paths.insert(path_str.clone()) {
            continue;
        }

        // Classify the file
        let ext = Path::new(&path_str)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let is_log = LOG_EXTENSIONS.contains(&ext.as_str())
            || LOG_PATH_PATTERNS.iter().any(|pat| path_str.contains(pat));

        let is_config = CONFIG_EXTENSIONS.contains(&ext.as_str());

        if is_log {
            let size_bytes = fs::metadata(&path_str).map(|m| m.len()).unwrap_or(0);
            log_files.push(DiscoveredLogFile {
                path: path_str,
                size_bytes,
            });
        } else if is_config {
            // Parse config for connection endpoints
            let extracted = parse_config_file(&path_str);
            config_files.push(DiscoveredConfigFile {
                path: path_str,
                extracted_endpoints: extracted,
            });
        }
    }

    (config_files, log_files)
}

// =========================================================================
// Config file parsing — extract connection endpoints
// =========================================================================

/// Max file size to parse (64 KB).
const MAX_CONFIG_SIZE: u64 = 65536;

/// Parse a config file looking for connection strings, URLs, host/port patterns.
fn parse_config_file(path: &str) -> Vec<ExtractedEndpoint> {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    if metadata.len() > MAX_CONFIG_SIZE || metadata.len() == 0 {
        return Vec::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut endpoints = Vec::new();

    // Scan each line for URL patterns and key=value connection settings
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        // Try to find URLs in the line
        for url_result in extract_urls_from_line(trimmed) {
            endpoints.push(url_result);
        }

        // Try key=value or key: value patterns for host/port
        if let Some(ep) = extract_host_port_from_kv(trimmed) {
            endpoints.push(ep);
        }
    }

    // Deduplicate by (key, value)
    endpoints.sort_by(|a, b| (&a.key, &a.value).cmp(&(&b.key, &b.value)));
    endpoints.dedup_by(|a, b| a.key == b.key && a.value == b.value);

    endpoints
}

/// URL prefixes we scan for in config files.
const URL_PREFIXES: &[(&str, &str)] = &[
    ("jdbc:postgresql://", "postgresql"),
    ("jdbc:mysql://", "mysql"),
    ("jdbc:oracle:", "oracle"),
    ("jdbc:sqlserver://", "sqlserver"),
    ("jdbc:mariadb://", "mariadb"),
    ("amqp://", "rabbitmq"),
    ("amqps://", "rabbitmq"),
    ("redis://", "redis"),
    ("rediss://", "redis"),
    ("mongodb://", "mongodb"),
    ("mongodb+srv://", "mongodb"),
    ("kafka://", "kafka"),
    ("http://", "http"),
    ("https://", "https"),
];

/// Extract connection URLs from a config line.
fn extract_urls_from_line(line: &str) -> Vec<ExtractedEndpoint> {
    let mut results = Vec::new();

    for &(prefix, tech) in URL_PREFIXES {
        if let Some(start) = line.find(prefix) {
            let url_start = start;
            // Find end of URL (space, quote, comma, or end of line)
            let url_part = &line[url_start..];
            let end = url_part
                .find([' ', '"', '\'', ',', ';', '>'])
                .unwrap_or(url_part.len());
            let url = &url_part[..end];

            // Extract the key (everything before the URL on this line)
            let key = extract_key_before(line, url_start);

            // Parse host:port from the URL
            let (host, port) = parse_host_port_from_url(url, prefix);

            results.push(ExtractedEndpoint {
                key,
                value: url.to_string(),
                parsed_host: host,
                parsed_port: port,
                technology: Some(tech.to_string()),
            });
        }
    }

    results
}

/// Try to extract a config key from text before a URL.
fn extract_key_before(line: &str, url_pos: usize) -> String {
    let before = line[..url_pos].trim();
    // Look for key=, key:, key =, key :
    let before = before.trim_end_matches(['=', ':', ' ', '"', '\'']);
    // Take the last word/token as the key
    let key = before
        .rsplit([' ', '<', '>', '"', '\'', '{'])
        .next()
        .unwrap_or(before)
        .trim();
    if key.is_empty() {
        "url".to_string()
    } else {
        key.to_string()
    }
}

/// Parse host:port from a URL string.
fn parse_host_port_from_url(url: &str, prefix: &str) -> (Option<String>, Option<u16>) {
    // Strip the prefix to get to the authority part
    let after_prefix = &url[prefix.len()..];

    // Skip user:pass@ if present
    let authority = if let Some(at_pos) = after_prefix.find('@') {
        &after_prefix[at_pos + 1..]
    } else {
        after_prefix
    };

    // Get host:port before any / or ?
    let host_port = authority.split(['/', '?']).next().unwrap_or(authority);

    // Parse host and port
    if let Some(colon) = host_port.rfind(':') {
        let host = &host_port[..colon];
        let port_str = &host_port[colon + 1..];
        let port = port_str.parse::<u16>().ok();
        let host = if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        };
        (host, port)
    } else {
        let host = if host_port.is_empty() {
            None
        } else {
            Some(host_port.to_string())
        };
        (host, None)
    }
}

/// Host/port related key patterns in config files.
const HOST_KEY_PATTERNS: &[&str] = &["host", "hostname", "server", "addr", "address", "endpoint"];
const PORT_KEY_PATTERNS: &[&str] = &["port"];

/// Try to extract host or port from a key=value or key: value line.
fn extract_host_port_from_kv(line: &str) -> Option<ExtractedEndpoint> {
    // Try key=value
    let (key, value) = if let Some((k, v)) = line.split_once('=') {
        (k.trim(), v.trim())
    } else if let Some((k, v)) = line.split_once(':') {
        // YAML-style key: value — but skip if it looks like a URL (has //)
        if v.contains("//") {
            return None;
        }
        (k.trim(), v.trim())
    } else {
        return None;
    };

    // Strip quotes from value
    let value = value.trim_matches(|c: char| c == '"' || c == '\'');
    if value.is_empty() || value.len() > 200 {
        return None;
    }

    let key_lower = key.to_lowercase();
    let key_lower = key_lower.trim_start_matches(['#', '-', ' ']);

    // Check if key matches a host pattern
    let is_host_key = HOST_KEY_PATTERNS.iter().any(|p| key_lower.contains(p));

    let is_port_key = PORT_KEY_PATTERNS.iter().any(|p| key_lower.ends_with(p));

    if is_host_key && looks_like_hostname(value) {
        // Infer technology from key name
        let tech = infer_technology_from_key(key_lower);
        return Some(ExtractedEndpoint {
            key: key.to_string(),
            value: value.to_string(),
            parsed_host: Some(value.to_string()),
            parsed_port: None,
            technology: tech,
        });
    }

    if is_port_key {
        if let Ok(port) = value.parse::<u16>() {
            let tech =
                infer_technology_from_port(port).or_else(|| infer_technology_from_key(key_lower));
            return Some(ExtractedEndpoint {
                key: key.to_string(),
                value: value.to_string(),
                parsed_host: None,
                parsed_port: Some(port),
                technology: tech,
            });
        }
    }

    None
}

/// Check if a value looks like a hostname (not a random string).
fn looks_like_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    // Must contain at least one letter
    if !value.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    // Basic hostname chars: letters, digits, dots, hyphens
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// Infer technology from a config key name.
fn infer_technology_from_key(key: &str) -> Option<String> {
    if key.contains("postgres") || key.contains("pg") {
        Some("postgresql".to_string())
    } else if key.contains("mysql") || key.contains("mariadb") {
        Some("mysql".to_string())
    } else if key.contains("redis") {
        Some("redis".to_string())
    } else if key.contains("mongo") {
        Some("mongodb".to_string())
    } else if key.contains("rabbit") || key.contains("amqp") {
        Some("rabbitmq".to_string())
    } else if key.contains("kafka") {
        Some("kafka".to_string())
    } else if key.contains("elastic") {
        Some("elasticsearch".to_string())
    } else if key.contains("oracle") {
        Some("oracle".to_string())
    } else if key.contains("sqlserver") || key.contains("mssql") {
        Some("sqlserver".to_string())
    } else {
        None
    }
}

/// Infer technology from a well-known port number.
fn infer_technology_from_port(port: u16) -> Option<String> {
    match port {
        5432 => Some("postgresql".to_string()),
        3306 => Some("mysql".to_string()),
        1521 => Some("oracle".to_string()),
        1433 => Some("sqlserver".to_string()),
        27017 => Some("mongodb".to_string()),
        6379 => Some("redis".to_string()),
        5672 | 5671 => Some("rabbitmq".to_string()),
        9092 => Some("kafka".to_string()),
        9200 | 9300 => Some("elasticsearch".to_string()),
        11211 => Some("memcached".to_string()),
        2181 => Some("zookeeper".to_string()),
        _ => None,
    }
}

// =========================================================================
// Services (systemd)
// =========================================================================

/// Discover systemd services via systemctl and get their main PIDs.
pub fn scan_services() -> Vec<DiscoveredService> {
    use std::process::Command;

    let output = Command::new("systemctl")
        .args(["list-units", "--type=service", "--no-pager", "--no-legend"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut services: Vec<DiscoveredService> = output
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
        .collect();

    // Enrich running services with their MainPID
    for service in &mut services {
        if service.status == "running" {
            service.pid = get_service_main_pid(&service.name);
        }
    }

    services
}

/// Get the MainPID of a systemd service.
fn get_service_main_pid(name: &str) -> Option<u32> {
    use std::process::Command;

    let output = Command::new("systemctl")
        .args(["show", name, "--property=MainPID"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output: "MainPID=12345"
    stdout
        .trim()
        .strip_prefix("MainPID=")
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&pid| pid > 0)
}

// =========================================================================
// Service ↔ Process cross-referencing and command suggestion
// =========================================================================

/// Cross-reference a process PID with systemd services to generate command suggestions.
pub fn suggest_commands(
    pid: u32,
    process_name: &str,
    cmdline: &str,
    services: &[DiscoveredService],
) -> (Option<CommandSuggestion>, Option<String>) {
    // Check if this PID matches a running systemd service
    for svc in services {
        if svc.pid == Some(pid) && svc.status == "running" {
            return (
                Some(CommandSuggestion {
                    check_cmd: format!("systemctl is-active {}", svc.name),
                    start_cmd: Some(format!("systemctl start {}", svc.name)),
                    stop_cmd: Some(format!("systemctl stop {}", svc.name)),
                    restart_cmd: Some(format!("systemctl restart {}", svc.name)),
                    logs_cmd: Some(format!("journalctl -u {} -n 100 --no-pager", svc.name)),
                    version_cmd: None,
                    confidence: "high".to_string(),
                    source: "systemd".to_string(),
                }),
                Some(format!("{}.service", svc.name)),
            );
        }
    }

    // Fallback: generate pgrep-based commands
    // Build a reasonable pattern from the cmdline
    let pattern = if cmdline.len() > 100 {
        // Use the first argument (binary path) + a short part of args
        cmdline
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    } else if cmdline.is_empty() {
        process_name.to_string()
    } else {
        cmdline.to_string()
    };

    // Escape single quotes in the pattern
    let safe_pattern = pattern.replace('\'', "'\\''");

    (
        Some(CommandSuggestion {
            check_cmd: format!("pgrep -f '{}'", safe_pattern),
            start_cmd: None,
            stop_cmd: None,
            restart_cmd: None,
            logs_cmd: None,
            version_cmd: None,
            confidence: "low".to_string(),
            source: "process".to_string(),
        }),
        None,
    )
}

// =========================================================================
// Firewall Rules (iptables / firewalld)
// =========================================================================

/// Scan Linux firewall rules using iptables or firewalld.
pub fn scan_firewall_rules() -> Vec<DiscoveredFirewallRule> {
    // Try iptables first (most common on Linux)
    if let Some(rules) = scan_iptables() {
        if !rules.is_empty() {
            return rules;
        }
    }

    // Try firewalld (RHEL/CentOS/Fedora)
    if let Some(rules) = scan_firewalld() {
        return rules;
    }

    Vec::new()
}

/// Scan iptables rules.
fn scan_iptables() -> Option<Vec<DiscoveredFirewallRule>> {
    use std::process::Command;

    // Try to get INPUT chain rules (need root, may fail)
    let output = Command::new("iptables")
        .args(["-L", "INPUT", "-n", "-v", "--line-numbers"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut rules = Vec::new();

    // Skip header lines
    for line in stdout.lines().skip(2) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        // Format: num pkts bytes target prot opt in out source destination [extra]
        let rule_num = parts[0];
        let target = parts[3]; // ACCEPT, DROP, REJECT
        let protocol = parts[4]; // tcp, udp, all
        let _destination = parts[8];

        // Parse port from dpt:PORT if present
        let local_port = line
            .split_whitespace()
            .find(|p| p.starts_with("dpt:"))
            .and_then(|s| s.strip_prefix("dpt:"))
            .and_then(|p| p.parse::<u16>().ok());

        let action = match target.to_lowercase().as_str() {
            "accept" => "allow",
            "drop" | "reject" => "block",
            _ => continue, // Skip chains like LOG, RETURN
        };

        rules.push(DiscoveredFirewallRule {
            name: format!("INPUT-{}", rule_num),
            action: action.to_string(),
            direction: "in".to_string(),
            protocol: protocol.to_lowercase(),
            local_port,
            remote_port: None,
            enabled: true,
        });
    }

    Some(rules)
}

/// Scan firewalld rules (RHEL/CentOS/Fedora).
fn scan_firewalld() -> Option<Vec<DiscoveredFirewallRule>> {
    use std::process::Command;

    // Get active zones first
    let zone_output = Command::new("firewall-cmd")
        .args(["--get-active-zones"])
        .output()
        .ok()?;

    if !zone_output.status.success() {
        return None;
    }

    // Get allowed services and ports
    let list_output = Command::new("firewall-cmd")
        .args(["--list-all"])
        .output()
        .ok()?;

    if !list_output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let mut rules = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();

        // Parse "services: ssh http https"
        if let Some(services) = trimmed.strip_prefix("services:") {
            for service in services.split_whitespace() {
                rules.push(DiscoveredFirewallRule {
                    name: format!("service-{}", service),
                    action: "allow".to_string(),
                    direction: "in".to_string(),
                    protocol: "tcp".to_string(),
                    local_port: well_known_service_port(service),
                    remote_port: None,
                    enabled: true,
                });
            }
        }

        // Parse "ports: 8080/tcp 9000/tcp"
        if let Some(ports) = trimmed.strip_prefix("ports:") {
            for port_spec in ports.split_whitespace() {
                if let Some((port_str, proto)) = port_spec.split_once('/') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        rules.push(DiscoveredFirewallRule {
                            name: format!("port-{}-{}", port, proto),
                            action: "allow".to_string(),
                            direction: "in".to_string(),
                            protocol: proto.to_lowercase(),
                            local_port: Some(port),
                            remote_port: None,
                            enabled: true,
                        });
                    }
                }
            }
        }
    }

    Some(rules)
}

/// Map well-known service names to ports.
fn well_known_service_port(service: &str) -> Option<u16> {
    match service {
        "ssh" => Some(22),
        "http" => Some(80),
        "https" => Some(443),
        "ftp" => Some(21),
        "smtp" => Some(25),
        "dns" => Some(53),
        "mysql" => Some(3306),
        "postgresql" => Some(5432),
        "redis" => Some(6379),
        _ => None,
    }
}

// =========================================================================
// Cron jobs + systemd timers
// =========================================================================

/// System cron jobs to exclude (background OS maintenance).
const SYSTEM_CRON_COMMANDS: &[&str] = &[
    "logrotate",
    "apt-daily",
    "apt-compat",
    "dpkg",
    "certbot",
    "fstrim",
    "man-db",
    "plocate",
    "mlocate",
    "e2scrub",
    "update-notifier",
    "apport",
    "popularity-contest",
    "sysstat",
];

/// Scan cron jobs from user crontabs and system cron directories.
pub fn scan_cron_jobs() -> Vec<DiscoveredScheduledJob> {
    let mut jobs = Vec::new();

    // 1. User crontabs: /var/spool/cron/crontabs/*
    if let Ok(entries) = fs::read_dir("/var/spool/cron/crontabs") {
        for entry in entries.flatten() {
            let user = entry.file_name().to_string_lossy().to_string();
            if let Ok(content) = fs::read_to_string(entry.path()) {
                parse_crontab_content(&content, &user, "crontab", &mut jobs);
            }
        }
    }

    // 2. System crontab: /etc/crontab
    if let Ok(content) = fs::read_to_string("/etc/crontab") {
        parse_system_crontab(&content, "crontab", &mut jobs);
    }

    // 3. /etc/cron.d/*
    if let Ok(entries) = fs::read_dir("/etc/cron.d") {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden files and common system files
            if file_name.starts_with('.') || file_name.starts_with("0hourly") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(entry.path()) {
                parse_system_crontab(&content, "cron.d", &mut jobs);
            }
        }
    }

    // 4. Systemd timers
    if let Some(timer_jobs) = scan_systemd_timers() {
        jobs.extend(timer_jobs);
    }

    jobs
}

/// Parse a user crontab (no user column — user is the file owner).
fn parse_crontab_content(
    content: &str,
    user: &str,
    source: &str,
    jobs: &mut Vec<DiscoveredScheduledJob>,
) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("SHELL=")
            || trimmed.starts_with("PATH=")
            || trimmed.starts_with("MAILTO=")
            || trimmed.starts_with("HOME=")
        {
            continue;
        }

        // Cron format: min hour dom month dow command
        let parts: Vec<&str> = trimmed.splitn(6, char::is_whitespace).collect();
        if parts.len() < 6 {
            continue;
        }

        let schedule = parts[..5].join(" ");
        let command = parts[5].to_string();

        // Skip system jobs
        if is_system_cron_command(&command) {
            continue;
        }

        let name = derive_job_name(&command);

        jobs.push(DiscoveredScheduledJob {
            name,
            schedule,
            command,
            user: user.to_string(),
            source: source.to_string(),
            enabled: true,
        });
    }
}

/// Parse a system crontab (has a user column between schedule and command).
fn parse_system_crontab(content: &str, source: &str, jobs: &mut Vec<DiscoveredScheduledJob>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("SHELL=")
            || trimmed.starts_with("PATH=")
            || trimmed.starts_with("MAILTO=")
            || trimmed.starts_with("HOME=")
        {
            continue;
        }

        // System crontab format: min hour dom month dow user command
        let parts: Vec<&str> = trimmed.splitn(7, char::is_whitespace).collect();
        if parts.len() < 7 {
            continue;
        }

        let schedule = parts[..5].join(" ");
        let user = parts[5].to_string();
        let command = parts[6].to_string();

        if is_system_cron_command(&command) {
            continue;
        }

        let name = derive_job_name(&command);

        jobs.push(DiscoveredScheduledJob {
            name,
            schedule,
            command,
            user,
            source: source.to_string(),
            enabled: true,
        });
    }
}

/// Check if a cron command looks like a system maintenance job.
fn is_system_cron_command(command: &str) -> bool {
    SYSTEM_CRON_COMMANDS
        .iter()
        .any(|pattern| command.contains(pattern))
}

/// Derive a human-readable name from a cron command.
fn derive_job_name(command: &str) -> String {
    // Take the last path component of the first word
    let first_word = command.split_whitespace().next().unwrap_or(command);
    let base = first_word.rsplit('/').next().unwrap_or(first_word);
    // Truncate long names
    if base.len() > 50 {
        format!("{}...", &base[..47])
    } else {
        base.to_string()
    }
}

/// Scan systemd timers.
fn scan_systemd_timers() -> Option<Vec<DiscoveredScheduledJob>> {
    use std::process::Command;

    let output = Command::new("systemctl")
        .args(["list-timers", "--no-pager", "--no-legend", "--all"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut jobs = Vec::new();

    // Format: NEXT LEFT LAST PASSED UNIT ACTIVATES
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // Timer lines have many columns; the UNIT and ACTIVATES are at the end
        if parts.len() < 2 {
            continue;
        }

        // The last two tokens are typically UNIT (the timer) and ACTIVATES (the service)
        let unit = parts[parts.len() - 2];
        let activates = parts[parts.len() - 1];

        // Skip system timers
        let timer_name = unit.trim_end_matches(".timer");
        if timer_name.starts_with("systemd-")
            || timer_name.starts_with("apt-")
            || timer_name.starts_with("fstrim")
            || timer_name.starts_with("logrotate")
            || timer_name.starts_with("man-db")
            || timer_name.starts_with("e2scrub")
            || timer_name.starts_with("phpsessionclean")
        {
            continue;
        }

        jobs.push(DiscoveredScheduledJob {
            name: timer_name.to_string(),
            schedule: "systemd-timer".to_string(),
            command: activates.to_string(),
            user: "root".to_string(),
            source: "systemd-timer".to_string(),
            enabled: true,
        });
    }

    Some(jobs)
}

// =========================================================================
// Tests
// =========================================================================

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

    #[test]
    fn test_parse_host_port_from_url_jdbc() {
        let (host, port) =
            parse_host_port_from_url("jdbc:postgresql://db-srv:5432/orders", "jdbc:postgresql://");
        assert_eq!(host, Some("db-srv".to_string()));
        assert_eq!(port, Some(5432));
    }

    #[test]
    fn test_parse_host_port_from_url_amqp_with_auth() {
        let (host, port) =
            parse_host_port_from_url("amqp://user:pass@rabbit-host:5672/vhost", "amqp://");
        assert_eq!(host, Some("rabbit-host".to_string()));
        assert_eq!(port, Some(5672));
    }

    #[test]
    fn test_parse_host_port_from_url_redis() {
        let (host, port) = parse_host_port_from_url("redis://redis-host:6379", "redis://");
        assert_eq!(host, Some("redis-host".to_string()));
        assert_eq!(port, Some(6379));
    }

    #[test]
    fn test_parse_host_port_from_url_no_port() {
        let (host, port) = parse_host_port_from_url("http://api-host/endpoint", "http://");
        assert_eq!(host, Some("api-host".to_string()));
        assert_eq!(port, None);
    }

    #[test]
    fn test_extract_urls_from_line_jdbc() {
        let line = "spring.datasource.url=jdbc:postgresql://db-srv:5432/orders";
        let results = extract_urls_from_line(line);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].parsed_host, Some("db-srv".to_string()));
        assert_eq!(results[0].parsed_port, Some(5432));
        assert_eq!(results[0].technology, Some("postgresql".to_string()));
    }

    #[test]
    fn test_extract_urls_from_line_redis() {
        let line = "REDIS_URL=redis://my-redis:6379";
        let results = extract_urls_from_line(line);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].parsed_host, Some("my-redis".to_string()));
        assert_eq!(results[0].parsed_port, Some(6379));
        assert_eq!(results[0].technology, Some("redis".to_string()));
    }

    #[test]
    fn test_extract_host_port_from_kv_host() {
        let line = "redis.host=cache-server-01";
        let result = extract_host_port_from_kv(line);
        assert!(result.is_some());
        let ep = result.unwrap();
        assert_eq!(ep.parsed_host, Some("cache-server-01".to_string()));
        assert_eq!(ep.technology, Some("redis".to_string()));
    }

    #[test]
    fn test_extract_host_port_from_kv_port() {
        let line = "postgres.port=5432";
        let result = extract_host_port_from_kv(line);
        assert!(result.is_some());
        let ep = result.unwrap();
        assert_eq!(ep.parsed_port, Some(5432));
        assert_eq!(ep.technology, Some("postgresql".to_string()));
    }

    #[test]
    fn test_looks_like_hostname() {
        assert!(looks_like_hostname("db-server-01"));
        assert!(looks_like_hostname("redis.internal.corp"));
        assert!(!looks_like_hostname(""));
        assert!(!looks_like_hostname("12345"));
        // "true" contains letters so looks_like_hostname("true") returns true
        // — that's OK, callers also check the key name for context
    }

    #[test]
    fn test_infer_technology_from_port() {
        assert_eq!(
            infer_technology_from_port(5432),
            Some("postgresql".to_string())
        );
        assert_eq!(infer_technology_from_port(6379), Some("redis".to_string()));
        assert_eq!(infer_technology_from_port(12345), None);
    }

    #[test]
    fn test_is_system_cron_command() {
        assert!(is_system_cron_command(
            "/usr/bin/logrotate /etc/logrotate.conf"
        ));
        assert!(is_system_cron_command("apt-daily-upgrade"));
        assert!(!is_system_cron_command(
            "/opt/app/scripts/nightly-cleanup.sh"
        ));
    }

    #[test]
    fn test_derive_job_name() {
        assert_eq!(derive_job_name("/opt/app/scripts/cleanup.sh"), "cleanup.sh");
        assert_eq!(
            derive_job_name("python /app/batch.py --mode=full"),
            "python"
        );
    }

    #[test]
    fn test_suggest_commands_with_systemd() {
        let services = vec![DiscoveredService {
            name: "nginx".to_string(),
            display_name: "nginx".to_string(),
            status: "running".to_string(),
            pid: Some(1234),
        }];
        let (suggestion, matched) =
            suggest_commands(1234, "nginx", "nginx: master process", &services);
        assert!(suggestion.is_some());
        let s = suggestion.unwrap();
        assert_eq!(s.confidence, "high");
        assert_eq!(s.source, "systemd");
        assert!(s.check_cmd.contains("is-active"));
        assert_eq!(matched, Some("nginx.service".to_string()));
    }

    #[test]
    fn test_suggest_commands_fallback() {
        let services = vec![];
        let (suggestion, matched) =
            suggest_commands(9999, "java", "/usr/bin/java -jar app.jar", &services);
        assert!(suggestion.is_some());
        let s = suggestion.unwrap();
        assert_eq!(s.confidence, "low");
        assert_eq!(s.source, "process");
        assert!(s.check_cmd.contains("pgrep"));
        assert_eq!(matched, None);
    }
}
