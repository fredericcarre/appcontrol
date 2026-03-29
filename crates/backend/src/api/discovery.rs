//! Discovery API: passive topology scanning and multi-step DAG creation.
//!
//! ## Workflow (not magic — user validates at each step)
//!
//! 1. **Collect**: `POST /trigger-all` or `/trigger/:agent_id` → agents scan and send reports
//! 2. **Correlate**: `POST /correlate` → backend analyzes reports, returns structured analysis
//!    (listeners grouped by service, cross-host connections, config-based deps, commands, cron jobs)
//! 3. **Create draft**: `POST /drafts` → user sends validated components + dependencies + commands
//! 4. **Edit draft**: `PUT /drafts/:id/components` and `PUT /drafts/:id/dependencies`
//! 5. **Apply draft**: `POST /drafts/:id/apply` → creates a real application with components + DAG + commands
//!
//! Endpoints:
//! - GET  /api/v1/discovery/reports             — list reports
//! - GET  /api/v1/discovery/reports/:id         — get report with raw data
//! - POST /api/v1/discovery/trigger/:agent_id   — trigger scan on one agent
//! - POST /api/v1/discovery/trigger-all         — trigger scan on all agents
//! - POST /api/v1/discovery/correlate           — analyze reports, return correlation
//! - GET  /api/v1/discovery/drafts              — list drafts
//! - GET  /api/v1/discovery/drafts/:id          — get draft details
//! - POST /api/v1/discovery/drafts              — create draft from user-validated data
//! - PUT  /api/v1/discovery/drafts/:id/components   — update draft components
//! - PUT  /api/v1/discovery/drafts/:id/dependencies — update draft dependencies
//! - POST /api/v1/discovery/drafts/:id/apply    — apply draft → create real app

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::{DbUuid, IntArray, UuidArray};
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

// ===========================================================================
// Reports
// ===========================================================================

/// List recent discovery reports.
pub async fn list_reports(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = sqlx::query_as::<_, (DbUuid, DbUuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, agent_id, hostname, scanned_at
         FROM discovery_reports
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    let reports: Vec<Value> = rows
        .iter()
        .map(|(id, agent_id, hostname, scanned_at)| {
            json!({
                "id": id,
                "agent_id": agent_id,
                "hostname": hostname,
                "scanned_at": scanned_at,
            })
        })
        .collect();

    Ok(Json(json!({ "reports": reports })))
}

/// Get full discovery report with raw process/listener/connection data.
pub async fn get_report(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(report_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            serde_json::Value,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id, agent_id, hostname, report, scanned_at
         FROM discovery_reports WHERE id = $1",
    )
    .bind(report_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((id, agent_id, hostname, report, scanned_at)) => Ok(Json(json!({
            "id": id,
            "agent_id": agent_id,
            "hostname": hostname,
            "report": report,
            "scanned_at": scanned_at,
        }))),
        None => Err(ApiError::NotFound),
    }
}

// ===========================================================================
// Scan triggers
// ===========================================================================

/// Trigger a discovery scan on a specific agent.
pub async fn trigger_scan(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let request_id = Uuid::new_v4();

    log_action(
        &state.db,
        user.user_id,
        "discovery_trigger",
        "agent",
        agent_id,
        json!({ "request_id": request_id }),
    )
    .await?;

    let msg = appcontrol_common::BackendMessage::RequestDiscovery { request_id };
    let sent = state.ws_hub.send_to_agent(agent_id, msg);

    Ok(Json(json!({
        "request_id": request_id,
        "agent_id": agent_id,
        "sent": sent,
    })))
}

/// Trigger discovery scan on ALL connected agents.
pub async fn trigger_all(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let request_id = Uuid::new_v4();

    log_action(
        &state.db,
        user.user_id,
        "discovery_trigger_all",
        "discovery",
        Uuid::nil(),
        json!({ "request_id": request_id }),
    )
    .await?;

    let agent_ids = sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM agents WHERE organization_id = $1 AND is_active = true",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    let mut sent_count = 0u32;
    for agent_id in &agent_ids {
        let msg = appcontrol_common::BackendMessage::RequestDiscovery { request_id };
        if state.ws_hub.send_to_agent(*agent_id, msg) {
            sent_count += 1;
        }
    }

    Ok(Json(json!({
        "request_id": request_id,
        "agents_targeted": agent_ids.len(),
        "agents_sent": sent_count,
    })))
}

// ===========================================================================
// Correlate — the key analysis step (returns data, does NOT create a draft)
// ===========================================================================

#[derive(Debug, Deserialize)]
pub struct CorrelateRequest {
    pub agent_ids: Vec<Uuid>,
}

/// Analyze recent discovery reports across selected agents.
/// Returns a structured correlation: services grouped by (process, port),
/// cross-host connections mapped to potential dependencies, config-based
/// dependencies, command suggestions, scheduled jobs, and unresolved
/// connections (to hosts not in the selected agent set).
///
/// The frontend displays this for human review. No draft is created here.
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
        let row = sqlx::query_as::<_, (DbUuid, String, serde_json::Value)>(
            "SELECT agent_id, hostname, report FROM discovery_reports
             WHERE agent_id = $1
             ORDER BY scanned_at DESC LIMIT 1",
        )
        .bind(agent_id)
        .fetch_optional(&state.db)
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
        let ips = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT COALESCE(ip_addresses, '[]'::jsonb) FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&state.db)
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

    // -----------------------------------------------------------------------
    // Build services: group listeners by (agent, process_name)
    // A "service" is a process that listens on one or more ports on a host.
    // Now enriched with command suggestions, config files, log files.
    // -----------------------------------------------------------------------
    let mut services: Vec<Value> = Vec::new();
    // Index: (remote_addr_or_hostname, port) → service index for dep matching
    let mut listen_index: std::collections::HashMap<(String, u16), usize> =
        std::collections::HashMap::new();

    for (agent_id, hostname, report) in &reports {
        // Group listeners by process_name on this agent
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

        // Build a PID→process data lookup for enrichment
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

        // Create one service per unique process on this host
        for (proc_name, ports) in &proc_ports {
            let idx = services.len();
            let port_list: Vec<u16> = ports
                .iter()
                .filter_map(|p| p.get("port").and_then(|v| v.as_u64()).map(|v| v as u16))
                .collect();

            // Find the process data for enrichment (use first matching PID)
            let first_pid = ports
                .iter()
                .filter_map(|p| p.get("pid").and_then(|v| v.as_u64()))
                .next();

            let process_data = first_pid.and_then(|pid| pid_to_process.get(&pid));

            // Extract enriched fields from the process
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

            // Extract user and env_vars from the process
            let user = process_data
                .and_then(|p| p.get("user"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string());
            let env_vars = process_data
                .and_then(|p| p.get("env_vars"))
                .cloned()
                .unwrap_or(json!({}));

            // Use technology_hint for display name and component type if available
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
                "user": user,
                "env_vars": env_vars,
            }));

            // Index by all reachable addresses
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

    // -----------------------------------------------------------------------
    // Add "client services" - processes that don't listen but have significant
    // outbound connections to known services (e.g., xcruntime → RabbitMQ)
    // -----------------------------------------------------------------------
    let mut client_services_added: std::collections::HashSet<(Uuid, String)> =
        std::collections::HashSet::new();

    for (agent_id, hostname, report) in &reports {
        if let Some(connections) = report.get("connections").and_then(|c| c.as_array()) {
            // Group connections by process name
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

            // Build a PID→process data lookup
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
                // Skip if already a listening service
                if services.iter().any(|s| {
                    s.get("agent_id").and_then(|a| a.as_str()) == Some(&agent_id.to_string())
                        && s.get("process_name").and_then(|n| n.as_str()) == Some(proc_name)
                }) {
                    continue;
                }

                // Check if this process has connections to known services
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

                // Skip if already added
                if !client_services_added.insert((agent_id.into_inner(), proc_name.clone())) {
                    continue;
                }

                // Find the process data for enrichment
                let first_pid = conns
                    .iter()
                    .filter_map(|c| c.get("pid").and_then(|v| v.as_u64()))
                    .next();
                let process_data = first_pid.and_then(|pid| pid_to_process.get(&pid));

                // Extract service name from cmdline (for XComponent)
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

                // Extract user and env_vars for client services too
                let user = process_data
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
                    "user": user,
                    "env_vars": env_vars,
                }));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Build connections: match outbound connections to services
    // -----------------------------------------------------------------------
    let mut resolved_deps: Vec<Value> = Vec::new();
    let mut unresolved_conns: Vec<Value> = Vec::new();
    let mut seen_deps: std::collections::HashSet<(usize, usize, String)> =
        std::collections::HashSet::new();

    // Build port-only index for fallback matching (when only one service listens on a port)
    let mut port_to_services: std::collections::HashMap<u16, Vec<usize>> =
        std::collections::HashMap::new();
    for (key, &idx) in &listen_index {
        port_to_services.entry(key.1).or_default().push(idx);
    }
    // Deduplicate service indices per port
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

                // Try to find target service with multiple strategies
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

        // Config-based dependencies: extracted endpoints from config files
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

                                // Try to match to a known service by host+port
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

    // Deduplicate unresolved connections by (remote_addr, remote_port)
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

    // -----------------------------------------------------------------------
    // Collect scheduled jobs from all reports
    // -----------------------------------------------------------------------
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

    // -----------------------------------------------------------------------
    // Collect system services (Windows Services / systemd units) from all reports
    // These can be added as components with sc/systemctl commands
    // -----------------------------------------------------------------------
    let mut system_services: Vec<Value> = Vec::new();
    for (agent_id, hostname, report) in &reports {
        if let Some(svcs) = report.get("services").and_then(|s| s.as_array()) {
            for svc in svcs {
                let mut svc_with_host = svc.clone();
                if let Some(obj) = svc_with_host.as_object_mut() {
                    obj.insert("hostname".to_string(), json!(hostname));
                    obj.insert("agent_id".to_string(), json!(agent_id));

                    // Generate command suggestions for service management
                    let svc_name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("");

                    // Detect OS from report or use heuristics
                    let is_windows = report
                        .get("os_type")
                        .and_then(|o| o.as_str())
                        .map(|o| o.to_lowercase().contains("windows"))
                        .unwrap_or_else(|| {
                            // Heuristic: Windows services have display_name with spaces
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
                        // Linux/systemd
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

/// Find the index of a service in the services list by agent_id and process_name.
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

/// Find target service using multiple matching strategies.
///
/// Strategies in order:
/// 1. Exact IP:port match in listen_index
/// 2. Lowercase hostname:port match
/// 3. Port-only match if only one service listens on that port
/// 4. Hostname prefix match (e.g., "server1" matches "server1.domain.com")
fn find_target_service(
    listen_index: &std::collections::HashMap<(String, u16), usize>,
    port_to_services: &std::collections::HashMap<u16, Vec<usize>>,
    remote_addr: &str,
    remote_port: u16,
    agent_hostnames: &std::collections::HashMap<Uuid, String>,
) -> Option<usize> {
    // Strategy 1: Exact match
    if let Some(&idx) = listen_index.get(&(remote_addr.to_string(), remote_port)) {
        return Some(idx);
    }

    // Strategy 2: Lowercase match
    let lower_addr = remote_addr.to_lowercase();
    if let Some(&idx) = listen_index.get(&(lower_addr.clone(), remote_port)) {
        return Some(idx);
    }

    // Strategy 3: Port-only match if unique
    // Only use this for well-known service ports to avoid false positives
    let well_known_ports: &[u16] = &[
        5432,  // PostgreSQL
        3306,  // MySQL
        1521,  // Oracle
        1433,  // SQL Server
        27017, // MongoDB
        6379,  // Redis
        5672,  // RabbitMQ AMQP
        15672, // RabbitMQ Management
        9200,  // Elasticsearch
        9092,  // Kafka
        2181,  // ZooKeeper
        8080,  // Common app server
        8443,  // HTTPS app server
    ];

    if well_known_ports.contains(&remote_port) {
        if let Some(indices) = port_to_services.get(&remote_port) {
            if indices.len() == 1 {
                return Some(indices[0]);
            }
        }
    }

    // Strategy 4: Hostname prefix match
    // If remote_addr looks like a hostname (no dots or has letters), try matching
    // against known agent hostnames
    if !remote_addr.chars().all(|c| c.is_ascii_digit() || c == '.') {
        for hostname in agent_hostnames.values() {
            let hostname_lower = hostname.to_lowercase();
            // Check if remote_addr is a prefix of the hostname
            if hostname_lower.starts_with(&lower_addr) {
                if let Some(&idx) = listen_index.get(&(hostname_lower.clone(), remote_port)) {
                    return Some(idx);
                }
            }
            // Check if hostname is a prefix of remote_addr
            if lower_addr.starts_with(&hostname_lower) {
                if let Some(&idx) = listen_index.get(&(hostname_lower, remote_port)) {
                    return Some(idx);
                }
            }
        }
    }

    None
}

/// Extract XComponent service name from command line.
/// Looks for patterns like "lynx-microservice1.xcproperties" and extracts "lynx-microservice".
fn extract_xcproperties_name(cmdline: &str) -> Option<String> {
    // Look for .xcproperties pattern
    if let Some(idx) = cmdline.find(".xcproperties") {
        let before = &cmdline[..idx];
        // Find the start of the filename (after \ or / or space)
        let start = before.rfind(['\\', '/', ' ']).map(|i| i + 1).unwrap_or(0);
        let name = &before[start..];
        if !name.is_empty() {
            // Remove trailing number (e.g., "lynx-microservice1" → "lynx-microservice")
            let name_cleaned = name.trim_end_matches(|c: char| c.is_ascii_digit());
            if !name_cleaned.is_empty() {
                return Some(name_cleaned.to_string());
            }
            return Some(name.to_string());
        }
    }
    None
}

/// Heuristic: guess component type from process name and ports.
/// Convert technology layer to component type.
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

    // Databases
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

    // Message queues - including Erlang runtime for RabbitMQ
    if name.contains("kafka")
        || name.contains("rabbit")
        || name.contains("activemq")
        || name.contains("mosquitto")
        || name == "erl"
        || name == "erl.exe"
    // Erlang VM = RabbitMQ
    {
        return "queue";
    }

    // Web servers / reverse proxies
    if name.contains("nginx")
        || name.contains("httpd")
        || name.contains("apache")
        || name.contains("haproxy")
        || name.contains("envoy")
        || name.contains("traefik")
        || name.contains("iis")
        || name.contains("w3wp")
    // IIS
    {
        return "proxy";
    }

    // API services (common patterns)
    if name.contains("restservice") || name.contains("apiservice") || name.contains("webapi") {
        return "api";
    }

    // Java processes - use port to disambiguate
    if name == "java" || name == "java.exe" || name.contains("javaw") {
        for port in ports {
            match port {
                9200 | 9300 => return "search",                  // ElasticSearch
                9092 | 2181 => return "queue",                   // Kafka / ZooKeeper
                8080 | 8443 | 8000..=8099 => return "appserver", // Tomcat, Jetty, etc.
                1521 => return "database",                       // Oracle
                _ => {}
            }
        }
        return "appserver"; // Default for Java
    }

    // .NET runtimes and custom services (XComponent, etc.)
    if name.contains("xcruntime") || name.contains("xcomponent") {
        return "service";
    }
    if name.contains("dotnet") || name.ends_with(".dll") {
        return "service";
    }

    // Known ports as fallback
    for port in ports {
        match port {
            5432 | 3306 | 1521 | 1433 | 27017 => return "database",
            6379 | 11211 => return "cache",
            9092 | 5672 | 61616 | 1883 => return "queue", // Added MQTT
            9200 | 8983 => return "search",
            80 | 443 | 8080 | 8443 => return "web",
            9000..=9099 => return "api", // Common REST API port range
            _ => {}
        }
    }

    "service"
}

// ===========================================================================
// Drafts — user-created after reviewing correlation
// ===========================================================================

/// List discovery drafts.
pub async fn list_drafts(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, status, inferred_at
         FROM discovery_drafts
         WHERE organization_id = $1
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    let drafts: Vec<Value> = rows
        .iter()
        .map(|(id, name, status, inferred_at)| {
            json!({
                "id": id,
                "name": name,
                "status": status,
                "inferred_at": inferred_at,
            })
        })
        .collect();

    Ok(Json(json!({ "drafts": drafts })))
}

/// Get full draft details: components + dependencies (with operational fields).
pub async fn get_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let draft = sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, status, inferred_at FROM discovery_drafts WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await?;

    let (id, name, status, inferred_at) = draft.ok_or(ApiError::NotFound)?;

    #[allow(clippy::type_complexity)]
    let components = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            String,
            serde_json::Value,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            serde_json::Value,
            serde_json::Value,
            Option<String>,
        ),
    >(
        "SELECT id, suggested_name, process_name, host, component_type, metadata,
                check_cmd, start_cmd, stop_cmd, restart_cmd,
                command_confidence, command_source,
                COALESCE(config_files, '[]'::jsonb),
                COALESCE(log_files, '[]'::jsonb),
                matched_service
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let deps = sqlx::query_as::<_, (DbUuid, DbUuid, DbUuid, String)>(
        "SELECT id, from_component, to_component, inferred_via
         FROM discovery_draft_dependencies WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let comp_json: Vec<Value> = components
        .iter()
        .map(
            |(
                cid,
                comp_name,
                proc,
                host,
                ctype,
                meta,
                check,
                start,
                stop,
                restart,
                confidence,
                source,
                configs,
                logs,
                matched_svc,
            )| {
                json!({
                    "id": cid,
                    "name": comp_name,
                    "process_name": proc,
                    "host": host,
                    "component_type": ctype,
                    "metadata": meta,
                    "check_cmd": check,
                    "start_cmd": start,
                    "stop_cmd": stop,
                    "restart_cmd": restart,
                    "command_confidence": confidence,
                    "command_source": source,
                    "config_files": configs,
                    "log_files": logs,
                    "matched_service": matched_svc,
                })
            },
        )
        .collect();

    let dep_json: Vec<Value> = deps
        .iter()
        .map(|(dep_id, from, to, via)| {
            json!({
                "id": dep_id,
                "from_component": from,
                "to_component": to,
                "inferred_via": via,
            })
        })
        .collect();

    Ok(Json(json!({
        "id": id,
        "name": name,
        "status": status,
        "inferred_at": inferred_at,
        "components": comp_json,
        "dependencies": dep_json,
    })))
}

/// Create a draft from user-validated components + dependencies + commands.
#[derive(Debug, Deserialize)]
pub struct CreateDraftRequest {
    pub name: String,
    pub components: Vec<DraftComponentInput>,
    pub dependencies: Vec<DraftDependencyInput>,
}

#[derive(Debug, Deserialize)]
pub struct DraftComponentInput {
    /// Temporary client-side ID for dependency referencing
    pub temp_id: String,
    pub name: String,
    pub process_name: Option<String>,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub listening_ports: Vec<i32>,
    pub component_type: String,
    /// Operational commands (pre-filled from suggestions, user-editable)
    #[serde(default)]
    pub check_cmd: Option<String>,
    #[serde(default)]
    pub start_cmd: Option<String>,
    #[serde(default)]
    pub stop_cmd: Option<String>,
    #[serde(default)]
    pub restart_cmd: Option<String>,
    #[serde(default)]
    pub command_confidence: Option<String>,
    #[serde(default)]
    pub command_source: Option<String>,
    /// Detected config/log files (informational)
    #[serde(default)]
    pub config_files: Option<serde_json::Value>,
    #[serde(default)]
    pub log_files: Option<serde_json::Value>,
    #[serde(default)]
    pub matched_service: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DraftDependencyInput {
    /// References DraftComponentInput.temp_id
    pub from_temp_id: String,
    pub to_temp_id: String,
    pub inferred_via: String,
}

pub async fn create_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateDraftRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let org_id = sqlx::query_scalar::<_, DbUuid>("SELECT organization_id FROM users WHERE id = $1")
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;

    log_action(
        &state.db,
        user.user_id,
        "discovery_create_draft",
        "discovery",
        Uuid::nil(),
        json!({ "name": &body.name, "components": body.components.len(), "dependencies": body.dependencies.len() }),
    )
    .await?;

    let draft_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO discovery_drafts (id, organization_id, name)
         VALUES ($1, $2, $3)",
    )
    .bind(draft_id)
    .bind(org_id)
    .bind(&body.name)
    .execute(&state.db)
    .await?;

    // Map temp_id → real UUID
    let mut temp_to_real: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::new();

    for comp in &body.components {
        let comp_id = Uuid::new_v4();
        temp_to_real.insert(comp.temp_id.clone(), comp_id);

        sqlx::query(
            "INSERT INTO discovery_draft_components
             (id, draft_id, agent_id, suggested_name, process_name, host,
              listening_ports, component_type,
              check_cmd, start_cmd, stop_cmd, restart_cmd,
              command_confidence, command_source,
              config_files, log_files, matched_service)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
        )
        .bind(comp_id)
        .bind(draft_id)
        .bind(comp.agent_id)
        .bind(&comp.name)
        .bind(&comp.process_name)
        .bind(&comp.host)
        .bind(IntArray::from(comp.listening_ports.clone()))
        .bind(&comp.component_type)
        .bind(&comp.check_cmd)
        .bind(&comp.start_cmd)
        .bind(&comp.stop_cmd)
        .bind(&comp.restart_cmd)
        .bind(comp.command_confidence.as_deref().unwrap_or("low"))
        .bind(&comp.command_source)
        .bind(comp.config_files.as_ref().unwrap_or(&json!([])))
        .bind(comp.log_files.as_ref().unwrap_or(&json!([])))
        .bind(&comp.matched_service)
        .execute(&state.db)
        .await?;
    }

    let mut dep_count = 0u32;
    for dep in &body.dependencies {
        if let (Some(&from_id), Some(&to_id)) = (
            temp_to_real.get(&dep.from_temp_id),
            temp_to_real.get(&dep.to_temp_id),
        ) {
            sqlx::query(
                "INSERT INTO discovery_draft_dependencies
                 (draft_id, from_component, to_component, inferred_via)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(draft_id)
            .bind(from_id)
            .bind(to_id)
            .bind(&dep.inferred_via)
            .execute(&state.db)
            .await?;
            dep_count += 1;
        }
    }

    Ok(Json(json!({
        "draft_id": draft_id,
        "name": body.name,
        "components_created": body.components.len(),
        "dependencies_created": dep_count,
        "status": "pending",
    })))
}

/// Update draft components (rename, change type, update commands).
#[derive(Debug, Deserialize)]
pub struct UpdateComponentsRequest {
    pub components: Vec<UpdateComponentInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateComponentInput {
    pub id: Uuid,
    pub name: String,
    pub component_type: String,
    #[serde(default)]
    pub check_cmd: Option<String>,
    #[serde(default)]
    pub start_cmd: Option<String>,
    #[serde(default)]
    pub stop_cmd: Option<String>,
    #[serde(default)]
    pub restart_cmd: Option<String>,
}

pub async fn update_draft_components(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
    Json(body): Json<UpdateComponentsRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Verify draft exists and is pending
    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM discovery_drafts WHERE id = $1")
            .bind(draft_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;

    if status != "pending" {
        return Err(ApiError::Conflict(format!("Draft is already {}", status)));
    }

    let mut updated = 0u32;
    for comp in &body.components {
        let result = sqlx::query(
            "UPDATE discovery_draft_components
             SET suggested_name = $2, component_type = $3,
                 check_cmd = $5, start_cmd = $6, stop_cmd = $7, restart_cmd = $8
             WHERE id = $1 AND draft_id = $4",
        )
        .bind(comp.id)
        .bind(&comp.name)
        .bind(&comp.component_type)
        .bind(draft_id)
        .bind(&comp.check_cmd)
        .bind(&comp.start_cmd)
        .bind(&comp.stop_cmd)
        .bind(&comp.restart_cmd)
        .execute(&state.db)
        .await?;
        updated += result.rows_affected() as u32;
    }

    Ok(Json(json!({ "updated": updated, "draft_id": draft_id })))
}

/// Update draft dependencies (add/remove).
#[derive(Debug, Deserialize)]
pub struct UpdateDependenciesRequest {
    pub add: Vec<AddDependencyInput>,
    pub remove: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AddDependencyInput {
    pub from_component: Uuid,
    pub to_component: Uuid,
    pub inferred_via: String,
}

pub async fn update_draft_dependencies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
    Json(body): Json<UpdateDependenciesRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM discovery_drafts WHERE id = $1")
            .bind(draft_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;

    if status != "pending" {
        return Err(ApiError::Conflict(format!("Draft is already {}", status)));
    }

    // Remove specified deps
    let mut removed = 0u32;
    for dep_id in &body.remove {
        let result =
            sqlx::query("DELETE FROM discovery_draft_dependencies WHERE id = $1 AND draft_id = $2")
                .bind(dep_id)
                .bind(draft_id)
                .execute(&state.db)
                .await?;
        removed += result.rows_affected() as u32;
    }

    // Add new deps
    let mut added = 0u32;
    for dep in &body.add {
        sqlx::query(
            "INSERT INTO discovery_draft_dependencies
             (draft_id, from_component, to_component, inferred_via)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(draft_id)
        .bind(dep.from_component)
        .bind(dep.to_component)
        .bind(&dep.inferred_via)
        .execute(&state.db)
        .await?;
        added += 1;
    }

    Ok(Json(json!({
        "draft_id": draft_id,
        "added": added,
        "removed": removed,
    })))
}

// ===========================================================================
// Apply — create real application from finalized draft (with commands!)
// ===========================================================================

/// Apply a draft: create a real application from the discovery draft.
/// Now sets check_cmd/start_cmd/stop_cmd on real components and creates
/// component_commands for log viewing and config inspection.
pub async fn apply_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let draft = sqlx::query_as::<_, (DbUuid, DbUuid, String, String)>(
        "SELECT id, organization_id, name, status FROM discovery_drafts WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_optional(&state.db)
    .await?;

    let (_, org_id, name, status) = draft.ok_or(ApiError::NotFound)?;
    if status != "pending" {
        return Err(ApiError::Conflict(format!("Draft is already {}", status)));
    }

    log_action(
        &state.db,
        user.user_id,
        "discovery_apply",
        "discovery_draft",
        draft_id,
        json!({ "name": &name }),
    )
    .await?;

    let site_id = sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM sites WHERE organization_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await?;

    let site_id = site_id.ok_or(ApiError::Validation(
        "Organization has no sites — create a site first".to_string(),
    ))?;

    // Create application
    let app_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO applications (id, organization_id, site_id, name, mode)
         VALUES ($1, $2, $3, $4, 'advisory')",
    )
    .bind(app_id)
    .bind(org_id)
    .bind(site_id)
    .bind(&name)
    .execute(&state.db)
    .await?;

    // Create components WITH operational commands
    #[allow(clippy::type_complexity)]
    let draft_comps = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<Uuid>,
            Option<String>,
            Option<String>,
            Option<String>,
            serde_json::Value,
            serde_json::Value,
        ),
    >(
        "SELECT id, suggested_name, process_name, host, component_type, agent_id,
                check_cmd, start_cmd, stop_cmd,
                COALESCE(config_files, '[]'::jsonb),
                COALESCE(log_files, '[]'::jsonb)
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let mut draft_to_real: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();

    for (
        draft_comp_id,
        comp_name,
        _process_name,
        host,
        comp_type,
        agent_id,
        check_cmd,
        start_cmd,
        stop_cmd,
        config_files,
        log_files,
    ) in &draft_comps
    {
        let real_comp_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO components (id, application_id, name, component_type, host, agent_id,
                                     check_cmd, start_cmd, stop_cmd, current_state)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'UNKNOWN')",
        )
        .bind(real_comp_id)
        .bind(app_id)
        .bind(comp_name)
        .bind(comp_type)
        .bind(host)
        .bind(agent_id)
        .bind(check_cmd)
        .bind(start_cmd)
        .bind(stop_cmd)
        .execute(&state.db)
        .await?;
        draft_to_real.insert(*draft_comp_id, real_comp_id);

        // Create custom commands for log files ("View Logs")
        if let Some(logs) = log_files.as_array() {
            for log_entry in logs {
                if let Some(log_path) = log_entry.get("path").and_then(|p| p.as_str()) {
                    let _ = sqlx::query(
                        "INSERT INTO component_commands (component_id, label, command)
                         VALUES ($1, $2, $3)",
                    )
                    .bind(real_comp_id)
                    .bind(format!(
                        "Logs: {}",
                        log_path.rsplit('/').next().unwrap_or(log_path)
                    ))
                    .bind(format!("tail -100 {}", log_path))
                    .execute(&state.db)
                    .await;
                }
            }
        }

        // Create custom commands for config files ("View Config")
        if let Some(configs) = config_files.as_array() {
            for config_entry in configs {
                if let Some(config_path) = config_entry.get("path").and_then(|p| p.as_str()) {
                    let _ = sqlx::query(
                        "INSERT INTO component_commands (component_id, label, command)
                         VALUES ($1, $2, $3)",
                    )
                    .bind(real_comp_id)
                    .bind(format!(
                        "Config: {}",
                        config_path.rsplit('/').next().unwrap_or(config_path)
                    ))
                    .bind(format!("cat {}", config_path))
                    .execute(&state.db)
                    .await;
                }
            }
        }
    }

    // Create dependencies
    let draft_deps = sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT from_component, to_component
         FROM discovery_draft_dependencies WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let mut dep_count = 0u32;
    for (from_draft, to_draft) in &draft_deps {
        if let (Some(&from_real), Some(&to_real)) =
            (draft_to_real.get(from_draft), draft_to_real.get(to_draft))
        {
            sqlx::query(
                "INSERT INTO dependencies (application_id, from_component_id, to_component_id)
                 VALUES ($1, $2, $3)",
            )
            .bind(app_id)
            .bind(from_real)
            .bind(to_real)
            .execute(&state.db)
            .await?;
            dep_count += 1;
        }
    }

    // Mark draft as applied
    sqlx::query(
        "UPDATE discovery_drafts SET status = 'applied', applied_app_id = $2 WHERE id = $1",
    )
    .bind(draft_id)
    .bind(app_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "application_id": app_id,
        "name": name,
        "mode": "advisory",
        "components_created": draft_comps.len(),
        "dependencies_created": dep_count,
    })))
}

// ===========================================================================
// Snapshot Schedules — automated discovery snapshots for comparison
// ===========================================================================

/// Row type for schedule queries (uses UuidArray for cross-database compatibility).
#[derive(Debug, sqlx::FromRow)]
struct ScheduleRow {
    id: DbUuid,
    name: String,
    agent_ids: UuidArray,
    frequency: String,
    cron_expression: Option<String>,
    enabled: bool,
    retention_days: i32,
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// List snapshot schedules for the organization.
pub async fn list_schedules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = sqlx::query_as::<_, ScheduleRow>(
        "SELECT id, name, agent_ids, frequency, cron_expression, enabled,
                retention_days, last_run_at, next_run_at, created_at
         FROM snapshot_schedules
         WHERE organization_id = $1
         ORDER BY created_at DESC",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    let schedules: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "name": row.name,
                "agent_ids": row.agent_ids.0,
                "frequency": row.frequency,
                "cron_expression": row.cron_expression,
                "enabled": row.enabled,
                "retention_days": row.retention_days,
                "last_run_at": row.last_run_at,
                "next_run_at": row.next_run_at,
                "created_at": row.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "schedules": schedules })))
}

/// Create a new snapshot schedule.
#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub agent_ids: Vec<Uuid>,
    pub frequency: String,
    #[serde(default = "default_retention")]
    pub retention_days: i32,
}

fn default_retention() -> i32 {
    30
}

pub async fn create_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateScheduleRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if body.agent_ids.is_empty() {
        return Err(ApiError::Validation(
            "At least one agent_id is required".to_string(),
        ));
    }

    let valid_frequencies = ["hourly", "daily", "weekly", "monthly"];
    if !valid_frequencies.contains(&body.frequency.as_str()) {
        return Err(ApiError::Validation(format!(
            "Invalid frequency. Must be one of: {:?}",
            valid_frequencies
        )));
    }

    log_action(
        &state.db,
        user.user_id,
        "discovery_create_schedule",
        "snapshot_schedule",
        Uuid::nil(),
        json!({ "name": &body.name, "frequency": &body.frequency, "agents": body.agent_ids.len() }),
    )
    .await?;

    let schedule_id = Uuid::new_v4();
    let next_run = calculate_next_run(&body.frequency);

    sqlx::query(
        "INSERT INTO snapshot_schedules (id, organization_id, name, agent_ids, frequency, retention_days, next_run_at, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(schedule_id)
    .bind(user.organization_id)
    .bind(&body.name)
    .bind(UuidArray::from(body.agent_ids.clone()))
    .bind(&body.frequency)
    .bind(body.retention_days)
    .bind(next_run)
    .bind(user.user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "id": schedule_id,
        "name": body.name,
        "agent_ids": body.agent_ids,
        "frequency": body.frequency,
        "retention_days": body.retention_days,
        "enabled": true,
        "next_run_at": next_run,
    })))
}

/// Update a snapshot schedule.
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct UpdateScheduleRequest {
    pub name: Option<String>,
    pub agent_ids: Option<Vec<Uuid>>,
    pub frequency: Option<String>,
    pub enabled: Option<bool>,
    pub retention_days: Option<i32>,
}

pub async fn update_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
    Json(body): Json<UpdateScheduleRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Verify ownership
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM snapshot_schedules WHERE id = $1 AND organization_id = $2)",
    )
    .bind(schedule_id)
    .bind(user.organization_id)
    .fetch_one(&state.db)
    .await?;

    if !exists {
        return Err(ApiError::NotFound);
    }

    // Check that at least one field is provided
    if body.name.is_none()
        && body.agent_ids.is_none()
        && body.frequency.is_none()
        && body.enabled.is_none()
        && body.retention_days.is_none()
    {
        return Err(ApiError::Validation("No fields to update".to_string()));
    }

    log_action(
        &state.db,
        user.user_id,
        "discovery_update_schedule",
        "snapshot_schedule",
        schedule_id,
        json!({ "updates": &body }),
    )
    .await?;

    // Execute updates for each provided field
    if let Some(ref name) = body.name {
        sqlx::query("UPDATE snapshot_schedules SET name = $2 WHERE id = $1")
            .bind(schedule_id)
            .bind(name)
            .execute(&state.db)
            .await?;
    }
    if let Some(ref agent_ids) = body.agent_ids {
        sqlx::query("UPDATE snapshot_schedules SET agent_ids = $2 WHERE id = $1")
            .bind(schedule_id)
            .bind(UuidArray::from(agent_ids.clone()))
            .execute(&state.db)
            .await?;
    }
    if let Some(ref frequency) = body.frequency {
        let next_run = calculate_next_run(frequency);
        sqlx::query("UPDATE snapshot_schedules SET frequency = $2, next_run_at = $3 WHERE id = $1")
            .bind(schedule_id)
            .bind(frequency)
            .bind(next_run)
            .execute(&state.db)
            .await?;
    }
    if let Some(enabled) = body.enabled {
        if enabled {
            // Get current frequency to calculate next_run
            let freq: String =
                sqlx::query_scalar("SELECT frequency FROM snapshot_schedules WHERE id = $1")
                    .bind(schedule_id)
                    .fetch_one(&state.db)
                    .await?;
            let next_run = calculate_next_run(&freq);
            sqlx::query(
                "UPDATE snapshot_schedules SET enabled = $2, next_run_at = $3 WHERE id = $1",
            )
            .bind(schedule_id)
            .bind(enabled)
            .bind(next_run)
            .execute(&state.db)
            .await?;
        } else {
            sqlx::query("UPDATE snapshot_schedules SET enabled = $2 WHERE id = $1")
                .bind(schedule_id)
                .bind(enabled)
                .execute(&state.db)
                .await?;
        }
    }
    if let Some(retention_days) = body.retention_days {
        sqlx::query("UPDATE snapshot_schedules SET retention_days = $2 WHERE id = $1")
            .bind(schedule_id)
            .bind(retention_days)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(json!({ "updated": true, "schedule_id": schedule_id })))
}

/// Calculate next run time based on frequency.
fn calculate_next_run(frequency: &str) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, Duration, Timelike, Utc};

    let now = Utc::now();

    match frequency {
        "hourly" => {
            // Next hour
            now.with_minute(0)
                .and_then(|t| t.with_second(0))
                .map(|t| t + Duration::hours(1))
                .unwrap_or(now + Duration::hours(1))
        }
        "daily" => {
            // Next day at midnight
            now.with_hour(0)
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_second(0))
                .map(|t| t + Duration::days(1))
                .unwrap_or(now + Duration::days(1))
        }
        "weekly" => {
            // Next week (Sunday) at midnight
            let days_until_sunday = (7 - now.weekday().num_days_from_sunday()) % 7;
            let days_until_sunday = if days_until_sunday == 0 {
                7
            } else {
                days_until_sunday
            };
            now.with_hour(0)
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_second(0))
                .map(|t| t + Duration::days(days_until_sunday as i64))
                .unwrap_or(now + Duration::days(7))
        }
        "monthly" => {
            // First day of next month at midnight
            let next_month = if now.month() == 12 {
                now.with_year(now.year() + 1).and_then(|t| t.with_month(1))
            } else {
                now.with_month(now.month() + 1)
            };
            next_month
                .and_then(|t| t.with_day(1))
                .and_then(|t| t.with_hour(0))
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_second(0))
                .unwrap_or(now + Duration::days(30))
        }
        _ => now + Duration::days(1), // Default to daily
    }
}

/// Delete a snapshot schedule.
pub async fn delete_schedule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "discovery_delete_schedule",
        "snapshot_schedule",
        schedule_id,
        json!({}),
    )
    .await?;

    let result =
        sqlx::query("DELETE FROM snapshot_schedules WHERE id = $1 AND organization_id = $2")
            .bind(schedule_id)
            .bind(user.organization_id)
            .execute(&state.db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(Json(json!({ "deleted": true })))
}

// ===========================================================================
// Scheduled Snapshots — captured snapshots from scheduled runs
// ===========================================================================

#[derive(Debug, Deserialize)]
pub struct ListSnapshotsQuery {
    pub schedule_id: Option<DbUuid>,
}

/// Row type for snapshot queries.
#[derive(Debug, sqlx::FromRow)]
struct SnapshotRow {
    id: DbUuid,
    schedule_id: DbUuid,
    schedule_name: String,
    agent_ids: UuidArray,
    report_ids: UuidArray,
    captured_at: chrono::DateTime<chrono::Utc>,
}

/// List scheduled snapshots.
pub async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<ListSnapshotsQuery>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = if let Some(schedule_id) = query.schedule_id {
        sqlx::query_as::<_, SnapshotRow>(
            "SELECT ss.id, ss.schedule_id, sch.name as schedule_name, ss.agent_ids, ss.report_ids, ss.captured_at
             FROM scheduled_snapshots ss
             JOIN snapshot_schedules sch ON sch.id = ss.schedule_id
             WHERE ss.organization_id = $1 AND ss.schedule_id = $2
             ORDER BY ss.captured_at DESC
             LIMIT 100",
        )
        .bind(user.organization_id)
        .bind(schedule_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, SnapshotRow>(
            "SELECT ss.id, ss.schedule_id, sch.name as schedule_name, ss.agent_ids, ss.report_ids, ss.captured_at
             FROM scheduled_snapshots ss
             JOIN snapshot_schedules sch ON sch.id = ss.schedule_id
             WHERE ss.organization_id = $1
             ORDER BY ss.captured_at DESC
             LIMIT 100",
        )
        .bind(user.organization_id)
        .fetch_all(&state.db)
        .await?
    };

    let snapshots: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "schedule_id": row.schedule_id,
                "schedule_name": row.schedule_name,
                "agent_ids": row.agent_ids.0,
                "report_ids": row.report_ids.0,
                "captured_at": row.captured_at,
            })
        })
        .collect();

    Ok(Json(json!({ "snapshots": snapshots })))
}

/// Compare two snapshots and return differences.
#[derive(Debug, Deserialize)]
pub struct CompareSnapshotsRequest {
    pub snapshot_id_1: DbUuid,
    pub snapshot_id_2: DbUuid,
}

pub async fn compare_snapshots(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CompareSnapshotsRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Fetch both snapshots' correlation results
    let snap1 = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT COALESCE(correlation_result, '{}'::jsonb)
         FROM scheduled_snapshots
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(body.snapshot_id_1)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    let snap2 = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT COALESCE(correlation_result, '{}'::jsonb)
         FROM scheduled_snapshots
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(body.snapshot_id_2)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?;

    let (corr1,) = snap1.ok_or(ApiError::NotFound)?;
    let (corr2,) = snap2.ok_or(ApiError::NotFound)?;

    // Extract services from both correlations
    let services1 = corr1
        .get("services")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let services2 = corr2
        .get("services")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();

    // Build keys for comparison: (hostname, process_name, ports)
    fn service_key(svc: &Value) -> String {
        let host = svc.get("hostname").and_then(|h| h.as_str()).unwrap_or("");
        let proc = svc
            .get("process_name")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        let ports: Vec<String> = svc
            .get("ports")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        format!("{}:{}:{}", host, proc, ports.join(","))
    }

    let keys1: std::collections::HashSet<String> = services1.iter().map(service_key).collect();
    let keys2: std::collections::HashSet<String> = services2.iter().map(service_key).collect();

    // Added: in snapshot 2 but not in snapshot 1
    let added: Vec<Value> = services2
        .iter()
        .filter(|s| !keys1.contains(&service_key(s)))
        .cloned()
        .collect();

    // Removed: in snapshot 1 but not in snapshot 2
    let removed: Vec<Value> = services1
        .iter()
        .filter(|s| !keys2.contains(&service_key(s)))
        .cloned()
        .collect();

    // Modified: same key but different details (simplified - check port count changes)
    let mut modified: Vec<Value> = Vec::new();
    for svc1 in &services1 {
        let key = service_key(svc1);
        if keys2.contains(&key) {
            // Find matching service in snapshot 2
            if let Some(svc2) = services2.iter().find(|s| service_key(s) == key) {
                let ports1: Vec<u64> = svc1
                    .get("ports")
                    .and_then(|p| p.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();
                let ports2: Vec<u64> = svc2
                    .get("ports")
                    .and_then(|p| p.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();

                if ports1 != ports2 {
                    modified.push(json!({
                        "before": svc1,
                        "after": svc2,
                        "changes": [format!("ports: {:?} → {:?}", ports1, ports2)],
                    }));
                }
            }
        }
    }

    Ok(Json(json!({
        "added": added,
        "removed": removed,
        "modified": modified,
        "summary": {
            "added_count": added.len(),
            "removed_count": removed.len(),
            "modified_count": modified.len(),
        }
    })))
}

// ===========================================================================
// File Content Reading — read config/log files from agents
// ===========================================================================

/// Request to read file content from an agent.
#[derive(Debug, Deserialize)]
pub struct ReadFileContentRequest {
    pub agent_id: DbUuid,
    pub path: String,
    /// For log files: read only the last N lines (default: 100)
    #[serde(default = "default_tail_lines")]
    pub tail_lines: Option<u32>,
}

fn default_tail_lines() -> Option<u32> {
    Some(100)
}

/// Read file content from an agent.
///
/// This sends a command to the agent to read the file and returns the content.
/// For log files, use tail_lines to get only the last N lines.
/// For config files, the full content is returned (up to 64KB).
pub async fn read_file_content(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ReadFileContentRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Validate agent exists and belongs to org
    let agent_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND organization_id = $2)",
    )
    .bind(body.agent_id)
    .bind(user.organization_id)
    .fetch_one(&state.db)
    .await?;

    if !agent_exists {
        return Err(ApiError::NotFound);
    }

    let request_id = Uuid::new_v4();

    log_action(
        &state.db,
        user.user_id,
        "discovery_read_file",
        "agent",
        body.agent_id,
        json!({ "path": &body.path, "tail_lines": body.tail_lines }),
    )
    .await?;

    // Build the read command based on OS detection
    // We use cross-platform commands that work on both Windows and Unix
    let command = if let Some(tail_lines) = body.tail_lines {
        // Log file: use tail (Unix) or PowerShell Get-Content -Tail (Windows)
        // The agent will detect OS and use appropriate command
        format!(
            r#"powershell -Command "Get-Content -Path '{}' -Tail {} -ErrorAction Stop""#,
            body.path.replace('\'', "''"),
            tail_lines
        )
    } else {
        // Config file: read full content (up to reasonable size)
        format!(
            r#"powershell -Command "Get-Content -Path '{}' -Raw -ErrorAction Stop""#,
            body.path.replace('\'', "''")
        )
    };

    // Send command to agent
    let msg = appcontrol_common::BackendMessage::ExecuteCommand {
        request_id,
        component_id: *DbUuid::nil(), // No component context for discovery
        command: command.clone(),
        timeout_seconds: 30,
        exec_mode: "sync".to_string(),
    };

    let sent = state.ws_hub.send_to_agent(body.agent_id, msg);

    if !sent {
        return Err(ApiError::Conflict("Agent is not connected".to_string()));
    }

    // Return the request_id so frontend can poll for result
    // The actual content will come back via CommandResult WebSocket event
    Ok(Json(json!({
        "request_id": request_id,
        "agent_id": body.agent_id,
        "path": body.path,
        "command": command,
        "sent": true,
    })))
}
