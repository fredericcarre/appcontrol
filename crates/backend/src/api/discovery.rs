//! Discovery API: passive topology scanning and DAG inference.
//!
//! Endpoints:
//! - GET  /api/v1/discovery/reports         — list discovery reports from agents
//! - POST /api/v1/discovery/trigger/:agent_id — request a discovery scan from an agent
//! - GET  /api/v1/discovery/drafts          — list inferred application drafts
//! - GET  /api/v1/discovery/drafts/:id      — get draft details with components + deps
//! - POST /api/v1/discovery/drafts/:id/apply — create an application from a draft
//! - POST /api/v1/discovery/infer           — run inference on recent reports

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

    // Get all active agents for this org
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

/// List discovery drafts (inferred application topologies).
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
         ORDER BY created_at DESC
         LIMIT 50",
    )
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

/// Get full draft details: components + inferred dependencies.
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

    let components = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, String)>(
        "SELECT id, suggested_name, process_name, host, component_type
         FROM discovery_draft_components WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let deps = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT from_component, to_component, inferred_via
         FROM discovery_draft_dependencies WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

    let comp_json: Vec<Value> = components
        .iter()
        .map(|(cid, name, proc, host, ctype)| {
            json!({
                "id": cid,
                "name": name,
                "process_name": proc,
                "host": host,
                "component_type": ctype,
            })
        })
        .collect();

    let dep_json: Vec<Value> = deps
        .iter()
        .map(|(from, to, via)| {
            json!({
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

/// Run inference on recent discovery reports to create a draft.
/// Groups processes by listening port, matches outbound connections to listeners,
/// and creates a draft application with inferred dependencies.
///
/// ## Inference Algorithm
///
/// 1. For each agent, extract listeners on application ports (1024–49151)
///    and create a draft component per unique (process_name, port) pair.
/// 2. For each agent, extract ESTABLISHED outbound connections.
/// 3. Cross-correlate: if agent A has a connection to remote_addr:remote_port
///    and agent B listens on that port, infer a dependency A→B.
/// 4. Also match connections within the same host (process X → process Y).
#[derive(Debug, Deserialize)]
pub struct InferRequest {
    pub name: String,
    pub agent_ids: Vec<Uuid>,
}

/// In-memory representation of a component during inference.
struct InferredComponent {
    comp_id: Uuid,
    agent_id: Uuid,
    hostname: String,
    process_name: String,
    port: u16,
    address: String,
}

pub async fn infer(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<InferRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get org_id from user
    let org_id = sqlx::query_scalar::<_, Uuid>("SELECT organization_id FROM users WHERE id = $1")
        .bind(user.user_id)
        .fetch_one(&state.db)
        .await?;

    log_action(
        &state.db,
        user.user_id,
        "discovery_infer",
        "discovery",
        Uuid::nil(),
        json!({ "name": &body.name, "agent_ids": &body.agent_ids }),
    )
    .await?;

    // Get the most recent report for each specified agent
    let mut agent_reports: Vec<(Uuid, String, serde_json::Value)> = Vec::new();

    for agent_id in &body.agent_ids {
        let report = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
            "SELECT agent_id, hostname, report FROM discovery_reports
             WHERE agent_id = $1
             ORDER BY scanned_at DESC LIMIT 1",
        )
        .bind(agent_id)
        .fetch_optional(&state.db)
        .await?;

        if let Some(r) = report {
            agent_reports.push(r);
        }
    }

    if agent_reports.is_empty() {
        return Err(ApiError::Validation(
            "No discovery reports found for the specified agents".to_string(),
        ));
    }

    // Create the draft
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

    // -----------------------------------------------------------------------
    // Phase 1: Create components from listeners
    // -----------------------------------------------------------------------
    let mut components: Vec<InferredComponent> = Vec::new();

    // Also collect agent IP addresses from the agents table for connection matching
    let mut agent_ips: std::collections::HashMap<Uuid, Vec<String>> =
        std::collections::HashMap::new();
    for (agent_id, _, _) in &agent_reports {
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

    for (agent_id, hostname, report) in &agent_reports {
        let listeners = report.get("listeners").and_then(|l| l.as_array());
        if let Some(listeners) = listeners {
            for listener in listeners {
                let port = listener.get("port").and_then(|p| p.as_u64()).unwrap_or(0);
                let proc_name = listener
                    .get("process_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                let host = listener
                    .get("address")
                    .and_then(|a| a.as_str())
                    .unwrap_or("0.0.0.0");

                // Skip ephemeral / system ports typically not application services
                if !(1024..=49151).contains(&port) {
                    continue;
                }

                let comp_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO discovery_draft_components
                     (id, draft_id, agent_id, suggested_name, process_name, host, listening_ports, component_type)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, 'service')",
                )
                .bind(comp_id)
                .bind(draft_id)
                .bind(agent_id)
                .bind(format!("{}-{}", proc_name, port))
                .bind(proc_name)
                .bind(host)
                .bind([port as i32])
                .execute(&state.db)
                .await?;

                components.push(InferredComponent {
                    comp_id,
                    agent_id: *agent_id,
                    hostname: hostname.clone(),
                    process_name: proc_name.to_string(),
                    port: port as u16,
                    address: host.to_string(),
                });
            }
        }
    }

    // -----------------------------------------------------------------------
    // Phase 2: Infer dependencies from outbound connections
    // -----------------------------------------------------------------------
    // Build a lookup: (hostname_or_ip, port) → component_id
    // A listener on 0.0.0.0 means it listens on ALL IPs of that host.
    let mut listen_index: std::collections::HashMap<(String, u16), Uuid> =
        std::collections::HashMap::new();

    for comp in &components {
        // Index by hostname:port
        listen_index.insert(
            (comp.hostname.clone().to_lowercase(), comp.port),
            comp.comp_id,
        );

        // Index by each known IP of this agent
        if let Some(ips) = agent_ips.get(&comp.agent_id) {
            for ip in ips {
                listen_index.insert((ip.clone(), comp.port), comp.comp_id);
            }
        }

        // Also index by listen address if it's specific (not 0.0.0.0 or ::)
        if comp.address != "0.0.0.0" && comp.address != "::" {
            listen_index.insert((comp.address.clone(), comp.port), comp.comp_id);
        }
    }

    let mut dep_count = 0u32;
    // Track created deps to avoid duplicates
    let mut created_deps: std::collections::HashSet<(Uuid, Uuid)> =
        std::collections::HashSet::new();

    for (agent_id, _hostname, report) in &agent_reports {
        let connections = report.get("connections").and_then(|c| c.as_array());
        if let Some(connections) = connections {
            for conn in connections {
                let remote_addr = conn
                    .get("remote_addr")
                    .and_then(|a| a.as_str())
                    .unwrap_or("");
                let remote_port = conn
                    .get("remote_port")
                    .and_then(|p| p.as_u64())
                    .unwrap_or(0) as u16;
                let local_port =
                    conn.get("local_port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                let conn_proc = conn
                    .get("process_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                if remote_addr.is_empty() || remote_port == 0 {
                    continue;
                }

                // Find the target component (the service being connected TO)
                let target = listen_index.get(&(remote_addr.to_string(), remote_port));

                if let Some(&target_comp_id) = target {
                    // Find the source component: the process making the connection.
                    // Match by agent_id + process_name, or by agent_id + local_port.
                    let source_comp_id = components
                        .iter()
                        .find(|c| {
                            c.agent_id == *agent_id
                                && (!conn_proc.is_empty() && c.process_name == conn_proc)
                        })
                        .or_else(|| {
                            // Fallback: source is any component on this agent listening on local_port
                            components
                                .iter()
                                .find(|c| c.agent_id == *agent_id && c.port == local_port)
                        })
                        .map(|c| c.comp_id);

                    if let Some(source_id) = source_comp_id {
                        // Don't create self-dependencies
                        if source_id != target_comp_id
                            && !created_deps.contains(&(source_id, target_comp_id))
                        {
                            sqlx::query(
                                "INSERT INTO discovery_draft_dependencies
                                 (draft_id, from_component, to_component, inferred_via)
                                 VALUES ($1, $2, $3, 'tcp_connection')",
                            )
                            .bind(draft_id)
                            .bind(source_id)
                            .bind(target_comp_id)
                            .execute(&state.db)
                            .await?;

                            created_deps.insert((source_id, target_comp_id));
                            dep_count += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(Json(json!({
        "draft_id": draft_id,
        "name": body.name,
        "components_inferred": components.len(),
        "dependencies_inferred": dep_count,
        "status": "pending",
    })))
}

/// Apply a draft: create a real application from the discovery draft.
pub async fn apply_draft(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Check draft exists and is pending
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

    // Get the default site for this org
    let site_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM sites WHERE organization_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await?;

    let site_id = site_id.ok_or(ApiError::Validation(
        "Organization has no sites — create a site first".to_string(),
    ))?;

    // Create the application
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

    // Create components from draft
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

    // Create dependencies from draft
    let draft_deps = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT from_component, to_component
         FROM discovery_draft_dependencies WHERE draft_id = $1",
    )
    .bind(draft_id)
    .fetch_all(&state.db)
    .await?;

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
        "dependencies_created": draft_deps.len(),
    })))
}
