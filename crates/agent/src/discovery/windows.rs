//! Windows-specific discovery: netstat -ano parsing for TCP,
//! sc query for services, schtasks for scheduled tasks,
//! and service↔process cross-referencing for command suggestions.
//!
//! ## Why netstat instead of Win32 API?
//!
//! The `GetExtendedTcpTable` API requires elevated privileges and unsafe FFI.
//! `netstat -ano` is available on all Windows versions since XP, runs without
//! admin rights, and provides PID→port mapping out of the box. It's the most
//! reliable cross-version approach.

use std::collections::HashMap;
use sysinfo::System;

use appcontrol_common::{
    CommandSuggestion, DiscoveredConnection, DiscoveredListener, DiscoveredScheduledJob,
    DiscoveredService,
};

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

// =========================================================================
// Service ↔ Process cross-referencing and command suggestion
// =========================================================================

/// Cross-reference a process PID with Windows services to generate command suggestions.
pub fn suggest_commands(
    pid: u32,
    process_name: &str,
    services: &[DiscoveredService],
) -> (Option<CommandSuggestion>, Option<String>) {
    // Check if this PID matches a running Windows service
    for svc in services {
        if svc.pid == Some(pid) && svc.status == "running" {
            return (
                Some(CommandSuggestion {
                    check_cmd: format!("sc query {} | findstr RUNNING", svc.name),
                    start_cmd: Some(format!("net start {}", svc.name)),
                    stop_cmd: Some(format!("net stop {}", svc.name)),
                    restart_cmd: Some(format!("net stop {} && net start {}", svc.name, svc.name)),
                    confidence: "high".to_string(),
                    source: "windows-service".to_string(),
                }),
                Some(svc.name.clone()),
            );
        }
    }

    // Fallback: tasklist-based check command
    let exe_name = if process_name.ends_with(".exe") {
        process_name.to_string()
    } else {
        format!("{}.exe", process_name)
    };

    (
        Some(CommandSuggestion {
            check_cmd: format!(
                "tasklist /FI \"IMAGENAME eq {}\" | findstr /I {}",
                exe_name, process_name
            ),
            start_cmd: None,
            stop_cmd: None,
            restart_cmd: None,
            confidence: "low".to_string(),
            source: "process".to_string(),
        }),
        None,
    )
}

// =========================================================================
// Scheduled Tasks (schtasks)
// =========================================================================

/// Scan Windows Task Scheduler for user-defined scheduled tasks.
pub fn scan_scheduled_tasks() -> Vec<DiscoveredScheduledJob> {
    use std::process::Command;

    let output = Command::new("schtasks")
        .args(["/query", "/fo", "CSV", "/v"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut jobs = Vec::new();
    let mut lines = output.lines();

    // Find column indices from header
    let header = match lines.next() {
        Some(h) => h,
        None => return jobs,
    };

    let headers: Vec<&str> = parse_csv_line(header);
    let idx_name = headers.iter().position(|h| h.contains("TaskName"));
    let idx_schedule = headers
        .iter()
        .position(|h| h.contains("Schedule Type") || h.contains("Scheduled Type"));
    let idx_command = headers.iter().position(|h| h.contains("Task To Run"));
    let idx_user = headers.iter().position(|h| h.contains("Run As User"));
    let idx_status = headers.iter().position(|h| h.contains("Status"));

    for line in lines {
        let fields = parse_csv_line(line);
        if fields.is_empty() {
            continue;
        }

        let name = idx_name.and_then(|i| fields.get(i)).unwrap_or(&"");
        let schedule = idx_schedule.and_then(|i| fields.get(i)).unwrap_or(&"");
        let command = idx_command.and_then(|i| fields.get(i)).unwrap_or(&"");
        let user = idx_user.and_then(|i| fields.get(i)).unwrap_or(&"");
        let status = idx_status.and_then(|i| fields.get(i)).unwrap_or(&"");

        // Skip Microsoft system tasks
        if name.starts_with("\\Microsoft\\") || name.starts_with("\\Windows\\") {
            continue;
        }

        // Skip empty/disabled tasks
        if command.is_empty() || *command == "N/A" {
            continue;
        }

        let enabled = !status.contains("Disabled");

        jobs.push(DiscoveredScheduledJob {
            name: name.to_string(),
            schedule: schedule.to_string(),
            command: command.to_string(),
            user: user.to_string(),
            source: "task-scheduler".to_string(),
            enabled,
        });
    }

    jobs
}

/// Simple CSV line parser (handles quoted fields).
fn parse_csv_line(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut in_quotes = false;
    let mut start = 0;

    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
        } else if bytes[i] == b',' && !in_quotes {
            let field = &line[start..i];
            fields.push(field.trim().trim_matches('"'));
            start = i + 1;
        }
    }
    // Last field
    if start <= line.len() {
        let field = &line[start..];
        fields.push(field.trim().trim_matches('"'));
    }

    fields
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

    #[test]
    fn test_parse_csv_line() {
        let line = r#""TaskName","Schedule","Command""#;
        let fields = parse_csv_line(line);
        assert_eq!(fields, vec!["TaskName", "Schedule", "Command"]);
    }

    #[test]
    fn test_parse_csv_line_with_commas_in_quotes() {
        let line = r#""Task, With Comma","Daily","C:\app\run.exe""#;
        let fields = parse_csv_line(line);
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], "Task, With Comma");
    }

    #[test]
    fn test_suggest_commands_with_service() {
        let services = vec![DiscoveredService {
            name: "MyService".to_string(),
            display_name: "My Service".to_string(),
            status: "running".to_string(),
            pid: Some(5678),
        }];
        let (suggestion, matched) = suggest_commands(5678, "myservice.exe", &services);
        assert!(suggestion.is_some());
        let s = suggestion.unwrap();
        assert_eq!(s.confidence, "high");
        assert_eq!(s.source, "windows-service");
        assert!(s.check_cmd.contains("findstr RUNNING"));
        assert_eq!(matched, Some("MyService".to_string()));
    }

    #[test]
    fn test_suggest_commands_fallback() {
        let services = vec![];
        let (suggestion, matched) = suggest_commands(9999, "java", &services);
        assert!(suggestion.is_some());
        let s = suggestion.unwrap();
        assert_eq!(s.confidence, "low");
        assert_eq!(s.source, "process");
        assert!(s.check_cmd.contains("tasklist"));
        assert_eq!(matched, None);
    }
}
