//! Passive topology discovery scanner — cross-platform (Linux + Windows).
//!
//! Scans the local host for running processes, TCP listeners, outbound connections,
//! system services, open config/log files, cron jobs, and generates command suggestions.
//! Sends an enriched `DiscoveryReport` to the backend.
//!
//! ## Platform support
//!
//! | Feature             | Linux                        | Windows                      |
//! |---------------------|------------------------------|------------------------------|
//! | Processes           | sysinfo (cross-platform)     | sysinfo (cross-platform)     |
//! | TCP listeners/conns | /proc/net/tcp + inode→PID    | netstat -ano (parsed)        |
//! | Services            | systemctl list-units         | sc query                     |
//! | Env vars            | /proc/[pid]/environ          | not collected (needs SeDebug)|
//! | Working dir         | /proc/[pid]/cwd              | not collected                |
//! | Config/log files    | /proc/[pid]/fd scanning      | not collected                |
//! | Config parsing      | YAML/properties/env/XML/JSON | not collected                |
//! | Cron/scheduled jobs | crontab + systemd timers     | schtasks /query              |
//! | Command suggestions | systemd cross-ref            | sc query cross-ref           |

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use chrono::Utc;
use std::collections::HashMap;
use sysinfo::System;
use uuid::Uuid;

use appcontrol_common::{
    AgentMessage, DiscoveredConnection, DiscoveredListener, DiscoveredProcess,
    DiscoveredScheduledJob, DiscoveredService,
};

/// System/kernel processes to exclude from discovery results.
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
    // Windows system processes
    "System",
    "Registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "services.exe",
    "lsass.exe",
    "svchost",
    "WmiPrvSE",
    "SearchIndexer",
    "RuntimeBroker",
    "fontdrvhost",
    "dwm",
    "Memory Compression",
    "Idle",
];

/// Environment variable prefixes/suffixes worth capturing for topology inference.
#[allow(dead_code)]
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

#[allow(dead_code)]
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

    // Platform-specific network scanning (listeners + connections with PIDs)
    let (mut listeners, connections) = scan_network(&sys);

    // Deduplicate listeners by port
    listeners.sort_by_key(|l| l.port);
    listeners.dedup_by_key(|l| l.port);

    // Platform-specific service scanning
    let services = scan_services();

    // Cross-platform process scanning with enrichment (config/log files, commands)
    let processes = scan_processes(&sys, &listeners, &services);

    // Platform-specific scheduled job scanning
    let scheduled_jobs = scan_scheduled_jobs();

    AgentMessage::DiscoveryReport {
        agent_id,
        hostname: hostname.to_string(),
        processes,
        listeners,
        connections,
        services,
        scheduled_jobs,
        scanned_at: Utc::now(),
    }
}

/// Scan network listeners and connections — dispatches to platform-specific code.
fn scan_network(sys: &System) -> (Vec<DiscoveredListener>, Vec<DiscoveredConnection>) {
    #[cfg(target_os = "linux")]
    {
        linux::scan_network(sys)
    }
    #[cfg(target_os = "windows")]
    {
        windows::scan_network(sys)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = sys;
        (Vec::new(), Vec::new())
    }
}

/// Scan system services — dispatches to platform-specific code.
fn scan_services() -> Vec<DiscoveredService> {
    #[cfg(target_os = "linux")]
    {
        linux::scan_services()
    }
    #[cfg(target_os = "windows")]
    {
        windows::scan_services()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// Read process environment variables — dispatches to platform-specific code.
fn read_process_env(pid: u32) -> HashMap<String, String> {
    #[cfg(target_os = "linux")]
    {
        linux::read_process_env(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Windows: reading env of another process requires SeDebugPrivilege,
        // which is not safe to assume. Skip for now.
        let _ = pid;
        HashMap::new()
    }
}

/// Scan scheduled jobs — dispatches to platform-specific code.
fn scan_scheduled_jobs() -> Vec<DiscoveredScheduledJob> {
    #[cfg(target_os = "linux")]
    {
        linux::scan_cron_jobs()
    }
    #[cfg(target_os = "windows")]
    {
        windows::scan_scheduled_tasks()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Cross-platform helpers
// ---------------------------------------------------------------------------

/// Check if a process name is a known system/kernel process.
fn is_system_process(name: &str) -> bool {
    SYSTEM_PROCESS_NAMES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Check if an environment variable key is interesting for topology inference.
#[allow(dead_code)]
pub(crate) fn is_interesting_env(key: &str) -> bool {
    let upper = key.to_uppercase();
    INTERESTING_ENV_PREFIXES
        .iter()
        .any(|p| upper.starts_with(p))
        || INTERESTING_ENV_SUFFIXES.iter().any(|s| upper.ends_with(s))
}

/// Enumerate all running processes (cross-platform via sysinfo),
/// filtering system processes and enriching with port/env/config/log/command data.
fn scan_processes(
    sys: &System,
    listeners: &[DiscoveredListener],
    services: &[DiscoveredService],
) -> Vec<DiscoveredProcess> {
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

            // Collect interesting environment variables (Linux only for now)
            let env_vars = read_process_env(pid_u32);

            // Read working directory (Linux only)
            let working_dir = read_working_dir(pid_u32);

            // Scan open files for configs and logs (Linux only)
            let (config_files, log_files) = scan_open_files(pid_u32);

            // Generate command suggestions (cross-platform)
            let (command_suggestion, matched_service) =
                suggest_commands(pid_u32, &name, &cmdline, services);

            Some(DiscoveredProcess {
                pid: pid_u32,
                name,
                cmdline,
                user,
                memory_bytes: p.memory(),
                cpu_pct: p.cpu_usage(),
                listening_ports,
                env_vars,
                working_dir,
                config_files,
                log_files,
                command_suggestion,
                matched_service,
            })
        })
        .collect()
}

/// Read the working directory of a process — dispatches to platform-specific code.
fn read_working_dir(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux::read_working_dir(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Scan open files for config and log files — dispatches to platform-specific code.
fn scan_open_files(
    pid: u32,
) -> (
    Vec<appcontrol_common::DiscoveredConfigFile>,
    Vec<appcontrol_common::DiscoveredLogFile>,
) {
    #[cfg(target_os = "linux")]
    {
        linux::scan_open_files(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        (Vec::new(), Vec::new())
    }
}

/// Generate command suggestions — dispatches to platform-specific code.
fn suggest_commands(
    pid: u32,
    name: &str,
    cmdline: &str,
    services: &[DiscoveredService],
) -> (Option<appcontrol_common::CommandSuggestion>, Option<String>) {
    #[cfg(target_os = "linux")]
    {
        linux::suggest_commands(pid, name, cmdline, services)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = cmdline;
        windows::suggest_commands(pid, name, services)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (pid, name, cmdline, services);
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_processes_returns_results() {
        let mut sys = System::new_all();
        sys.refresh_all();
        let services = Vec::new();
        let procs = scan_processes(&sys, &[], &services);
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
        assert!(is_system_process("svchost"));
        assert!(!is_system_process("nginx"));
        assert!(!is_system_process("java"));
        assert!(!is_system_process("postgres"));
    }

    #[test]
    fn test_interesting_env_detection() {
        assert!(is_interesting_env("DB_HOST"));
        assert!(is_interesting_env("DATABASE_URL"));
        assert!(is_interesting_env("REDIS_PORT"));
        assert!(is_interesting_env("MY_SERVICE_HOST"));
        assert!(!is_interesting_env("HOME"));
        assert!(!is_interesting_env("TERM"));
        assert!(!is_interesting_env("SHELL"));
    }
}
