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
    extract::{Extension, Path, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
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

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, chrono::DateTime<chrono::Utc>)>(
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

    let agent_ids = sqlx::query_scalar::<_, Uuid>(
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
    let mut reports: Vec<(Uuid, String, serde_json::Value)> = Vec::new();
    for agent_id in &body.agent_ids {
        let row = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
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
        agent_hostnames.insert(*agent_id, hostname.clone());
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
                agent_ips.insert(*agent_id, ip_list);
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

            services.push(json!({
                "agent_id": agent_id,
                "hostname": hostname,
                "process_name": proc_name,
                "ports": port_list,
                "port_details": ports,
                "suggested_name": format!("{}@{}", proc_name, hostname),
                "component_type": guess_component_type(proc_name, &port_list),
                "command_suggestion": command_suggestion,
                "config_files": config_files,
                "log_files": log_files,
                "matched_service": matched_service,
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
    // Build connections: match outbound connections to services
    // -----------------------------------------------------------------------
    let mut resolved_deps: Vec<Value> = Vec::new();
    let mut unresolved_conns: Vec<Value> = Vec::new();
    let mut seen_deps: std::collections::HashSet<(usize, usize, String)> =
        std::collections::HashSet::new();

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

                // Find target service
                let target_idx = listen_index.get(&(remote_addr.to_string(), remote_port));

                if let Some(&to_idx) = target_idx {
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

    Ok(Json(json!({
        "agents_analyzed": reports.len(),
        "services": services,
        "dependencies": resolved_deps,
        "unresolved_connections": unresolved_conns,
        "scheduled_jobs": scheduled_jobs,
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

/// Heuristic: guess component type from process name and ports.
fn guess_component_type(process_name: &str, ports: &[u16]) -> &'static str {
    let name = process_name.to_lowercase();

    // Databases
    if name.contains("postgres") || name.contains("pgbouncer") {
        return "database";
    }
    if name.contains("mysql") || name.contains("mariadb") {
        return "database";
    }
    if name.contains("mongo") {
        return "database";
    }
    if name.contains("oracle") || name.contains("tnslsnr") {
        return "database";
    }
    if name.contains("redis") || name.contains("memcache") {
        return "cache";
    }
    if name.contains("elasticsearch") || name.contains("solr") {
        return "search";
    }

    // Message queues
    if name.contains("kafka")
        || name.contains("rabbit")
        || name.contains("activemq")
        || name.contains("mosquitto")
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
    {
        return "proxy";
    }

    // Known ports
    for port in ports {
        match port {
            5432 | 3306 | 1521 | 1433 | 27017 => return "database",
            6379 | 11211 => return "cache",
            9092 | 5672 | 61616 => return "queue",
            9200 | 8983 => return "search",
            80 | 443 | 8080 | 8443 => return "web",
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

    let rows = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
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

    let draft = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
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

    let deps = sqlx::query_as::<_, (Uuid, Uuid, Uuid, String)>(
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

    let org_id = sqlx::query_scalar::<_, Uuid>("SELECT organization_id FROM users WHERE id = $1")
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
        .bind(&comp.listening_ports)
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

    let draft = sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
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

    let site_id = sqlx::query_scalar::<_, Uuid>(
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
    let draft_deps = sqlx::query_as::<_, (Uuid, Uuid)>(
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
