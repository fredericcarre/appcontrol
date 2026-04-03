//! Discovery scan triggers.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

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

    let agent_ids = crate::repository::discovery_queries::list_active_agent_ids(
        &state.db,
        *user.organization_id,
    )
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
