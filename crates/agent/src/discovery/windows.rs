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
    CommandSuggestion, DiscoveredConnection, DiscoveredFirewallRule, DiscoveredListener,
    DiscoveredScheduledJob, DiscoveredService,
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
    let name_lower = process_name.to_lowercase();

    // Check if this PID matches a running Windows service
    for svc in services {
        if svc.pid == Some(pid) && svc.status == "running" {
            return (
                Some(CommandSuggestion {
                    check_cmd: format!("sc query {} | findstr RUNNING", svc.name),
                    start_cmd: Some(format!("net start {}", svc.name)),
                    stop_cmd: Some(format!("net stop {}", svc.name)),
                    restart_cmd: Some(format!("net stop {} && net start {}", svc.name, svc.name)),
                    logs_cmd: None, // Windows services log to Event Viewer
                    version_cmd: None,
                    confidence: "high".to_string(),
                    source: "windows-service".to_string(),
                }),
                Some(svc.name.clone()),
            );
        }
    }

    // Well-known applications with specialized commands
    let exe_name = if process_name.ends_with(".exe") {
        process_name.to_string()
    } else {
        format!("{}.exe", process_name)
    };

    // MySQL - often installed as service but may run standalone
    if name_lower.contains("mysqld") {
        return (
            Some(CommandSuggestion {
                check_cmd: format!(
                    "wmic Path win32_process Where \"Caption Like '{}%'\" get caption | findstr /I mysqld",
                    process_name
                ),
                start_cmd: Some("sc start MySQL".to_string()),
                stop_cmd: Some("sc stop MySQL".to_string()),
                restart_cmd: Some("sc stop MySQL && sc start MySQL".to_string()),
                logs_cmd: Some("type \"C:\\ProgramData\\MySQL\\MySQL Server*\\Data\\*.err\" | more".to_string()),
                version_cmd: Some("mysql --version".to_string()),
                confidence: "medium".to_string(),
                source: "mysql".to_string(),
            }),
            Some("MySQL".to_string()),
        );
    }

    // RabbitMQ (Erlang runtime)
    if name_lower == "erl" || name_lower == "erl.exe" {
        return (
            Some(CommandSuggestion {
                check_cmd: "wmic Path win32_process Where \"Caption = 'erl.exe'\" get caption | findstr erl.exe".to_string(),
                start_cmd: Some("rabbitmq-service start".to_string()),
                stop_cmd: Some("rabbitmq-service stop".to_string()),
                restart_cmd: Some("rabbitmq-service stop && rabbitmq-service start".to_string()),
                logs_cmd: Some("type \"%APPDATA%\\RabbitMQ\\log\\*.log\" | more".to_string()),
                version_cmd: Some("rabbitmqctl version".to_string()),
                confidence: "medium".to_string(),
                source: "rabbitmq".to_string(),
            }),
            Some("RabbitMQ".to_string()),
        );
    }

    // ElasticSearch (Java process)
    if name_lower == "java" || name_lower == "java.exe" || name_lower.contains("javaw") {
        // Note: The actual identification happens in correlate based on port 9200
        return (
            Some(CommandSuggestion {
                check_cmd: format!(
                    "wmic Path win32_process Where \"Caption = '{}'\" get CommandLine | findstr /I java",
                    exe_name
                ),
                start_cmd: None, // Need cmdline context
                stop_cmd: Some(format!(
                    "wmic Path win32_process Where \"Caption = '{}'\" Call Terminate",
                    exe_name
                )),
                restart_cmd: None,
                logs_cmd: None, // Depends on application
                version_cmd: Some("java -version".to_string()),
                confidence: "low".to_string(),
                source: "java".to_string(),
            }),
            None,
        );
    }

    // Nginx
    if name_lower.contains("nginx") {
        return (
            Some(CommandSuggestion {
                check_cmd: "wmic process where \"ExecutablePath like '%nginx.exe'\" get ProcessID | findstr /R \"[0-9]\"".to_string(),
                start_cmd: Some("nginx".to_string()),
                stop_cmd: Some("wmic process where \"ExecutablePath like '%nginx.exe'\" call terminate".to_string()),
                restart_cmd: Some("nginx -s reload".to_string()),
                logs_cmd: Some("type nginx\\logs\\error.log | more".to_string()),
                version_cmd: Some("nginx -v".to_string()),
                confidence: "high".to_string(),
                source: "nginx".to_string(),
            }),
            Some("nginx".to_string()),
        );
    }

    // Generic .NET / custom EXE fallback with wmic-based stop
    (
        Some(CommandSuggestion {
            check_cmd: format!(
                "wmic Path win32_process Where \"Caption = '{}'\" get caption | findstr /I {}",
                exe_name,
                process_name.split('.').next().unwrap_or(process_name)
            ),
            start_cmd: None, // Unknown - user must fill in
            stop_cmd: Some(format!(
                "wmic Path win32_process Where \"Caption = '{}'\" Call Terminate",
                exe_name
            )),
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
// Firewall Rules (netsh advfirewall)
// =========================================================================

/// Scan Windows Firewall rules using netsh advfirewall.
pub fn scan_firewall_rules() -> Vec<DiscoveredFirewallRule> {
    use std::process::Command;

    let output = Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", "name=all"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut rules = Vec::new();
    let mut current_name = String::new();
    let mut current_action = String::new();
    let mut current_direction = String::new();
    let mut current_protocol = String::new();
    let mut current_local_port: Option<u16> = None;
    let mut current_remote_port: Option<u16> = None;
    let mut current_enabled = true;

    for line in output.lines() {
        let line = line.trim();

        if let Some(name) = line.strip_prefix("Rule Name:") {
            // Flush previous rule
            if !current_name.is_empty() && !current_action.is_empty() {
                // Skip Microsoft/Windows system rules
                if !current_name.starts_with("@") && !current_name.contains("Microsoft") {
                    rules.push(DiscoveredFirewallRule {
                        name: current_name.clone(),
                        action: current_action.clone(),
                        direction: current_direction.clone(),
                        protocol: current_protocol.clone(),
                        local_port: current_local_port,
                        remote_port: current_remote_port,
                        enabled: current_enabled,
                    });
                }
            }
            current_name = name.trim().to_string();
            current_action = String::new();
            current_direction = String::new();
            current_protocol = String::new();
            current_local_port = None;
            current_remote_port = None;
            current_enabled = true;
        } else if let Some(enabled) = line.strip_prefix("Enabled:") {
            current_enabled = enabled.trim().eq_ignore_ascii_case("Yes");
        } else if let Some(direction) = line.strip_prefix("Direction:") {
            current_direction = match direction.trim().to_lowercase().as_str() {
                "in" => "in".to_string(),
                "out" => "out".to_string(),
                _ => direction.trim().to_lowercase(),
            };
        } else if let Some(action) = line.strip_prefix("Action:") {
            current_action = match action.trim().to_lowercase().as_str() {
                "allow" => "allow".to_string(),
                "block" => "block".to_string(),
                _ => action.trim().to_lowercase(),
            };
        } else if let Some(proto) = line.strip_prefix("Protocol:") {
            current_protocol = proto.trim().to_lowercase();
        } else if let Some(port) = line.strip_prefix("LocalPort:") {
            let port_str = port.trim();
            if port_str != "Any" {
                // Handle single port (common case)
                current_local_port = port_str
                    .split(',')
                    .next()
                    .and_then(|p| p.trim().parse().ok());
            }
        } else if let Some(port) = line.strip_prefix("RemotePort:") {
            let port_str = port.trim();
            if port_str != "Any" {
                current_remote_port = port_str
                    .split(',')
                    .next()
                    .and_then(|p| p.trim().parse().ok());
            }
        }
    }

    // Flush last rule
    if !current_name.is_empty() && !current_action.is_empty() {
        if !current_name.starts_with("@") && !current_name.contains("Microsoft") {
            rules.push(DiscoveredFirewallRule {
                name: current_name,
                action: current_action,
                direction: current_direction,
                protocol: current_protocol,
                local_port: current_local_port,
                remote_port: current_remote_port,
                enabled: current_enabled,
            });
        }
    }

    rules
}

// =========================================================================
// Process Domain (AD Account)
// =========================================================================

/// Read the domain (AD account) for a process using wmic.
/// Returns the domain name if the process runs under a domain account.
pub fn read_process_domain(pid: u32) -> Option<String> {
    use std::process::Command;

    // Use wmic to get process owner domain
    let output = Command::new("wmic")
        .args([
            "process",
            "where",
            &format!("ProcessId={}", pid),
            "get",
            "Name",
            "/value",
        ])
        .output();

    // wmic process get can fail or return nothing, fallback to tasklist /v
    // tasklist /v /FI "PID eq <pid>" shows the user name as DOMAIN\user
    let output = Command::new("tasklist")
        .args(["/v", "/FI", &format!("PID eq {}", pid), "/FO", "CSV"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return None,
    };

    // Parse CSV output: "Image Name","PID","Session Name","Session#","Mem Usage","Status","User Name",...
    // User Name is in format "DOMAIN\user" or "NT AUTHORITY\SYSTEM"
    for line in output.lines().skip(1) {
        // Skip header
        let fields = parse_csv_line(line);
        if fields.len() >= 7 {
            let user_field = fields[6];
            if let Some(backslash_idx) = user_field.find('\\') {
                let domain = &user_field[..backslash_idx];
                // Skip local accounts and system accounts
                if !domain.is_empty()
                    && domain != "NT AUTHORITY"
                    && domain != "NT SERVICE"
                    && domain != "BUILTIN"
                {
                    return Some(domain.to_string());
                }
            }
        }
    }

    None
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
        assert_eq!(s.source, "java"); // java process has specific handling
        assert!(s.check_cmd.contains("wmic") || s.check_cmd.contains("java"));
        assert_eq!(matched, None);
    }

    #[test]
    fn test_suggest_commands_generic_fallback() {
        let services = vec![];
        let (suggestion, matched) = suggest_commands(9999, "myapp.exe", &services);
        assert!(suggestion.is_some());
        let s = suggestion.unwrap();
        assert_eq!(s.confidence, "low");
        assert_eq!(s.source, "process");
        assert!(s.check_cmd.contains("wmic"));
        assert_eq!(matched, None);
    }
}
