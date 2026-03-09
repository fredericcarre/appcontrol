//! macOS-specific discovery implementation.
//!
//! Uses `lsof` for network scanning and `launchctl` for service/job discovery.

use std::collections::HashMap;
use std::process::Command;

use appcontrol_common::{
    CommandSuggestion, DiscoveredConfigFile, DiscoveredConnection, DiscoveredListener,
    DiscoveredLogFile, DiscoveredScheduledJob, DiscoveredService,
};
use sysinfo::System;

/// Scan TCP listeners and established connections using `lsof`.
pub fn scan_network(_sys: &System) -> (Vec<DiscoveredListener>, Vec<DiscoveredConnection>) {
    let listeners = scan_listeners();
    let connections = scan_connections();
    (listeners, connections)
}

/// Scan TCP listeners via `lsof -iTCP -sTCP:LISTEN -n -P`.
fn scan_listeners() -> Vec<DiscoveredListener> {
    let output = match Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P", "-F", "pcn"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("Failed to run lsof for listeners: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        return Vec::new();
    }

    parse_lsof_output(&String::from_utf8_lossy(&output.stdout), true)
        .into_iter()
        .map(|(port, pid, process_name, addr)| DiscoveredListener {
            port,
            protocol: "tcp".to_string(),
            pid: Some(pid),
            process_name: Some(process_name),
            address: addr,
        })
        .collect()
}

/// Scan established connections via `lsof -iTCP -sTCP:ESTABLISHED -n -P`.
fn scan_connections() -> Vec<DiscoveredConnection> {
    let output = match Command::new("lsof")
        .args(["-iTCP", "-sTCP:ESTABLISHED", "-n", "-P", "-F", "pcn"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("Failed to run lsof for connections: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        return Vec::new();
    }

    parse_lsof_connections(&String::from_utf8_lossy(&output.stdout))
}

/// Parse lsof -F output format.
/// Format: p<pid>\nc<command>\nn<name> (network info)
fn parse_lsof_output(output: &str, is_listener: bool) -> Vec<(u16, u32, String, String)> {
    let mut results = Vec::new();
    let mut current_pid: Option<u32> = None;
    let mut current_cmd: Option<String> = None;

    for line in output.lines() {
        if let Some(pid_str) = line.strip_prefix('p') {
            current_pid = pid_str.parse().ok();
        } else if let Some(cmd_str) = line.strip_prefix('c') {
            current_cmd = Some(cmd_str.to_string());
        } else if let Some(name) = line.strip_prefix('n') {
            if let (Some(pid), Some(ref cmd)) = (current_pid, &current_cmd) {
                // n format: "host:port" or "*:port" for listeners, "local->remote" for connections
                if is_listener {
                    // Listener format: "*:8080" or "127.0.0.1:5432"
                    if let Some(port_str) = name.rsplit(':').next() {
                        if let Ok(port) = port_str.parse::<u16>() {
                            let addr = name
                                .rsplit(':')
                                .nth(1)
                                .unwrap_or("*")
                                .replace('*', "0.0.0.0");
                            results.push((port, pid, cmd.clone(), addr));
                        }
                    }
                }
            }
        }
    }

    // Deduplicate by port
    results.sort_by_key(|(port, _, _, _)| *port);
    results.dedup_by_key(|(port, _, _, _)| *port);
    results
}

/// Parse lsof output for established connections.
fn parse_lsof_connections(output: &str) -> Vec<DiscoveredConnection> {
    let mut results = Vec::new();
    let mut current_pid: Option<u32> = None;
    let mut current_cmd: Option<String> = None;

    for line in output.lines() {
        if let Some(pid_str) = line.strip_prefix('p') {
            current_pid = pid_str.parse().ok();
        } else if let Some(cmd_str) = line.strip_prefix('c') {
            current_cmd = Some(cmd_str.to_string());
        } else if let Some(name) = line.strip_prefix('n') {
            if let (Some(pid), Some(ref cmd)) = (current_pid, &current_cmd) {
                // Connection format: "local_addr:local_port->remote_addr:remote_port"
                if let Some((local, remote)) = name.split_once("->") {
                    let local_port = local
                        .rsplit(':')
                        .next()
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(0);
                    let (remote_addr, remote_port) = if let Some(idx) = remote.rfind(':') {
                        let addr = &remote[..idx];
                        let port = remote[idx + 1..].parse::<u16>().unwrap_or(0);
                        (addr.to_string(), port)
                    } else {
                        (remote.to_string(), 0)
                    };

                    // Filter out localhost connections and low ports (likely ephemeral)
                    if !remote_addr.starts_with("127.")
                        && !remote_addr.starts_with("::1")
                        && remote_port > 0
                    {
                        results.push(DiscoveredConnection {
                            local_port,
                            remote_addr,
                            remote_port,
                            pid: Some(pid),
                            process_name: Some(cmd.clone()),
                            state: "ESTABLISHED".to_string(),
                        });
                    }
                }
            }
        }
    }

    results
}

/// Scan launchd services via `launchctl list`.
pub fn scan_services() -> Vec<DiscoveredService> {
    let output = match Command::new("launchctl").args(["list"]).output() {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("Failed to run launchctl list: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();

    // Skip header line
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let pid = if parts[0] == "-" {
                None
            } else {
                parts[0].parse::<u32>().ok()
            };
            let status = if parts[1] == "0" {
                "running".to_string()
            } else if parts[1] == "-" {
                "stopped".to_string()
            } else {
                format!("exit({})", parts[1])
            };
            let name = parts[2].to_string();

            // Filter out Apple system services for cleaner output
            if !name.starts_with("com.apple.")
                && !name.starts_with('[')
                && !name.contains("UIKitApplication")
            {
                services.push(DiscoveredService {
                    name: name.clone(),
                    display_name: name,
                    status,
                    pid,
                });
            }
        }
    }

    services
}

/// Scan cron jobs and launchd agents for scheduled tasks.
pub fn scan_scheduled_jobs() -> Vec<DiscoveredScheduledJob> {
    let mut jobs = Vec::new();

    // Scan user crontab
    if let Ok(output) = Command::new("crontab").args(["-l"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    if let Some(job) = parse_cron_line(line) {
                        jobs.push(job);
                    }
                }
            }
        }
    }

    // Scan LaunchAgents/LaunchDaemons for scheduled jobs
    let plist_dirs = [
        "/Library/LaunchDaemons",
        "/Library/LaunchAgents",
    ];

    // Expand home directory for user LaunchAgents
    if let Ok(home) = std::env::var("HOME") {
        let user_agents = format!("{}/Library/LaunchAgents", home);
        if let Ok(entries) = std::fs::read_dir(&user_agents) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "plist") {
                    if let Some(job) = parse_launchd_plist(&path) {
                        jobs.push(job);
                    }
                }
            }
        }
    }

    for dir in &plist_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "plist") {
                    if let Some(job) = parse_launchd_plist(&path) {
                        jobs.push(job);
                    }
                }
            }
        }
    }

    jobs
}

/// Parse a cron line into a DiscoveredScheduledJob.
fn parse_cron_line(line: &str) -> Option<DiscoveredScheduledJob> {
    // Standard cron format: min hour day month dow command
    let parts: Vec<&str> = line.splitn(6, char::is_whitespace).collect();
    if parts.len() >= 6 {
        let schedule = format!(
            "{} {} {} {} {}",
            parts[0], parts[1], parts[2], parts[3], parts[4]
        );
        let command = parts[5].to_string();

        // Skip common system jobs
        if command.contains("logrotate") || command.contains("periodic") {
            return None;
        }

        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

        Some(DiscoveredScheduledJob {
            name: command
                .split_whitespace()
                .next()
                .unwrap_or("cron")
                .to_string(),
            schedule,
            command,
            user,
            source: "crontab".to_string(),
            enabled: true,
        })
    } else {
        None
    }
}

/// Parse a launchd plist for scheduled job info.
fn parse_launchd_plist(path: &std::path::Path) -> Option<DiscoveredScheduledJob> {
    // Simple plist parsing - look for StartCalendarInterval or StartInterval
    let content = std::fs::read_to_string(path).ok()?;

    let has_schedule = content.contains("StartCalendarInterval")
        || content.contains("StartInterval")
        || content.contains("WatchPaths");

    if !has_schedule {
        return None;
    }

    let name = path.file_stem()?.to_string_lossy().to_string();

    // Extract program/command if possible
    let command = if let Some(start) = content.find("<key>Program</key>") {
        content[start..]
            .lines()
            .nth(1)
            .and_then(|l| {
                l.trim()
                    .strip_prefix("<string>")
                    .and_then(|s| s.strip_suffix("</string>"))
            })
            .unwrap_or(&name)
            .to_string()
    } else {
        name.clone()
    };

    Some(DiscoveredScheduledJob {
        name,
        schedule: "launchd".to_string(),
        command,
        user: "system".to_string(),
        source: path.to_string_lossy().to_string(),
        enabled: true,
    })
}

/// Read process environment variables from macOS.
/// Note: Reading another process's environment on macOS requires root or SIP disabled.
pub fn read_process_env(_pid: u32) -> HashMap<String, String> {
    // On macOS, reading another process's environment is restricted.
    // Would need `ps eww <pid>` with root, or libproc.
    HashMap::new()
}

/// Read process working directory.
pub fn read_working_dir(pid: u32) -> Option<String> {
    // Use lsof to get cwd
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-F", "n"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix('n') {
            return Some(rest.to_string());
        }
    }
    None
}

/// Scan open files for config and log files.
pub fn scan_open_files(pid: u32) -> (Vec<DiscoveredConfigFile>, Vec<DiscoveredLogFile>) {
    let output = match Command::new("lsof")
        .args(["-p", &pid.to_string(), "-F", "n"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return (Vec::new(), Vec::new()),
    };

    if !output.status.success() {
        return (Vec::new(), Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut configs = Vec::new();
    let mut logs = Vec::new();

    for line in stdout.lines() {
        if !line.starts_with('n') {
            continue;
        }
        let path = &line[1..];

        // Skip non-file paths
        if !path.starts_with('/') || path.contains("/dev/") || path.contains("/proc/") {
            continue;
        }

        // Identify config files
        let config_extensions = [
            ".yaml",
            ".yml",
            ".json",
            ".xml",
            ".conf",
            ".cfg",
            ".ini",
            ".properties",
            ".toml",
            ".env",
        ];
        let config_paths = ["/etc/", "/conf/", "/config/", "/.config/"];

        let is_config = config_extensions.iter().any(|ext| path.ends_with(ext))
            || config_paths.iter().any(|p| path.contains(p));

        if is_config {
            configs.push(DiscoveredConfigFile {
                path: path.to_string(),
                extracted_endpoints: Vec::new(), // Would need to read and parse
            });
        }

        // Identify log files
        let log_extensions = [".log", ".out", ".err"];
        let log_paths = ["/log/", "/logs/", "/var/log/"];

        let is_log = log_extensions.iter().any(|ext| path.ends_with(ext))
            || log_paths.iter().any(|p| path.contains(p));

        if is_log {
            // Get file size
            let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            logs.push(DiscoveredLogFile {
                path: path.to_string(),
                size_bytes,
            });
        }
    }

    (configs, logs)
}

/// Generate command suggestions for a process on macOS.
pub fn suggest_commands(
    _pid: u32,
    name: &str,
    cmdline: &str,
    services: &[DiscoveredService],
) -> (Option<CommandSuggestion>, Option<String>) {
    // Check if process matches a launchd service
    for svc in services {
        if svc.name.contains(name)
            || name.contains(&svc.name.replace("com.", "").replace('.', ""))
        {
            return (
                Some(CommandSuggestion {
                    check_cmd: format!("launchctl list {} | grep -q PID", svc.name),
                    start_cmd: Some(format!("launchctl start {}", svc.name)),
                    stop_cmd: Some(format!("launchctl stop {}", svc.name)),
                    restart_cmd: Some(format!("launchctl kickstart -k system/{}", svc.name)),
                    logs_cmd: Some(format!("log show --predicate 'subsystem == \"{}\"' --last 1h", svc.name)),
                    version_cmd: None,
                    confidence: "high".to_string(),
                    source: "launchd".to_string(),
                }),
                Some(svc.name.clone()),
            );
        }
    }

    // Well-known services with Homebrew patterns
    let known_services: &[(&str, &str, &str)] = &[
        ("postgres", "postgresql", "pg_isready"),
        ("mysql", "mysql", "mysqladmin ping"),
        ("redis-server", "redis", "redis-cli ping"),
        (
            "mongod",
            "mongodb-community",
            "mongosh --eval 'db.runCommand({ping:1})'",
        ),
        ("nginx", "nginx", "curl -sf http://localhost/ > /dev/null"),
        ("httpd", "httpd", "curl -sf http://localhost/ > /dev/null"),
    ];

    for (proc_name, brew_name, check) in known_services {
        if name.contains(proc_name) || cmdline.contains(proc_name) {
            return (
                Some(CommandSuggestion {
                    check_cmd: check.to_string(),
                    start_cmd: Some(format!("brew services start {}", brew_name)),
                    stop_cmd: Some(format!("brew services stop {}", brew_name)),
                    restart_cmd: Some(format!("brew services restart {}", brew_name)),
                    logs_cmd: Some(format!("cat $(brew --prefix)/var/log/{}*.log 2>/dev/null | tail -100", brew_name)),
                    version_cmd: Some(format!("{} --version 2>/dev/null || brew info {}", proc_name, brew_name)),
                    confidence: "medium".to_string(),
                    source: "homebrew".to_string(),
                }),
                Some(brew_name.to_string()),
            );
        }
    }

    // Fallback: pgrep-based for unknown services
    if !cmdline.is_empty() {
        let pattern = if cmdline.len() > 50 {
            &cmdline[..50]
        } else {
            cmdline
        };
        let escaped = pattern.replace('\'', "'\\''");

        return (
            Some(CommandSuggestion {
                check_cmd: format!("pgrep -f '{}' > /dev/null", escaped),
                start_cmd: None, // Unknown - user must fill in
                stop_cmd: Some(format!("pkill -f '{}'", escaped)),
                restart_cmd: None,
                logs_cmd: None,
                version_cmd: None,
                confidence: "low".to_string(),
                source: "pgrep".to_string(),
            }),
            None,
        );
    }

    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lsof_listener_output() {
        let output = r#"p1234
cpostgres
n*:5432
p5678
cnginx
n127.0.0.1:80
"#;
        let result = parse_lsof_output(output, true);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|(port, _, _, _)| *port == 5432));
        assert!(result.iter().any(|(port, _, _, _)| *port == 80));
    }

    #[test]
    fn test_parse_lsof_connection_output() {
        let output = r#"p1234
cchrome
n192.168.1.100:54321->142.250.185.206:443
p5678
cnode
n10.0.0.5:45678->10.0.0.10:5432
"#;
        let result = parse_lsof_connections(output);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|c| c.remote_port == 443));
        assert!(result.iter().any(|c| c.remote_port == 5432));
    }
}
