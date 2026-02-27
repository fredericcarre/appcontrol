//! Discovery API: passive topology scanning and multi-step DAG creation.
//!
//! ## Workflow (not magic — user validates at each step)
//!
//! 1. **Collect**: `POST /trigger-all` or `/trigger/:agent_id` → agents scan and send reports
//! 2. **Correlate**: `POST /correlate` → backend analyzes reports, returns structured analysis
//!    (listeners grouped by service, cross-host connections identified) for user review
//! 3. **Create draft**: `POST /drafts` → user sends validated components + dependencies
//! 4. **Edit draft**: `PUT /drafts/:id/components` and `PUT /drafts/:id/dependencies`
//! 5. **Apply draft**: `POST /drafts/:id/apply` → creates a real application with components + DAG
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
/// cross-host connections mapped to potential dependencies, and unresolved
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

        // Create one service per unique process on this host
        for (proc_name, ports) in &proc_ports {
            let idx = services.len();
            let port_list: Vec<u16> = ports
                .iter()
                .filter_map(|p| p.get("port").and_then(|v| v.as_u64()).map(|v| v as u16))
                .collect();

            services.push(json!({
                "agent_id": agent_id,
                "hostname": hostname,
                "process_name": proc_name,
                "ports": port_list,
                "port_details": ports,
                "suggested_name": format!("{}@{}", proc_name, hostname),
                "component_type": guess_component_type(proc_name, &port_list),
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
    let mut seen_deps: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    for (agent_id, hostname, report) in &reports {
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
                    // Find source service (the process making the connection on this agent)
                    let from_idx = if !conn_proc.is_empty() {
                        services.iter().position(|s| {
                            s.get("agent_id")
                                .and_then(|a| a.as_str())
                                .map(|a| a == agent_id.to_string())
                                .unwrap_or(false)
                                && s.get("process_name")
                                    .and_then(|n| n.as_str())
                                    .map(|n| n == conn_proc)
                                    .unwrap_or(false)
                        })
                    } else {
                        None
                    };

                    if let Some(from_idx) = from_idx {
                        if from_idx != to_idx && !seen_deps.contains(&(from_idx, to_idx)) {
                            seen_deps.insert((from_idx, to_idx));
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
                        // Source process not a known service — still a useful dep
                        if !seen_deps.contains(&(usize::MAX, to_idx)) {
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
                    }
                } else {
                    // Connection to unknown host — not in our agent set
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

    Ok(Json(json!({
        "agents_analyzed": reports.len(),
        "services": services,
        "dependencies": resolved_deps,
        "unresolved_connections": unresolved_conns,
    })))
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

/// Get full draft details: components + dependencies.
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

    let components = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            String,
            serde_json::Value,
        ),
    >(
        "SELECT id, suggested_name, process_name, host, component_type, metadata
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
        .map(|(cid, name, proc, host, ctype, meta)| {
            json!({
                "id": cid,
                "name": name,
                "process_name": proc,
                "host": host,
                "component_type": ctype,
                "metadata": meta,
            })
        })
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

/// Create a draft from user-validated components + dependencies.
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
             (id, draft_id, agent_id, suggested_name, process_name, host, listening_ports, component_type)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(comp_id)
        .bind(draft_id)
        .bind(comp.agent_id)
        .bind(&comp.name)
        .bind(&comp.process_name)
        .bind(&comp.host)
        .bind(&comp.listening_ports)
        .bind(&comp.component_type)
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

/// Update draft components (rename, change type, add/remove).
#[derive(Debug, Deserialize)]
pub struct UpdateComponentsRequest {
    pub components: Vec<UpdateComponentInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateComponentInput {
    pub id: Uuid,
    pub name: String,
    pub component_type: String,
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
             SET suggested_name = $2, component_type = $3
             WHERE id = $1 AND draft_id = $4",
        )
        .bind(comp.id)
        .bind(&comp.name)
        .bind(&comp.component_type)
        .bind(draft_id)
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
// Apply — create real application from finalized draft
// ===========================================================================

/// Apply a draft: create a real application from the discovery draft.
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

    // Create components
    let draft_comps = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, String)>(
        "SELECT id, suggested_name, process_name, host, component_type
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let mut draft_to_real: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();

    for (draft_comp_id, comp_name, _process_name, host, comp_type) in &draft_comps {
        let real_comp_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO components (id, application_id, name, component_type, host, current_state)
             VALUES ($1, $2, $3, $4, $5, 'UNKNOWN')",
        )
        .bind(real_comp_id)
        .bind(app_id)
        .bind(comp_name)
        .bind(comp_type)
        .bind(host)
        .execute(&state.db)
        .await?;
        draft_to_real.insert(*draft_comp_id, real_comp_id);
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
