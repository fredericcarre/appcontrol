use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: Uuid,
    pub hostname: String,
    pub organization_id: Uuid,
    pub gateway_id: Option<Uuid>,
    pub labels: Value,
    pub ip_addresses: Value,
    pub version: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let agents = sqlx::query_as::<_, AgentRow>(
        r#"
        SELECT id, hostname, organization_id, gateway_id, labels, ip_addresses, version, last_heartbeat_at, is_active, created_at
        FROM agents
        WHERE organization_id = $1
        ORDER BY hostname
        "#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await?;

    // Get live connection status from the WebSocket hub
    let connected_agents = state.ws_hub.connected_agent_ids();
    let connected_set: std::collections::HashSet<Uuid> = connected_agents.into_iter().collect();

    // Enrich agents with connection status
    let agents_with_status: Vec<Value> = agents
        .into_iter()
        .map(|a| {
            let connected = connected_set.contains(&a.id);
            json!({
                "id": a.id,
                "hostname": a.hostname,
                "organization_id": a.organization_id,
                "gateway_id": a.gateway_id,
                "labels": a.labels,
                "ip_addresses": a.ip_addresses,
                "version": a.version,
                "last_heartbeat_at": a.last_heartbeat_at,
                "is_active": a.is_active,
                "created_at": a.created_at,
                "connected": connected,
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agents_with_status })))
}

pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let agent = sqlx::query_as::<_, AgentRow>(
        r#"
        SELECT id, hostname, organization_id, gateway_id, labels, ip_addresses, version, last_heartbeat_at, is_active, created_at
        FROM agents
        WHERE id = $1 AND organization_id = $2
        "#,
    )
    .bind(id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(agent)))
}
