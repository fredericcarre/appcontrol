//! Cluster members API — CRUD and batch actions for fan-out cluster members.
//!
//! A member is a first-class instance of a fan-out component: its own agent,
//! optional per-member command overrides, and its own FSM state.
//!
//! Routes:
//! - GET    /components/:id/members                      list members of a component
//! - POST   /components/:id/members                      add a member
//! - GET    /members/:id                                 get one member
//! - PUT    /members/:id                                 update member
//! - DELETE /members/:id                                 remove member
//! - POST   /components/:id/members/actions/start        fan-out start (batch)
//! - POST   /components/:id/members/actions/stop         fan-out stop (batch)

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::{ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::repository::cluster_members::{
    ClusterMember, ClusterMemberWithState, CreateClusterMember, UpdateClusterMember,
};
use crate::AppState;
use appcontrol_common::{BackendMessage, PermissionLevel};

// ============================================================================
// Request/response types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateMemberRequest {
    pub hostname: String,
    pub agent_id: Uuid,
    #[serde(default)]
    pub site_id: Option<Uuid>,
    #[serde(default)]
    pub check_cmd_override: Option<String>,
    #[serde(default)]
    pub start_cmd_override: Option<String>,
    #[serde(default)]
    pub stop_cmd_override: Option<String>,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default)]
    pub env_vars_override: Option<Value>,
    #[serde(default)]
    pub member_order: Option<i32>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub tags: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateMemberRequest {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub site_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub check_cmd_override: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub start_cmd_override: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub stop_cmd_override: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub install_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub env_vars_override: Option<Option<Value>>,
    #[serde(default)]
    pub member_order: Option<i32>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default)]
    pub tags: Option<Value>,
}

/// Deserialize a JSON field that may be absent, null, or a value — mapping to
/// `None` (absent), `Some(None)` (null, explicit clear), or `Some(Some(v))`.
fn deserialize_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

#[derive(Debug, Deserialize)]
pub struct BatchActionRequest {
    /// Optional member subset: if absent, act on all enabled members.
    #[serde(default)]
    pub member_ids: Option<Vec<Uuid>>,
    /// Optional: when true, dispatch in parallel; default false (sequential).
    #[serde(default)]
    pub parallel: bool,
}

// ============================================================================
// Serialization helpers
// ============================================================================

fn member_to_json(m: &ClusterMember) -> Value {
    json!({
        "id": m.id,
        "component_id": m.component_id,
        "hostname": m.hostname,
        "agent_id": m.agent_id,
        "site_id": m.site_id,
        "check_cmd_override": m.check_cmd_override,
        "start_cmd_override": m.start_cmd_override,
        "stop_cmd_override": m.stop_cmd_override,
        "install_path": m.install_path,
        "env_vars_override": m.env_vars_override,
        "member_order": m.member_order,
        "is_enabled": m.is_enabled,
        "tags": m.tags,
        "created_at": m.created_at,
        "updated_at": m.updated_at,
    })
}

fn member_with_state_to_json(m: &ClusterMemberWithState) -> Value {
    let mut v = member_to_json(&m.member);
    if let Value::Object(ref mut obj) = v {
        obj.insert("current_state".to_string(), json!(m.current_state));
        obj.insert("last_check_at".to_string(), json!(m.last_check_at));
        obj.insert(
            "last_check_exit_code".to_string(),
            json!(m.last_check_exit_code),
        );
    }
    v
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/v1/components/:id/members
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let component = state
        .component_repo
        .get_component(component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let members = state
        .cluster_member_repo
        .list_by_component(component_id)
        .await?;

    let result: Vec<Value> = members.iter().map(member_with_state_to_json).collect();

    Ok(Json(json!({ "members": result })))
}

/// POST /api/v1/components/:id/members
pub async fn create_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Json(body): Json<CreateMemberRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let component = state
        .component_repo
        .get_component(component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "create_cluster_member",
        "cluster_member",
        component_id,
        json!({ "hostname": body.hostname, "agent_id": body.agent_id }),
    )
    .await
    .ok();

    let input = CreateClusterMember {
        component_id,
        hostname: body.hostname,
        agent_id: body.agent_id,
        site_id: body.site_id,
        check_cmd_override: body.check_cmd_override,
        start_cmd_override: body.start_cmd_override,
        stop_cmd_override: body.stop_cmd_override,
        install_path: body.install_path,
        env_vars_override: body.env_vars_override,
        member_order: body.member_order.unwrap_or(0),
        is_enabled: body.is_enabled.unwrap_or(true),
        tags: body.tags.unwrap_or(json!([])),
    };

    let member = state.cluster_member_repo.create(input).await?;

    // Push new configuration to the member's agent so it starts scheduling checks.
    crate::websocket::send_config_to_agent(&state, member.agent_id).await;

    Ok((StatusCode::CREATED, Json(member_to_json(&member))))
}

/// GET /api/v1/members/:id
pub async fn get_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let member = state.cluster_member_repo.get(id).await?.ok_or_not_found()?;
    let component = state
        .component_repo
        .get_component(member.component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    Ok(Json(member_to_json(&member)))
}

/// PUT /api/v1/members/:id
pub async fn update_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMemberRequest>,
) -> Result<Json<Value>, ApiError> {
    let existing = state.cluster_member_repo.get(id).await?.ok_or_not_found()?;
    let component = state
        .component_repo
        .get_component(existing.component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "update_cluster_member",
        "cluster_member",
        id,
        json!({}),
    )
    .await
    .ok();

    let input = UpdateClusterMember {
        hostname: body.hostname,
        agent_id: body.agent_id,
        site_id: body.site_id,
        check_cmd_override: body.check_cmd_override,
        start_cmd_override: body.start_cmd_override,
        stop_cmd_override: body.stop_cmd_override,
        install_path: body.install_path,
        env_vars_override: body.env_vars_override,
        member_order: body.member_order,
        is_enabled: body.is_enabled,
        tags: body.tags,
    };

    let updated = state
        .cluster_member_repo
        .update(id, input)
        .await?
        .ok_or_not_found()?;

    // Push updated config to both the previous and new agent if the agent changed.
    if existing.agent_id != updated.agent_id {
        crate::websocket::send_config_to_agent(&state, existing.agent_id).await;
    }
    crate::websocket::send_config_to_agent(&state, updated.agent_id).await;

    Ok(Json(member_to_json(&updated)))
}

/// DELETE /api/v1/members/:id
pub async fn delete_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let existing = state.cluster_member_repo.get(id).await?.ok_or_not_found()?;
    let component = state
        .component_repo
        .get_component(existing.component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_cluster_member",
        "cluster_member",
        id,
        json!({}),
    )
    .await
    .ok();

    let agent_id = existing.agent_id;
    let deleted = state.cluster_member_repo.delete(id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }

    crate::websocket::send_config_to_agent(&state, agent_id).await;

    Ok(Json(json!({ "status": "deleted" })))
}

/// POST /api/v1/components/:id/members/actions/start
/// POST /api/v1/components/:id/members/actions/stop
/// Fan-out dispatch: send a command to each selected member's agent.
pub async fn batch_action(
    state: Arc<AppState>,
    user: AuthUser,
    component_id: Uuid,
    action: &'static str, // "start" or "stop"
    body: BatchActionRequest,
) -> Result<Json<Value>, ApiError> {
    let component = state
        .component_repo
        .get_component(component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    let members_with_state = state
        .cluster_member_repo
        .list_by_component(component_id)
        .await?;

    let selected: Vec<&ClusterMemberWithState> = if let Some(ids) = &body.member_ids {
        members_with_state
            .iter()
            .filter(|m| ids.contains(&m.member.id) && m.member.is_enabled)
            .collect()
    } else {
        members_with_state
            .iter()
            .filter(|m| m.member.is_enabled)
            .collect()
    };

    if selected.is_empty() {
        return Err(ApiError::Conflict(
            "No enabled members to act on".to_string(),
        ));
    }

    // Resolve the base command from the component (override handled below per-member)
    let base_cmd = match action {
        "start" => component.start_cmd.clone(),
        "stop" => component.stop_cmd.clone(),
        _ => None,
    };

    log_action(
        &state.db,
        user.user_id,
        &format!("cluster_members_{}", action),
        "component",
        component_id,
        json!({
            "action": action,
            "member_count": selected.len(),
            "parallel": body.parallel,
        }),
    )
    .await
    .ok();

    let mut dispatched = Vec::new();
    let mut skipped = Vec::new();

    for m in &selected {
        let cmd = match action {
            "start" => m
                .member
                .start_cmd_override
                .clone()
                .or_else(|| base_cmd.clone()),
            "stop" => m
                .member
                .stop_cmd_override
                .clone()
                .or_else(|| base_cmd.clone()),
            _ => None,
        };
        let Some(cmd) = cmd else {
            skipped.push(json!({
                "member_id": m.member.id,
                "reason": "no_command"
            }));
            continue;
        };

        let request_id = Uuid::new_v4();
        let timeout_secs = match action {
            "start" => component.start_timeout_seconds as u32,
            _ => component.stop_timeout_seconds as u32,
        };
        let message = BackendMessage::ExecuteCommand {
            request_id,
            component_id,
            command: cmd,
            timeout_seconds: timeout_secs,
            exec_mode: "detached".to_string(),
            cluster_member_id: Some(m.member.id),
        };
        let sent = state.ws_hub.send_to_agent(m.member.agent_id, message);
        if sent {
            dispatched.push(json!({
                "member_id": m.member.id,
                "agent_id": m.member.agent_id,
                "request_id": request_id,
            }));
        } else {
            skipped.push(json!({
                "member_id": m.member.id,
                "reason": "agent_unavailable"
            }));
        }
    }

    Ok(Json(json!({
        "action": action,
        "dispatched": dispatched,
        "skipped": skipped,
    })))
}

/// POST /api/v1/components/:id/members/actions/start
pub async fn batch_start(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    body: Option<Json<BatchActionRequest>>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map(|Json(b)| b).unwrap_or(BatchActionRequest {
        member_ids: None,
        parallel: false,
    });
    batch_action(state, user, component_id, "start", body).await
}

/// POST /api/v1/components/:id/members/actions/stop
pub async fn batch_stop(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    body: Option<Json<BatchActionRequest>>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map(|Json(b)| b).unwrap_or(BatchActionRequest {
        member_ids: None,
        parallel: false,
    });
    batch_action(state, user, component_id, "stop", body).await
}

// ============================================================================
// Cluster configuration (mode + policy + threshold)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UpdateClusterConfigRequest {
    #[serde(default)]
    pub cluster_mode: Option<String>,
    #[serde(default)]
    pub cluster_health_policy: Option<String>,
    #[serde(default)]
    pub cluster_min_healthy_pct: Option<i16>,
}

/// PUT /api/v1/components/:id/cluster-config
///
/// Toggles the component's cluster mode (aggregate ↔ fan_out) and/or updates
/// the health aggregation policy without touching the rest of the component.
pub async fn update_cluster_config(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Json(body): Json<UpdateClusterConfigRequest>,
) -> Result<Json<Value>, ApiError> {
    let component = state
        .component_repo
        .get_component(component_id, *user.organization_id)
        .await?
        .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        component.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Validate values before hitting the DB.
    if let Some(ref mode) = body.cluster_mode {
        if mode != "aggregate" && mode != "fan_out" {
            return Err(ApiError::Validation(
                "cluster_mode must be 'aggregate' or 'fan_out'".to_string(),
            ));
        }
    }
    if let Some(ref policy) = body.cluster_health_policy {
        if !matches!(
            policy.as_str(),
            "all_healthy" | "any_healthy" | "quorum" | "threshold_pct"
        ) {
            return Err(ApiError::Validation(
                "cluster_health_policy must be one of all_healthy, any_healthy, quorum, threshold_pct"
                    .to_string(),
            ));
        }
    }
    if let Some(pct) = body.cluster_min_healthy_pct {
        if !(1..=100).contains(&pct) {
            return Err(ApiError::Validation(
                "cluster_min_healthy_pct must be between 1 and 100".to_string(),
            ));
        }
    }

    log_action(
        &state.db,
        user.user_id,
        "update_cluster_config",
        "component",
        component_id,
        json!({
            "cluster_mode": body.cluster_mode,
            "cluster_health_policy": body.cluster_health_policy,
            "cluster_min_healthy_pct": body.cluster_min_healthy_pct,
        }),
    )
    .await
    .ok();

    // Build the UPDATE dynamically via COALESCE — works identically on PG/SQLite.
    #[cfg(feature = "postgres")]
    {
        sqlx::query(
            "UPDATE components SET \
                cluster_mode = COALESCE($2, cluster_mode), \
                cluster_health_policy = COALESCE($3, cluster_health_policy), \
                cluster_min_healthy_pct = COALESCE($4, cluster_min_healthy_pct), \
                updated_at = now() \
             WHERE id = $1",
        )
        .bind(crate::db::bind_id(component_id))
        .bind(body.cluster_mode.as_deref())
        .bind(body.cluster_health_policy.as_deref())
        .bind(body.cluster_min_healthy_pct)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query(
            "UPDATE components SET \
                cluster_mode = COALESCE($2, cluster_mode), \
                cluster_health_policy = COALESCE($3, cluster_health_policy), \
                cluster_min_healthy_pct = COALESCE($4, cluster_min_healthy_pct), \
                updated_at = datetime('now') \
             WHERE id = $1",
        )
        .bind(crate::db::DbUuid::from(component_id))
        .bind(body.cluster_mode.as_deref())
        .bind(body.cluster_health_policy.as_deref())
        .bind(body.cluster_min_healthy_pct)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // If there's an assigned agent, push updated config so scheduler picks up
    // fan-out vs aggregate right away.
    if let Some(agent_id) = component.agent_id {
        crate::websocket::send_config_to_agent(&state, agent_id).await;
    }

    Ok(Json(json!({
        "id": component_id,
        "status": "updated",
    })))
}
