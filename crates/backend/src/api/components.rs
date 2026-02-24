use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::error::{ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct CreateComponentRequest {
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    /// Host as entered by user: FQDN or IP address.
    /// Resolved to agent_id by matching agents.hostname or agents.ip_addresses.
    pub host: Option<String>,
    /// Alias for `host` — backward compatibility with old API callers.
    #[serde(default)]
    pub hostname: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub post_start_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    pub rebuild_infra_cmd: Option<String>,
    pub rebuild_agent_id: Option<Uuid>,
    pub rebuild_protected: Option<bool>,
    pub check_interval_seconds: Option<i32>,
    pub start_timeout_seconds: Option<i32>,
    pub stop_timeout_seconds: Option<i32>,
    pub is_optional: Option<bool>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub env_vars: Option<Value>,
    pub tags: Option<Value>,
}

impl CreateComponentRequest {
    /// Returns the effective host value (prefers `host`, falls back to `hostname` alias).
    pub fn effective_host(&self) -> Option<&str> {
        self.host.as_deref().or(self.hostname.as_deref())
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct UpdateComponentRequest {
    pub name: Option<String>,
    pub component_type: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    /// Host as entered by user: FQDN or IP address.
    pub host: Option<String>,
    /// Alias for `host` — backward compatibility.
    #[serde(default)]
    pub hostname: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub integrity_check_cmd: Option<String>,
    pub post_start_check_cmd: Option<String>,
    pub infra_check_cmd: Option<String>,
    pub rebuild_cmd: Option<String>,
    pub rebuild_infra_cmd: Option<String>,
    pub rebuild_agent_id: Option<Uuid>,
    pub rebuild_protected: Option<bool>,
    pub check_interval_seconds: Option<i32>,
    pub start_timeout_seconds: Option<i32>,
    pub stop_timeout_seconds: Option<i32>,
    pub is_optional: Option<bool>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub env_vars: Option<Value>,
    pub tags: Option<Value>,
}

impl UpdateComponentRequest {
    /// Returns the effective host value (prefers `host`, falls back to `hostname` alias).
    pub fn effective_host(&self) -> Option<&str> {
        self.host.as_deref().or(self.hostname.as_deref())
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComponentRow {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<Uuid>,
    pub host: Option<String>,
    pub agent_id: Option<Uuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDependencyRequest {
    pub from_component_id: Uuid,
    pub to_component_id: Uuid,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: Uuid,
    pub application_id: Uuid,
    pub from_component_id: Uuid,
    pub to_component_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_components(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let components = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, created_at, updated_at
        FROM components WHERE application_id = $1 ORDER BY name
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "components": components })))
}

pub async fn get_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, created_at, updated_at
        FROM components WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
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

    Ok(Json(json!(component)))
}

pub async fn create_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateComponentRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    let comp_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_component",
        "component",
        comp_id,
        json!({"name": body.name}),
    )
    .await?;

    // Use effective_host() to support both "host" and "hostname" JSON fields
    let effective_host = body.effective_host().map(|s| s.to_string());

    // Resolve host → agent_id if host is provided but agent_id is not.
    // No multicast: one host matches exactly one agent.
    let resolved_agent_id = if body.agent_id.is_some() {
        body.agent_id
    } else if let Some(ref host) = effective_host {
        resolve_host_to_agent(&state.db, host).await
    } else {
        None
    };

    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        INSERT INTO components (id, application_id, name, component_type, display_name, description, icon, group_id,
                                host, agent_id, check_cmd, start_cmd, stop_cmd,
                                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
                                position_x, position_y, env_vars, tags)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
        RETURNING id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, created_at, updated_at
        "#,
    )
    .bind(comp_id)
    .bind(app_id)
    .bind(&body.name)
    .bind(&body.component_type)
    .bind(&body.display_name)
    .bind(&body.description)
    .bind(body.icon.as_deref().unwrap_or("box"))
    .bind(body.group_id)
    .bind(&effective_host)
    .bind(resolved_agent_id)
    .bind(&body.check_cmd)
    .bind(&body.start_cmd)
    .bind(&body.stop_cmd)
    .bind(body.check_interval_seconds.unwrap_or(30))
    .bind(body.start_timeout_seconds.unwrap_or(120))
    .bind(body.stop_timeout_seconds.unwrap_or(60))
    .bind(body.is_optional.unwrap_or(false))
    .bind(body.position_x.unwrap_or(0.0))
    .bind(body.position_y.unwrap_or(0.0))
    .bind(body.env_vars.as_ref().unwrap_or(&json!({})))
    .bind(body.tags.as_ref().unwrap_or(&json!([])))
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(json!(component))))
}

pub async fn update_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateComponentRequest>,
) -> Result<Json<Value>, ApiError> {
    // Get current component to check app permission
    let current =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, current, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "update_component",
        "component",
        id,
        json!({}),
    )
    .await?;

    // Use effective_host() to support both "host" and "hostname" JSON fields
    let effective_host = body.effective_host().map(|s| s.to_string());

    // If host changed, re-resolve agent_id
    let resolved_agent_id = if body.agent_id.is_some() {
        body.agent_id
    } else if let Some(ref host) = effective_host {
        resolve_host_to_agent(&state.db, host).await
    } else {
        None
    };

    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        UPDATE components SET
            name = COALESCE($2, name),
            component_type = COALESCE($3, component_type),
            display_name = COALESCE($4, display_name),
            description = COALESCE($5, description),
            icon = COALESCE($6, icon),
            group_id = COALESCE($7, group_id),
            host = COALESCE($8, host),
            agent_id = COALESCE($9, agent_id),
            check_cmd = COALESCE($10, check_cmd),
            start_cmd = COALESCE($11, start_cmd),
            stop_cmd = COALESCE($12, stop_cmd),
            check_interval_seconds = COALESCE($13, check_interval_seconds),
            start_timeout_seconds = COALESCE($14, start_timeout_seconds),
            stop_timeout_seconds = COALESCE($15, stop_timeout_seconds),
            is_optional = COALESCE($16, is_optional),
            position_x = COALESCE($17, position_x),
            position_y = COALESCE($18, position_y),
            updated_at = now()
        WHERE id = $1
        RETURNING id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.component_type)
    .bind(&body.display_name)
    .bind(&body.description)
    .bind(&body.icon)
    .bind(body.group_id)
    .bind(&effective_host)
    .bind(resolved_agent_id)
    .bind(&body.check_cmd)
    .bind(&body.start_cmd)
    .bind(&body.stop_cmd)
    .bind(body.check_interval_seconds)
    .bind(body.start_timeout_seconds)
    .bind(body.stop_timeout_seconds)
    .bind(body.is_optional)
    .bind(body.position_x)
    .bind(body.position_y)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(component)))
}

pub async fn delete_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_component",
        "component",
        id,
        json!({}),
    )
    .await?;

    sqlx::query("DELETE FROM components WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "start_component",
        "component",
        id,
        json!({}),
    )
    .await?;

    // Trigger FSM transition to Starting
    crate::core::fsm::transition_component(&state, id, appcontrol_common::ComponentState::Starting)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    Ok(Json(json!({ "status": "starting" })))
}

pub async fn stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "stop_component",
        "component",
        id,
        json!({}),
    )
    .await?;

    crate::core::fsm::transition_component(&state, id, appcontrol_common::ComponentState::Stopping)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn force_stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "force_stop_component",
        "component",
        id,
        json!({"reason": "production_incident", "bypass_dependencies": true}),
    )
    .await?;

    // Force stop: bypass FSM and DAG dependencies
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::force_stop_single_component(&state_clone, id).await
        {
            tracing::error!("Force stop failed for component {}: {}", id, e);
        }
    });

    Ok(Json(
        json!({ "status": "force_stopping", "message": "Force stop initiated (bypassing dependencies)" }),
    ))
}

pub async fn start_with_deps(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Build DAG and find all upstream dependencies of this component
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut subset = dag.find_all_dependencies(id);
    subset.insert(id); // Include the target component itself

    log_action(
        &state.db,
        user.user_id,
        "start_with_deps",
        "component",
        id,
        json!({"component_id": id, "dependency_count": subset.len() - 1}),
    )
    .await?;

    // Acquire operation lock
    let guard = state
        .operation_lock
        .try_lock(app_id, "start_with_deps", user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    let total_components = subset.len();
    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(e) =
            crate::core::sequencer::execute_start_subset(&state_clone, app_id, &subset).await
        {
            tracing::error!("Failed to start component {} with deps: {}", id, e);
        }
    });

    Ok(Json(json!({
        "status": "starting_with_deps",
        "component_id": id,
        "total_components": total_components,
    })))
}

pub async fn execute_command(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((id, cmd)): Path<(Uuid, String)>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "execute_command",
        "component",
        id,
        json!({"command": cmd}),
    )
    .await?;

    // Look up the command from component_commands
    let command = sqlx::query_scalar::<_, String>(
        "SELECT command FROM component_commands WHERE component_id = $1 AND name = $2",
    )
    .bind(id)
    .bind(&cmd)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    let request_id = Uuid::new_v4();
    Ok(Json(
        json!({ "request_id": request_id, "command": command, "status": "executing" }),
    ))
}

pub async fn list_dependencies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let deps = sqlx::query_as::<_, DependencyRow>(
        "SELECT id, application_id, from_component_id, to_component_id, created_at FROM dependencies WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "dependencies": deps })))
}

pub async fn create_dependency(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateDependencyRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Check for cycles before inserting
    if crate::core::dag::validate_no_cycle(
        &state.db,
        app_id,
        body.from_component_id,
        body.to_component_id,
    )
    .await
    .is_err()
    {
        return Err(ApiError::Conflict(
            "Adding this dependency would create a cycle".to_string(),
        ));
    }

    let dep_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_dependency",
        "dependency",
        dep_id,
        json!({"from": body.from_component_id, "to": body.to_component_id}),
    )
    .await?;

    let dep = sqlx::query_as::<_, DependencyRow>(
        r#"
        INSERT INTO dependencies (id, application_id, from_component_id, to_component_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id, application_id, from_component_id, to_component_id, created_at
        "#,
    )
    .bind(dep_id)
    .bind(app_id)
    .bind(body.from_component_id)
    .bind(body.to_component_id)
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(json!(dep))))
}

pub async fn delete_dependency(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM dependencies WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_dependency",
        "dependency",
        id,
        json!({}),
    )
    .await?;

    sqlx::query("DELETE FROM dependencies WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Host → Agent resolution
// ---------------------------------------------------------------------------

/// Resolve a user-provided host (FQDN or IP) to an agent_id.
///
/// Match order:
/// 1. Exact hostname match in agents table
/// 2. IP address match in agents.ip_addresses JSONB array
///
/// No multicast: returns the first match only. If multiple agents
/// share an IP, the first one (by created_at) wins.
pub async fn resolve_host_to_agent(pool: &sqlx::PgPool, host: &str) -> Option<Uuid> {
    // 1. Try exact hostname match
    let by_hostname = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM agents WHERE hostname = $1 AND is_active = true ORDER BY created_at LIMIT 1",
    )
    .bind(host)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if by_hostname.is_some() {
        return by_hostname;
    }

    // 2. Try IP address match in JSONB array
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM agents WHERE ip_addresses ? $1 AND is_active = true ORDER BY created_at LIMIT 1",
    )
    .bind(host)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Called when an agent registers: resolve all components that reference
/// this agent's hostname or IPs via the `host` field but have no agent_id yet.
///
/// This is the "late binding" path: user creates component with host="srv-oracle.prod",
/// agent registers later with hostname="srv-oracle.prod" → agent_id is set automatically.
pub async fn resolve_components_for_agent(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    hostname: &str,
    ip_addresses: &[String],
) {
    // Match components by hostname
    let result =
        sqlx::query("UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL")
            .bind(agent_id)
            .bind(hostname)
            .execute(pool)
            .await;

    if let Ok(r) = result {
        if r.rows_affected() > 0 {
            tracing::info!(
                agent_id = %agent_id,
                hostname = %hostname,
                count = r.rows_affected(),
                "Resolved components by hostname"
            );
        }
    }

    // Match components by any of the agent's IP addresses
    for ip in ip_addresses {
        let result =
            sqlx::query("UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL")
                .bind(agent_id)
                .bind(ip)
                .execute(pool)
                .await;

        if let Ok(r) = result {
            if r.rows_affected() > 0 {
                tracing::info!(
                    agent_id = %agent_id,
                    ip = %ip,
                    count = r.rows_affected(),
                    "Resolved components by IP address"
                );
            }
        }
    }
}
