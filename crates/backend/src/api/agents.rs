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

    Ok(Json(json!({ "agents": agents })))
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
