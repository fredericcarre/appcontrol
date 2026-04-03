//! Discovery correlation: analyze reports, find services and dependencies.

use axum::{
    extract::{Extension, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbUuid;
use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CorrelateRequest {
    pub agent_ids: Vec<Uuid>,
}

/// Analyze recent discovery reports across selected agents.
/// Returns a structured correlation: services grouped by (process, port),
/// cross-host connections mapped to potential dependencies, config-based
/// dependencies, command suggestions, scheduled jobs, and unresolved
/// connections (to hosts not in the selected agent set).
pub async fn correlate(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CorrelateRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if body.agent_ids.is_empty() {
        return Err(ApiError::Validation(
            "At least one agent_id is required".to_string(),
        ));
    }

    // Fetch latest report per agent
    let mut reports: Vec<(DbUuid, String, serde_json::Value)> = Vec::new();
    for agent_id in &body.agent_ids {
        let row =
            crate::repository::discovery_queries::get_latest_report_for_agent(&state.db, *agent_id)
                .await?;
        if let Some(r) = row {
            reports.push(r);
        }
    }

    if reports.is_empty() {
        return Err(ApiError::Validation(
            "No discovery reports found for the specified agents".to_string(),
        ));
    }

    // Fetch agent IPs for connection matching
    let mut agent_ips: std::collections::HashMap<Uuid, Vec<String>> =
        std::collections::HashMap::new();
    let mut agent_hostnames: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();
    for (agent_id, hostname, _) in &reports {
        agent_hostnames.insert(agent_id.into_inner(), hostname.clone());
        let ips = crate::repository::discovery_queries::get_agent_ip_addresses(
            &state.db,
            agent_id.into_inner(),
        )
        .await?;
        if let Some(ips_val) = ips {
            if let Some(arr) = ips_val.as_array() {
                let ip_list: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                agent_ips.insert(agent_id.into_inner(), ip_list);
            }
        }
    }

    // Build services
    let mut services: Vec<Value> = Vec::new();
    let mut listen_index: std::collections::HashMap<(String, u16), usize> =
        std::collections::HashMap::new();

    for (agent_id, hostname, report) in &reports {
        let mut proc_ports: std::collections::HashMap<String, Vec<Value>> =
            std::collections::HashMap::new();

        if let Some(listeners) = report.get("listeners").and_then(|l| l.as_array()) {
            for listener in listeners {
                let port = listener.get("port").and_then(|p| p.as_u64()).unwrap_or(0);
                let proc_name = listener
                    .get("process_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let address = listener
                    .get("address")
                    .and_then(|a| a.as_str())
                    .unwrap_or("0.0.0.0");

                if !(1..=49151).contains(&port) {
                    continue;
                }

                proc_ports.entry(proc_name).or_default().push(json!({
                    "port": port,
                    "address": address,
                    "pid": listener.get("pid"),
                }));
            }
        }

        let processes = report
            .get("processes")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        let pid_to_process: std::collections::HashMap<u64, &Value> = processes
            .iter()
            .filter_map(|p| {
                let pid = p.get("pid").and_then(|v| v.as_u64())?;
                Some((pid, p))
            })
            .collect();

        for (proc_name, ports) in &proc_ports {
            let idx = services.len();
            let port_list: Vec<u16> = ports
                .iter()
                .filter_map(|p| p.get("port").and_then(|v| v.as_u64()).map(|v| v as u16))
                .collect();

            let first_pid = ports
                .iter()
                .filter_map(|p| p.get("pid").and_then(|v| v.as_u64()))
                .next();

            let process_data = first_pid.and_then(|pid| pid_to_process.get(&pid));

            let command_suggestion = process_data
                .and_then(|p| p.get("command_suggestion"))
                .cloned();
            let config_files = process_data
                .and_then(|p| p.get("config_files"))
                .cloned()
                .unwrap_or(json!([]));
            let log_files = process_data
                .and_then(|p| p.get("log_files"))
                .cloned()
                .unwrap_or(json!([]));
            let matched_service = process_data.and_then(|p| p.get("matched_service")).cloned();
            let technology_hint = process_data.and_then(|p| p.get("technology_hint")).cloned();

            let user_field = process_data
                .and_then(|p| p.get("user"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string());
            let env_vars = process_data
                .and_then(|p| p.get("env_vars"))
                .cloned()
                .unwrap_or(json!({}));

            let (display_name, component_type) = if let Some(ref tech) = technology_hint {
                let tech_name = tech
                    .get("display_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(proc_name);
                let tech_layer = tech.get("layer").and_then(|l| l.as_str()).unwrap_or("");
                let comp_type = layer_to_component_type(tech_layer)
                    .unwrap_or_else(|| guess_component_type(proc_name, &port_list));
                (format!("{}@{}", tech_name, hostname), comp_type)
            } else {
                (
                    format!("{}@{}", proc_name, hostname),
                    guess_component_type(proc_name, &port_list),
                )
            };

            services.push(json!({
                "agent_id": agent_id,
                "hostname": hostname,
                "process_name": proc_name,
                "ports": port_list,
                "port_details": ports,
                "suggested_name": display_name,
                "component_type": component_type,
                "command_suggestion": command_suggestion,
                "config_files": config_files,
                "log_files": log_files,
                "matched_service": matched_service,
                "technology_hint": technology_hint,
                "user": user_field,
                "env_vars": env_vars,
            }));

            for &port in &port_list {
                listen_index.insert((hostname.to_lowercase(), port), idx);
                if let Some(ips) = agent_ips.get(agent_id) {
                    for ip in ips {
                        listen_index.insert((ip.clone(), port), idx);
                    }
                }
            }
        }
    }

    // Add client services
    let mut client_services_added: std::collections::HashSet<(Uuid, String)> =
        std::collections::HashSet::new();

    for (agent_id, hostname, report) in &reports {
        if let Some(connections) = report.get("connections").and_then(|c| c.as_array()) {
            let mut proc_connections: std::collections::HashMap<String, Vec<&Value>> =
                std::collections::HashMap::new();
            for conn in connections {
                let proc_name = conn
                    .get("process_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                if !proc_name.is_empty() {
                    proc_connections.entry(proc_name).or_default().push(conn);
                }
            }

            let processes = report
                .get("processes")
                .and_then(|p| p.as_array())
                .cloned()
                .unwrap_or_default();
            let pid_to_process: std::collections::HashMap<u64, &Value> = processes
                .iter()
                .filter_map(|p| {
                    let pid = p.get("pid").and_then(|v| v.as_u64())?;
                    Some((pid, p))
                })
                .collect();

            for (proc_name, conns) in &proc_connections {
                if services.iter().any(|s| {
                    s.get("agent_id").and_then(|a| a.as_str()) == Some(&agent_id.to_string())
                        && s.get("process_name").and_then(|n| n.as_str()) == Some(proc_name)
                }) {
                    continue;
                }

                let connects_to_service = conns.iter().any(|conn| {
                    let remote_addr = conn
                        .get("remote_addr")
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    let remote_port = conn
                        .get("remote_port")
                        .and_then(|p| p.as_u64())
                        .unwrap_or(0) as u16;
                    listen_index.contains_key(&(remote_addr.to_string(), remote_port))
                });

                if !connects_to_service {
                    continue;
                }

                if !client_services_added.insert((agent_id.into_inner(), proc_name.clone())) {
                    continue;
                }

                let first_pid = conns
                    .iter()
                    .filter_map(|c| c.get("pid").and_then(|v| v.as_u64()))
                    .next();
                let process_data = first_pid.and_then(|pid| pid_to_process.get(&pid));

                let cmdline = process_data
                    .and_then(|p| p.get("cmdline"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let xc_service_name = extract_xcproperties_name(cmdline);

                let display_name = if let Some(ref xc_name) = xc_service_name {
                    format!("{}@{}", xc_name, hostname)
                } else {
                    format!("{}@{}", proc_name, hostname)
                };

                let command_suggestion = process_data
                    .and_then(|p| p.get("command_suggestion"))
                    .cloned();
                let technology_hint = process_data.and_then(|p| p.get("technology_hint")).cloned();

                let user_field = process_data
                    .and_then(|p| p.get("user"))
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string());
                let env_vars = process_data
                    .and_then(|p| p.get("env_vars"))
                    .cloned()
                    .unwrap_or(json!({}));

                let empty_ports: Vec<u16> = Vec::new();
                let empty_details: Vec<Value> = Vec::new();
                services.push(json!({
                    "agent_id": agent_id,
                    "hostname": hostname,
                    "process_name": proc_name,
                    "ports": empty_ports,
                    "port_details": empty_details,
                    "suggested_name": display_name,
                    "component_type": "backend",
                    "command_suggestion": command_suggestion,
                    "config_files": [],
                    "log_files": [],
                    "matched_service": xc_service_name,
                    "technology_hint": technology_hint,
                    "is_client_service": true,
                    "user": user_field,
                    "env_vars": env_vars,
                }));
            }
        }
    }

    // Build connections
    let mut resolved_deps: Vec<Value> = Vec::new();
    let mut unresolved_conns: Vec<Value> = Vec::new();
    let mut seen_deps: std::collections::HashSet<(usize, usize, String)> =
        std::collections::HashSet::new();

    let mut port_to_services: std::collections::HashMap<u16, Vec<usize>> =
        std::collections::HashMap::new();
    for (key, &idx) in &listen_index {
        port_to_services.entry(key.1).or_default().push(idx);
    }
    for indices in port_to_services.values_mut() {
        indices.sort();
        indices.dedup();
    }

    for (agent_id, hostname, report) in &reports {
        // TCP connection-based dependencies
        if let Some(connections) = report.get("connections").and_then(|c| c.as_array()) {
            for conn in connections {
                let remote_addr = conn
                    .get("remote_addr")
                    .and_then(|a| a.as_str())
                    .unwrap_or("");
                let remote_port = conn
                    .get("remote_port")
                    .and_then(|p| p.as_u64())
                    .unwrap_or(0) as u16;
                let conn_proc = conn
                    .get("process_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                if remote_addr.is_empty() || remote_port == 0 {
                    continue;
                }

                let target_idx = find_target_service(
                    &listen_index,
                    &port_to_services,
                    remote_addr,
                    remote_port,
                    &agent_hostnames,
                );

                if let Some(to_idx) = target_idx {
                    let from_idx = find_service_index(&services, agent_id, conn_proc);

                    if let Some(from_idx) = from_idx {
                        let dep_key = (from_idx, to_idx, "tcp_connection".to_string());
                        if from_idx != to_idx && !seen_deps.contains(&dep_key) {
                            seen_deps.insert(dep_key);
                            resolved_deps.push(json!({
                                "from_service_index": from_idx,
                                "to_service_index": to_idx,
                                "from_process": conn_proc,
                                "to_process": services[to_idx].get("process_name"),
                                "remote_addr": remote_addr,
                                "remote_port": remote_port,
                                "inferred_via": "tcp_connection",
                            }));
                        }
                    } else {
                        resolved_deps.push(json!({
                            "from_service_index": null,
                            "to_service_index": to_idx,
                            "from_process": conn_proc,
                            "from_hostname": hostname,
                            "from_agent_id": agent_id,
                            "to_process": services[to_idx].get("process_name"),
                            "remote_addr": remote_addr,
                            "remote_port": remote_port,
                            "inferred_via": "tcp_connection",
                        }));
                    }
                } else {
                    unresolved_conns.push(json!({
                        "from_hostname": hostname,
                        "from_agent_id": agent_id,
                        "from_process": conn_proc,
                        "remote_addr": remote_addr,
                        "remote_port": remote_port,
                    }));
                }
            }
        }

        // Config-based dependencies
        if let Some(processes) = report.get("processes").and_then(|p| p.as_array()) {
            for proc in processes {
                let proc_name = proc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if let Some(configs) = proc.get("config_files").and_then(|c| c.as_array()) {
                    for config in configs {
                        if let Some(endpoints) =
                            config.get("extracted_endpoints").and_then(|e| e.as_array())
                        {
                            for ep in endpoints {
                                let parsed_host =
                                    ep.get("parsed_host").and_then(|h| h.as_str()).unwrap_or("");
                                let parsed_port =
                                    ep.get("parsed_port").and_then(|p| p.as_u64()).unwrap_or(0)
                                        as u16;
                                let config_key =
                                    ep.get("key").and_then(|k| k.as_str()).unwrap_or("");
                                let technology =
                                    ep.get("technology").and_then(|t| t.as_str()).unwrap_or("");

                                if parsed_host.is_empty() && parsed_port == 0 {
                                    continue;
                                }

                                let target_idx = if parsed_port > 0 {
                                    listen_index
                                        .get(&(parsed_host.to_lowercase(), parsed_port))
                                        .copied()
                                } else {
                                    None
                                };

                                if let Some(to_idx) = target_idx {
                                    let from_idx =
                                        find_service_index(&services, agent_id, proc_name);
                                    if let Some(from_idx) = from_idx {
                                        let dep_key = (from_idx, to_idx, "config_file".to_string());
                                        if from_idx != to_idx && !seen_deps.contains(&dep_key) {
                                            seen_deps.insert(dep_key);
                                            resolved_deps.push(json!({
                                                "from_service_index": from_idx,
                                                "to_service_index": to_idx,
                                                "from_process": proc_name,
                                                "to_process": services[to_idx].get("process_name"),
                                                "remote_addr": parsed_host,
                                                "remote_port": parsed_port,
                                                "inferred_via": "config_file",
                                                "config_key": config_key,
                                                "technology": technology,
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduplicate unresolved
    let mut seen_unresolved: std::collections::HashSet<(String, u16)> =
        std::collections::HashSet::new();
    unresolved_conns.retain(|c| {
        let addr = c
            .get("remote_addr")
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();
        let port = c.get("remote_port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
        seen_unresolved.insert((addr, port))
    });

    // Collect scheduled jobs
    let mut scheduled_jobs: Vec<Value> = Vec::new();
    for (_, hostname, report) in &reports {
        if let Some(jobs) = report.get("scheduled_jobs").and_then(|j| j.as_array()) {
            for job in jobs {
                let mut job_with_host = job.clone();
                if let Some(obj) = job_with_host.as_object_mut() {
                    obj.insert("hostname".to_string(), json!(hostname));
                }
                scheduled_jobs.push(job_with_host);
            }
        }
    }

    // Collect system services
    let mut system_services: Vec<Value> = Vec::new();
    for (agent_id, hostname, report) in &reports {
        if let Some(svcs) = report.get("services").and_then(|s| s.as_array()) {
            for svc in svcs {
                let mut svc_with_host = svc.clone();
                if let Some(obj) = svc_with_host.as_object_mut() {
                    obj.insert("hostname".to_string(), json!(hostname));
                    obj.insert("agent_id".to_string(), json!(agent_id));

                    let svc_name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("");

                    let is_windows = report
                        .get("os_type")
                        .and_then(|o| o.as_str())
                        .map(|o| o.to_lowercase().contains("windows"))
                        .unwrap_or_else(|| {
                            svc.get("display_name")
                                .and_then(|d| d.as_str())
                                .map(|d| d.contains(' '))
                                .unwrap_or(false)
                        });

                    if is_windows {
                        obj.insert(
                            "check_cmd".to_string(),
                            json!(format!("sc query {} | findstr RUNNING", svc_name)),
                        );
                        obj.insert(
                            "start_cmd".to_string(),
                            json!(format!("net start {}", svc_name)),
                        );
                        obj.insert(
                            "stop_cmd".to_string(),
                            json!(format!("net stop {}", svc_name)),
                        );
                    } else {
                        obj.insert(
                            "check_cmd".to_string(),
                            json!(format!("systemctl is-active {}", svc_name)),
                        );
                        obj.insert(
                            "start_cmd".to_string(),
                            json!(format!("systemctl start {}", svc_name)),
                        );
                        obj.insert(
                            "stop_cmd".to_string(),
                            json!(format!("systemctl stop {}", svc_name)),
                        );
                    }
                }
                system_services.push(svc_with_host);
            }
        }
    }

    Ok(Json(json!({
        "agents_analyzed": reports.len(),
        "services": services,
        "dependencies": resolved_deps,
        "unresolved_connections": unresolved_conns,
        "scheduled_jobs": scheduled_jobs,
        "system_services": system_services,
    })))
}

// ============================================================================
// Helper functions
// ============================================================================

fn find_service_index(services: &[Value], agent_id: &Uuid, proc_name: &str) -> Option<usize> {
    if proc_name.is_empty() {
        return None;
    }
    services.iter().position(|s| {
        s.get("agent_id")
            .and_then(|a| a.as_str())
            .map(|a| a == agent_id.to_string())
            .unwrap_or(false)
            && s.get("process_name")
                .and_then(|n| n.as_str())
                .map(|n| n == proc_name)
                .unwrap_or(false)
    })
}

fn find_target_service(
    listen_index: &std::collections::HashMap<(String, u16), usize>,
    port_to_services: &std::collections::HashMap<u16, Vec<usize>>,
    remote_addr: &str,
    remote_port: u16,
    agent_hostnames: &std::collections::HashMap<Uuid, String>,
) -> Option<usize> {
    if let Some(&idx) = listen_index.get(&(remote_addr.to_string(), remote_port)) {
        return Some(idx);
    }

    let lower_addr = remote_addr.to_lowercase();
    if let Some(&idx) = listen_index.get(&(lower_addr.clone(), remote_port)) {
        return Some(idx);
    }

    let well_known_ports: &[u16] = &[
        5432, 3306, 1521, 1433, 27017, 6379, 5672, 15672, 9200, 9092, 2181, 8080, 8443,
    ];

    if well_known_ports.contains(&remote_port) {
        if let Some(indices) = port_to_services.get(&remote_port) {
            if indices.len() == 1 {
                return Some(indices[0]);
            }
        }
    }

    if !remote_addr.chars().all(|c| c.is_ascii_digit() || c == '.') {
        for hostname in agent_hostnames.values() {
            let hostname_lower = hostname.to_lowercase();
            if hostname_lower.starts_with(&lower_addr) {
                if let Some(&idx) = listen_index.get(&(hostname_lower.clone(), remote_port)) {
                    return Some(idx);
                }
            }
            if lower_addr.starts_with(&hostname_lower) {
                if let Some(&idx) = listen_index.get(&(hostname_lower, remote_port)) {
                    return Some(idx);
                }
            }
        }
    }

    None
}

fn extract_xcproperties_name(cmdline: &str) -> Option<String> {
    if let Some(idx) = cmdline.find(".xcproperties") {
        let before = &cmdline[..idx];
        let start = before.rfind(['\\', '/', ' ']).map(|i| i + 1).unwrap_or(0);
        let name = &before[start..];
        if !name.is_empty() {
            let name_cleaned = name.trim_end_matches(|c: char| c.is_ascii_digit());
            if !name_cleaned.is_empty() {
                return Some(name_cleaned.to_string());
            }
            return Some(name.to_string());
        }
    }
    None
}

fn layer_to_component_type(layer: &str) -> Option<&'static str> {
    match layer {
        "Database" => Some("database"),
        "Middleware" => Some("middleware"),
        "Application" => Some("appserver"),
        "Access Points" => Some("webfront"),
        "Scheduler" => Some("batch"),
        "File Transfer" => Some("service"),
        "Security" => Some("service"),
        "Infrastructure" => Some("service"),
        _ => None,
    }
}

fn guess_component_type(process_name: &str, ports: &[u16]) -> &'static str {
    let name = process_name.to_lowercase();

    if name.contains("postgres") || name.contains("pgbouncer") {
        return "database";
    }
    if name.contains("mysql") || name.contains("mariadb") || name.contains("mysqld") {
        return "database";
    }
    if name.contains("mongo") {
        return "database";
    }
    if name.contains("oracle") || name.contains("tnslsnr") {
        return "database";
    }
    if name.contains("sqlservr") || name.contains("mssql") {
        return "database";
    }
    if name.contains("redis") || name.contains("memcache") {
        return "cache";
    }
    if name.contains("elasticsearch") || name.contains("solr") {
        return "search";
    }

    if name.contains("kafka")
        || name.contains("rabbit")
        || name.contains("activemq")
        || name.contains("mosquitto")
        || name == "erl"
        || name == "erl.exe"
    {
        return "queue";
    }

    if name.contains("nginx")
        || name.contains("httpd")
        || name.contains("apache")
        || name.contains("haproxy")
        || name.contains("envoy")
        || name.contains("traefik")
        || name.contains("iis")
        || name.contains("w3wp")
    {
        return "proxy";
    }

    if name.contains("restservice") || name.contains("apiservice") || name.contains("webapi") {
        return "api";
    }

    if name == "java" || name == "java.exe" || name.contains("javaw") {
        for port in ports {
            match port {
                9200 | 9300 => return "search",
                9092 | 2181 => return "queue",
                8080 | 8443 | 8000..=8099 => return "appserver",
                1521 => return "database",
                _ => {}
            }
        }
        return "appserver";
    }

    if name.contains("xcruntime") || name.contains("xcomponent") {
        return "service";
    }
    if name.contains("dotnet") || name.ends_with(".dll") {
        return "service";
    }

    for port in ports {
        match port {
            5432 | 3306 | 1521 | 1433 | 27017 => return "database",
            6379 | 11211 => return "cache",
            9092 | 5672 | 61616 | 1883 => return "queue",
            9200 | 8983 => return "search",
            80 | 443 | 8080 | 8443 => return "web",
            9000..=9099 => return "api",
            _ => {}
        }
    }

    "service"
}
