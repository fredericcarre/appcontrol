use crate::db::DbUuid;
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
    /// Cluster size: number of nodes (NULL = not a cluster, >= 2 for cluster)
    pub cluster_size: Option<i32>,
    /// List of node hostnames/IPs in the cluster
    pub cluster_nodes: Option<Vec<String>>,
    /// Reference to another application (for app-type synthetic components)
    pub referenced_app_id: Option<Uuid>,
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
    /// Cluster size: number of nodes (NULL = not a cluster, >= 2 for cluster)
    pub cluster_size: Option<i32>,
    /// List of node hostnames/IPs in the cluster
    pub cluster_nodes: Option<Vec<String>>,
    /// Reference to another application (for app-type synthetic components)
    pub referenced_app_id: Option<Uuid>,
}

impl UpdateComponentRequest {
    /// Returns the effective host value (prefers `host`, falls back to `hostname` alias).
    pub fn effective_host(&self) -> Option<&str> {
        self.host.as_deref().or(self.hostname.as_deref())
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComponentRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub name: String,
    pub component_type: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub group_id: Option<DbUuid>,
    pub host: Option<String>,
    pub agent_id: Option<DbUuid>,
    pub check_cmd: Option<String>,
    pub start_cmd: Option<String>,
    pub stop_cmd: Option<String>,
    pub check_interval_seconds: i32,
    pub start_timeout_seconds: i32,
    pub stop_timeout_seconds: i32,
    pub is_optional: bool,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub cluster_size: Option<i32>,
    pub cluster_nodes: Option<Value>,
    pub referenced_app_id: Option<DbUuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDependencyRequest {
    pub from_component_id: DbUuid,
    pub to_component_id: DbUuid,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub from_component_id: DbUuid,
    pub to_component_id: DbUuid,
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

    #[cfg(feature = "postgres")]
    let components = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at
        FROM components WHERE application_id = $1 ORDER BY name
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let components = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at
        FROM components WHERE application_id = $1 ORDER BY name
        "#,
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "components": components })))
}

pub async fn get_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    #[cfg(feature = "postgres")]
    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at
        FROM components WHERE id = $1
        "#,
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at
        FROM components WHERE id = $1
        "#,
    )
    .bind(DbUuid::from(id))
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

    // Convert cluster_nodes Vec<String> to JSONB Value
    let cluster_nodes_json = body
        .cluster_nodes
        .as_ref()
        .map(|nodes| serde_json::to_value(nodes).unwrap_or(json!([])));

    #[cfg(feature = "postgres")]
    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        INSERT INTO components (id, application_id, name, component_type, display_name, description, icon, group_id,
                                host, agent_id, check_cmd, start_cmd, stop_cmd,
                                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
                                position_x, position_y, env_vars, tags, cluster_size, cluster_nodes, referenced_app_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)
        RETURNING id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at
        "#,
    )
    .bind(crate::db::bind_id(comp_id))
    .bind(crate::db::bind_id(app_id))
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
    .bind(body.cluster_size)
    .bind(&cluster_nodes_json)
    .bind(body.referenced_app_id)
    .fetch_one(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        INSERT INTO components (id, application_id, name, component_type, display_name, description, icon, group_id,
                                host, agent_id, check_cmd, start_cmd, stop_cmd,
                                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
                                position_x, position_y, env_vars, tags, cluster_size, cluster_nodes, referenced_app_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)
        RETURNING id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at
        "#,
    )
    .bind(DbUuid::from(comp_id))
    .bind(DbUuid::from(app_id))
    .bind(&body.name)
    .bind(&body.component_type)
    .bind(&body.display_name)
    .bind(&body.description)
    .bind(body.icon.as_deref().unwrap_or("box"))
    .bind(body.group_id.map(DbUuid::from))
    .bind(&effective_host)
    .bind(resolved_agent_id.map(DbUuid::from))
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
    .bind(body.cluster_size)
    .bind(&cluster_nodes_json)
    .bind(body.referenced_app_id.map(DbUuid::from))
    .fetch_one(&state.db)
    .await?;

    // Push config to affected agent so it starts health checks immediately
    let agent_ids = resolved_agent_id.map(|id| vec![id]);
    crate::websocket::push_config_to_affected_agents(
        &state,
        Some(app_id),
        None,
        agent_ids.as_deref(),
    )
    .await;

    Ok((StatusCode::CREATED, Json(json!(component))))
}

pub async fn update_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateComponentRequest>,
) -> Result<Json<Value>, ApiError> {
    // Get current component to check app permission
    #[cfg(feature = "postgres")]
    let current =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let current =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(DbUuid::from(id))
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

    // Convert cluster_nodes Vec<String> to JSONB Value
    let cluster_nodes_json = body
        .cluster_nodes
        .as_ref()
        .map(|nodes| serde_json::to_value(nodes).unwrap_or(json!([])));

    let update_sql = format!(
        "UPDATE components SET
            name = COALESCE($2, name),
            component_type = COALESCE($3, component_type),
            display_name = $4,
            description = $5,
            icon = COALESCE($6, icon),
            group_id = $7,
            host = COALESCE($8, host),
            agent_id = COALESCE($9, agent_id),
            check_cmd = $10,
            start_cmd = $11,
            stop_cmd = $12,
            check_interval_seconds = COALESCE($13, check_interval_seconds),
            start_timeout_seconds = COALESCE($14, start_timeout_seconds),
            stop_timeout_seconds = COALESCE($15, stop_timeout_seconds),
            is_optional = COALESCE($16, is_optional),
            position_x = COALESCE($17, position_x),
            position_y = COALESCE($18, position_y),
            cluster_size = $19,
            cluster_nodes = $20,
            referenced_app_id = $21,
            updated_at = {}
        WHERE id = $1
        RETURNING id, application_id, name, component_type, display_name, description, icon, group_id,
               host, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, cluster_size, cluster_nodes, referenced_app_id, created_at, updated_at",
        crate::db::sql::now()
    );

    // Note: Fields that can be explicitly cleared (set to NULL) use direct assignment.
    // Fields that should always have a value use COALESCE to keep existing if not provided.
    // Special case: host and agent_id use COALESCE to preserve existing if not explicitly provided.
    #[cfg(feature = "postgres")]
    let component = sqlx::query_as::<_, ComponentRow>(&update_sql)
        .bind(crate::db::bind_id(id))
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
        .bind(body.cluster_size)
        .bind(&cluster_nodes_json)
        .bind(body.referenced_app_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let component = sqlx::query_as::<_, ComponentRow>(&update_sql)
        .bind(DbUuid::from(id))
        .bind(&body.name)
        .bind(&body.component_type)
        .bind(&body.display_name)
        .bind(&body.description)
        .bind(&body.icon)
        .bind(body.group_id.map(DbUuid::from))
        .bind(&effective_host)
        .bind(resolved_agent_id.map(DbUuid::from))
        .bind(&body.check_cmd)
        .bind(&body.start_cmd)
        .bind(&body.stop_cmd)
        .bind(body.check_interval_seconds)
        .bind(body.start_timeout_seconds)
        .bind(body.stop_timeout_seconds)
        .bind(body.is_optional)
        .bind(body.position_x)
        .bind(body.position_y)
        .bind(body.cluster_size)
        .bind(&cluster_nodes_json)
        .bind(body.referenced_app_id.map(DbUuid::from))
        .fetch_optional(&state.db)
        .await?
        .ok_or_not_found()?;

    // Push config to affected agent so it picks up the changes
    // Use the actual agent_id from the updated component (after COALESCE), not the resolved one
    let agent_ids: Option<Vec<uuid::Uuid>> = component.agent_id.map(|id| vec![id.into_inner()]);
    crate::websocket::push_config_to_affected_agents(
        &state,
        Some(current.into()),
        None,
        agent_ids.as_deref(),
    )
    .await;

    Ok(Json(json!(component)))
}

pub async fn delete_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Get app_id and agent_id before deleting
    #[cfg(feature = "postgres")]
    let (app_id, agent_id): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT application_id, agent_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let (app_id, agent_id): (Uuid, Option<Uuid>) = {
        let row: (DbUuid, Option<DbUuid>) =
            sqlx::query_as("SELECT application_id, agent_id FROM components WHERE id = $1")
                .bind(DbUuid::from(id))
                .fetch_optional(&state.db)
                .await?
                .ok_or_not_found()?;
        (row.0.into_inner(), row.1.map(|u| u.into_inner()))
    };

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

    #[cfg(feature = "postgres")]
    sqlx::query("DELETE FROM components WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("DELETE FROM components WHERE id = $1")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    // Push config to affected agent so it stops checking this component
    let agent_ids = agent_id.map(|id| vec![id]);
    crate::websocket::push_config_to_affected_agents(
        &state,
        Some(app_id),
        None,
        agent_ids.as_deref(),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ── Position Update (for Map Designer) ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdatePositionRequest {
    pub x: f32,
    pub y: f32,
}

/// PATCH /api/v1/components/:id/position
/// Update component position on the map canvas (for drag & drop editing).
/// Requires Edit permission (not just Operate) since it modifies configuration.
pub async fn update_position(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePositionRequest>,
) -> Result<StatusCode, ApiError> {
    // Get app_id for permission check
    #[cfg(feature = "postgres")]
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(DbUuid::from(id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Note: We don't log position updates to avoid spamming action_log during drag operations.
    // Position is not a critical operational parameter.

    let pos_sql = format!(
        "UPDATE components SET position_x = $2, position_y = $3, updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    );

    #[cfg(feature = "postgres")]
    sqlx::query(&pos_sql)
        .bind(crate::db::bind_id(id))
        .bind(body.x)
        .bind(body.y)
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(&pos_sql)
        .bind(DbUuid::from(id))
        .bind(body.x)
        .bind(body.y)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/v1/components/batch-positions
/// Update multiple component positions at once (for batch save after editing session).
#[derive(Debug, Deserialize)]
pub struct BatchPositionUpdate {
    pub positions: Vec<ComponentPosition>,
}

#[derive(Debug, Deserialize)]
pub struct ComponentPosition {
    pub id: DbUuid,
    pub x: f32,
    pub y: f32,
}

pub async fn update_positions_batch(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<BatchPositionUpdate>,
) -> Result<StatusCode, ApiError> {
    if body.positions.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Get app_id from first component (all should be in same app)
    let first_id = body.positions[0].id;
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(first_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Update all positions in a transaction
    let mut tx = state.db.begin().await?;

    for pos in &body.positions {
        sqlx::query(&format!("UPDATE components SET position_x = $2, position_y = $3, updated_at = {} WHERE id = $1 AND application_id = $4", crate::db::sql::now()))
            .bind(pos.id)
            .bind(pos.x)
            .bind(pos.y)
            .bind(crate::db::bind_id(app_id))
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Get component info including referenced_app_id
    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        application_id: DbUuid,
        referenced_app_id: Option<DbUuid>,
    }

    let comp_info = sqlx::query_as::<_, ComponentInfo>(
        "SELECT application_id, referenced_app_id FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        comp_info.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "start_component",
        "component",
        id,
        json!({"referenced_app_id": comp_info.referenced_app_id}),
    )
    .await?;

    // For application-type components, start the referenced app instead
    if let Some(ref_app_id) = comp_info.referenced_app_id {
        // Check permission on referenced app as well
        let ref_perm =
            effective_permission(&state.db, user.user_id, ref_app_id, user.is_admin()).await;
        if ref_perm < PermissionLevel::Operate {
            return Err(ApiError::Forbidden);
        }

        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::core::sequencer::execute_start(&state_clone, ref_app_id).await {
                tracing::error!("Start referenced app {} failed: {}", ref_app_id, e);
            }
        });

        return Ok(Json(
            json!({ "status": "starting", "propagated_to_app": ref_app_id }),
        ));
    }

    // Start the component: transition to Starting, dispatch start_cmd to agent
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::start_single_component(&state_clone, id).await {
            tracing::error!("Start component failed for {}: {}", id, e);
        }
    });

    Ok(Json(json!({ "status": "starting" })))
}

pub async fn stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // Get component info including referenced_app_id
    #[derive(sqlx::FromRow)]
    struct ComponentInfo {
        application_id: DbUuid,
        referenced_app_id: Option<DbUuid>,
    }

    let comp_info = sqlx::query_as::<_, ComponentInfo>(
        "SELECT application_id, referenced_app_id FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        comp_info.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "stop_component",
        "component",
        id,
        json!({"referenced_app_id": comp_info.referenced_app_id}),
    )
    .await?;

    // For application-type components, stop the referenced app instead
    if let Some(ref_app_id) = comp_info.referenced_app_id {
        // Check permission on referenced app as well
        let ref_perm =
            effective_permission(&state.db, user.user_id, ref_app_id, user.is_admin()).await;
        if ref_perm < PermissionLevel::Operate {
            return Err(ApiError::Forbidden);
        }

        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::core::sequencer::execute_stop(&state_clone, ref_app_id).await {
                tracing::error!("Stop referenced app {} failed: {}", ref_app_id, e);
            }
        });

        return Ok(Json(
            json!({ "status": "stopping", "propagated_to_app": ref_app_id }),
        ));
    }

    // Stop the component and all its dependents in reverse DAG order
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::core::sequencer::stop_with_dependents(&state_clone, id).await {
            tracing::error!("Stop component with dependents failed for {}: {}", id, e);
        }
    });

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn force_stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(id))
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
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(id))
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
        .try_lock(app_id, "start_with_deps", *user.user_id)
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

/// Repair/restart a component by:
/// 1. Stopping all dependents (components that depend on this one)
/// 2. Stopping this component
/// 3. Starting this component
/// 4. Starting all dependents (in DAG order)
pub async fn restart_with_dependents(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Build DAG and find all downstream dependents of this component
    let dag = crate::core::dag::build_dag(&state.db, app_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let dependents = dag.find_all_dependents(id);
    let dependent_count = dependents.len();

    log_action(
        &state.db,
        user.user_id,
        "restart_with_dependents",
        "component",
        id,
        json!({"component_id": id, "dependent_count": dependent_count}),
    )
    .await?;

    // Acquire operation lock
    let guard = state
        .operation_lock
        .try_lock(app_id, "restart_with_dependents", *user.user_id)
        .await
        .map_err(|e| ApiError::Conflict(e.to_string()))?;

    // Build set of all components to restart (target + dependents)
    let mut all_components = dependents.clone();
    all_components.insert(id);
    let total_components = all_components.len();

    let state_clone = state.clone();
    tokio::spawn(async move {
        let _guard = guard;

        // Phase 1: Stop the target component and all its dependents (in reverse DAG order)
        tracing::info!(
            component_id = %id,
            dependent_count = dependent_count,
            "Phase 1: Stopping component and dependents"
        );
        if let Err(e) = crate::core::sequencer::stop_with_dependents(&state_clone, id).await {
            tracing::error!("Failed to stop component {} with dependents: {}", id, e);
            return;
        }

        // Phase 2: Start all components (target + dependents) in DAG order
        tracing::info!(
            component_id = %id,
            total_components = total_components,
            "Phase 2: Starting component and dependents"
        );
        if let Err(e) =
            crate::core::sequencer::execute_start_subset(&state_clone, app_id, &all_components)
                .await
        {
            tracing::error!("Failed to start component {} with dependents: {}", id, e);
            return;
        }

        tracing::info!(component_id = %id, "Restart with dependents completed");
    });

    Ok(Json(json!({
        "status": "restarting_with_dependents",
        "component_id": id,
        "dependent_count": dependent_count,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandBody {
    #[serde(default)]
    pub parameters: Option<std::collections::HashMap<String, String>>,
}

pub async fn execute_command(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((id, cmd)): Path<(Uuid, String)>,
    body: Option<Json<ExecuteCommandBody>>,
) -> Result<Json<Value>, ApiError> {
    // Fetch component with all command columns
    #[derive(sqlx::FromRow)]
    struct ComponentCmd {
        application_id: DbUuid,
        agent_id: Option<DbUuid>,
        check_cmd: Option<String>,
        start_cmd: Option<String>,
        stop_cmd: Option<String>,
        integrity_check_cmd: Option<String>,
        infra_check_cmd: Option<String>,
    }

    #[cfg(feature = "postgres")]
    let comp = sqlx::query_as::<_, ComponentCmd>(
        "SELECT application_id, agent_id, check_cmd, start_cmd, stop_cmd, \
                integrity_check_cmd, infra_check_cmd \
         FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let comp = sqlx::query_as::<_, ComponentCmd>(
        "SELECT application_id, agent_id, check_cmd, start_cmd, stop_cmd, \
                integrity_check_cmd, infra_check_cmd \
         FROM components WHERE id = $1",
    )
    .bind(DbUuid::from(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    let perm = effective_permission(
        &state.db,
        user.user_id,
        comp.application_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(ApiError::Forbidden);
    }

    // Check if this is a built-in command
    let builtin_cmd = match cmd.as_str() {
        "check" => comp.check_cmd.clone(),
        "start" => comp.start_cmd.clone(),
        "stop" => comp.stop_cmd.clone(),
        // restart_cmd is not a standard column; handled as custom command
        "integrity_check" => comp.integrity_check_cmd.clone(),
        "infra_check" => comp.infra_check_cmd.clone(),
        _ => None,
    };

    // If it's a built-in command, use it directly; otherwise look up in component_commands
    let (command_id, command_template): (Option<Uuid>, String) = if let Some(builtin) = builtin_cmd
    {
        (None, builtin)
    } else {
        // Look up the command definition from component_commands
        let cmd_row = sqlx::query_as::<_, (DbUuid, String, bool)>(
            "SELECT id, command, requires_confirmation FROM component_commands WHERE component_id = $1 AND name = $2",
        )
        .bind(crate::db::bind_id(id))
        .bind(&cmd)
        .fetch_optional(&state.db)
        .await?
        .ok_or_not_found()?;

        (Some(cmd_row.0.into()), cmd_row.1)
    };

    let agent_id = comp.agent_id;

    // Load parameter definitions and validate/interpolate (only for custom commands)
    let params = if let Some(cid) = command_id {
        sqlx::query_as::<_, crate::api::command_params::InputParamRow>(
            "SELECT id, command_id, name, description, default_value, validation_regex, required, display_order, created_at \
             FROM command_input_params WHERE command_id = $1 ORDER BY display_order, name",
        )
        .bind(crate::db::bind_id(cid))
        .fetch_all(&state.db)
        .await?
    } else {
        vec![]
    };

    let param_values = body
        .as_ref()
        .and_then(|b| b.parameters.clone())
        .unwrap_or_default();

    let final_command = if params.is_empty() {
        command_template.clone()
    } else {
        crate::api::command_params::validate_and_interpolate_params(
            &command_template,
            &params,
            &param_values,
        )
        .map_err(|errors| ApiError::Validation(errors.join("; ")))?
    };

    log_action(
        &state.db,
        user.user_id,
        "execute_command",
        "component",
        id,
        json!({"command": cmd, "parameters": param_values}),
    )
    .await?;

    let agent_id = agent_id.ok_or(ApiError::Conflict(
        "No agent assigned to this component".to_string(),
    ))?;

    let request_id = Uuid::new_v4();

    // Record dispatch in command_executions for audit trail
    let command_type_label = if command_id.is_none() { &cmd } else { "custom" };
    if let Err(e) = sqlx::query(
        "INSERT INTO command_executions (request_id, component_id, agent_id, command_type, status, user_id, command_text)
         VALUES ($1, $2, $3, $4, 'dispatched', $5, $6)
         ON CONFLICT (request_id) DO NOTHING",
    )
    .bind(request_id)
    .bind(crate::db::bind_id(id))
    .bind(crate::db::bind_id(agent_id))
    .bind(command_type_label)
    .bind(user.user_id)
    .bind(&final_command)
    .execute(&state.db)
    .await
    {
        tracing::warn!(request_id = %request_id, "Failed to record command dispatch: {}", e);
    }

    // Dispatch to agent via WebSocket hub (sync mode for custom commands)
    let message = appcontrol_common::BackendMessage::ExecuteCommand {
        request_id,
        component_id: id,
        command: final_command.clone(),
        timeout_seconds: 300,
        exec_mode: "sync".to_string(),
    };

    let dispatched = state.ws_hub.send_to_agent(agent_id, message);
    if !dispatched {
        return Err(ApiError::Conflict(
            "Agent is not reachable — no gateway route available".to_string(),
        ));
    }

    metrics::counter!("commands_executed_total", "command" => cmd.clone()).increment(1);

    tracing::info!(
        request_id = %request_id,
        component_id = %id,
        agent_id = %agent_id,
        "Custom command dispatched to agent (sync)"
    );

    Ok(Json(json!({
        "request_id": request_id,
        "command": final_command,
        "status": "dispatched",
        "component_id": id,
        "agent_id": agent_id,
    })))
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

    #[cfg(feature = "postgres")]
    let deps = sqlx::query_as::<_, DependencyRow>(
        "SELECT id, application_id, from_component_id, to_component_id, created_at FROM dependencies WHERE application_id = $1",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let deps = sqlx::query_as::<_, DependencyRow>(
        "SELECT id, application_id, from_component_id, to_component_id, created_at FROM dependencies WHERE application_id = $1",
    )
    .bind(DbUuid::from(app_id))
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

    #[cfg(feature = "postgres")]
    let dep = sqlx::query_as::<_, DependencyRow>(
        r#"
        INSERT INTO dependencies (id, application_id, from_component_id, to_component_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id, application_id, from_component_id, to_component_id, created_at
        "#,
    )
    .bind(dep_id)
    .bind(crate::db::bind_id(app_id))
    .bind(body.from_component_id)
    .bind(body.to_component_id)
    .fetch_one(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let dep = sqlx::query_as::<_, DependencyRow>(
        r#"
        INSERT INTO dependencies (id, application_id, from_component_id, to_component_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id, application_id, from_component_id, to_component_id, created_at
        "#,
    )
    .bind(DbUuid::from(dep_id))
    .bind(DbUuid::from(app_id))
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
    #[cfg(feature = "postgres")]
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM dependencies WHERE id = $1")
            .bind(crate::db::bind_id(id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM dependencies WHERE id = $1")
            .bind(DbUuid::from(id))
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

    #[cfg(feature = "postgres")]
    sqlx::query("DELETE FROM dependencies WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("DELETE FROM dependencies WHERE id = $1")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Custom Commands listing
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CustomCommandRow {
    pub id: DbUuid,
    pub component_id: DbUuid,
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub requires_confirmation: bool,
    pub min_permission_level: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List custom commands for a component.
pub async fn list_custom_commands(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let commands = sqlx::query_as::<_, CustomCommandRow>(
        "SELECT id, component_id, name, command, description, requires_confirmation, min_permission_level, created_at \
         FROM component_commands WHERE component_id = $1 ORDER BY name",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "commands": commands })))
}

// ---------------------------------------------------------------------------
// Command Execution History
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CommandExecutionRow {
    pub id: DbUuid,
    pub request_id: DbUuid,
    pub component_id: DbUuid,
    pub agent_id: Option<DbUuid>,
    pub command_type: String,
    pub exit_code: Option<i16>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i32>,
    pub status: String,
    pub dispatched_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ExecutionHistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

/// List command execution history for a component.
pub async fn list_command_executions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ExecutionHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let executions = if let Some(ref status_filter) = query.status {
        sqlx::query_as::<_, CommandExecutionRow>(
            "SELECT id, request_id, component_id, agent_id, command_type, exit_code, \
                    stdout, stderr, duration_ms, status, dispatched_at, completed_at \
             FROM command_executions \
             WHERE component_id = $1 AND status = $2 \
             ORDER BY dispatched_at DESC LIMIT $3 OFFSET $4",
        )
        .bind(crate::db::bind_id(component_id))
        .bind(status_filter)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, CommandExecutionRow>(
            "SELECT id, request_id, component_id, agent_id, command_type, exit_code, \
                    stdout, stderr, duration_ms, status, dispatched_at, completed_at \
             FROM command_executions \
             WHERE component_id = $1 \
             ORDER BY dispatched_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(crate::db::bind_id(component_id))
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(json!({ "executions": executions })))
}

// ---------------------------------------------------------------------------
// Component State Transitions History
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StateTransitionRow {
    pub id: DbUuid,
    pub component_id: DbUuid,
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List state transition history for a component.
pub async fn list_state_transitions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ExecutionHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let transitions = sqlx::query_as::<_, StateTransitionRow>(
        "SELECT id, component_id, from_state, to_state, trigger, created_at \
         FROM state_transitions \
         WHERE component_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(crate::db::bind_id(component_id))
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "transitions": transitions })))
}

// ---------------------------------------------------------------------------
// Check Events History
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CheckEventRow {
    pub id: i64,
    pub component_id: DbUuid,
    pub check_type: String,
    pub exit_code: i16,
    pub stdout: Option<String>,
    pub duration_ms: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List recent check events (health/integrity/infra checks) for a component.
pub async fn list_check_events(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ExecutionHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let events = sqlx::query_as::<_, CheckEventRow>(
        "SELECT id, component_id, check_type, exit_code, stdout, duration_ms, created_at \
         FROM check_events \
         WHERE component_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(crate::db::bind_id(component_id))
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "events": events })))
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
pub async fn resolve_host_to_agent(pool: &crate::db::DbPool, host: &str) -> Option<Uuid> {
    // 1. Try exact hostname match
    #[cfg(feature = "postgres")]
    let by_hostname = sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM agents WHERE hostname = $1 AND is_active = true ORDER BY created_at LIMIT 1",
    )
    .bind(host)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let by_hostname = sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM agents WHERE hostname = $1 AND is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .bind(host)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if by_hostname.is_some() {
        return by_hostname.map(|x| x.into_inner());
    }

    // 2. Try IP address match in JSONB array
    #[cfg(feature = "postgres")]
    let by_ip = sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM agents WHERE ip_addresses ? $1 AND is_active = true ORDER BY created_at LIMIT 1",
    )
    .bind(host)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let by_ip = sqlx::query_scalar::<_, DbUuid>(
        "SELECT id FROM agents WHERE EXISTS(SELECT 1 FROM json_each(ip_addresses) WHERE value = $1) AND is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .bind(host)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    by_ip.map(|x| x.into_inner())
}

/// Called when an agent registers: resolve all components that reference
/// this agent's hostname or IPs via the `host` field but have no agent_id yet.
///
/// This is the "late binding" path: user creates component with host="srv-oracle.prod",
/// agent registers later with hostname="srv-oracle.prod" → agent_id is set automatically.
pub async fn resolve_components_for_agent(
    pool: &crate::db::DbPool,
    agent_id: impl Into<Uuid> + Send,
    hostname: &str,
    ip_addresses: &[String],
) {
    let agent_id: Uuid = agent_id.into();
    // Match components by hostname
    let result =
        sqlx::query("UPDATE components SET agent_id = $1 WHERE host = $2 AND agent_id IS NULL")
            .bind(crate::db::bind_id(agent_id))
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
                .bind(crate::db::bind_id(agent_id))
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

// ---------------------------------------------------------------------------
// Component Metrics (from check command stdout)
// ---------------------------------------------------------------------------

/// Get the latest metrics for a component.
/// Metrics are extracted from check command stdout (any valid JSON).
pub async fn get_component_metrics(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Get latest check event with metrics
    let latest = sqlx::query_as::<_, (serde_json::Value, i16, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT metrics, exit_code, created_at
           FROM check_events
           WHERE component_id = $1 AND metrics IS NOT NULL
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(&state.db)
    .await?;

    match latest {
        Some((metrics, exit_code, at)) => Ok(Json(json!({
            "component_id": component_id,
            "metrics": metrics,
            "exit_code": exit_code,
            "at": at,
        }))),
        None => Ok(Json(json!({
            "component_id": component_id,
            "metrics": null,
            "message": "No metrics available"
        }))),
    }
}

/// Get metrics history for a component (for charts).
pub async fn get_component_metrics_history(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    // Get last 100 check events with metrics (last ~1 hour at 30s intervals)
    let history = sqlx::query_as::<_, (serde_json::Value, i16, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT metrics, exit_code, created_at
           FROM check_events
           WHERE component_id = $1 AND metrics IS NOT NULL
           ORDER BY created_at DESC
           LIMIT 100"#,
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_all(&state.db)
    .await?;

    let points: Vec<Value> = history
        .into_iter()
        .map(|(metrics, exit_code, at)| {
            json!({
                "metrics": metrics,
                "exit_code": exit_code,
                "at": at,
            })
        })
        .collect();

    Ok(Json(json!({
        "component_id": component_id,
        "history": points,
    })))
}

// ============================================================================
// Site Overrides (Failover Configuration)
// ============================================================================

/// GET /api/v1/components/:id/site-overrides
///
/// Returns all site overrides for a component, with site and agent details.
pub async fn list_site_overrides(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    // First get the app_id to check permissions
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    #[derive(Debug, sqlx::FromRow)]
    struct SiteOverrideRow {
        id: DbUuid,
        component_id: DbUuid,
        site_id: DbUuid,
        agent_id_override: Option<DbUuid>,
        check_cmd_override: Option<String>,
        start_cmd_override: Option<String>,
        stop_cmd_override: Option<String>,
        rebuild_cmd_override: Option<String>,
        env_vars_override: Option<Value>,
        created_at: chrono::DateTime<chrono::Utc>,
        // Joined
        site_name: String,
        site_code: String,
        site_type: String,
        agent_hostname: Option<String>,
    }

    let overrides = sqlx::query_as::<_, SiteOverrideRow>(
        r#"
        SELECT
            so.id, so.component_id, so.site_id,
            so.agent_id_override, so.check_cmd_override, so.start_cmd_override,
            so.stop_cmd_override, so.rebuild_cmd_override, so.env_vars_override,
            so.created_at,
            s.name as site_name, s.code as site_code, s.site_type,
            a.hostname as agent_hostname
        FROM site_overrides so
        JOIN sites s ON so.site_id = s.id
        LEFT JOIN agents a ON so.agent_id_override = a.id
        WHERE so.component_id = $1
        ORDER BY s.site_type, s.name
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_all(&state.db)
    .await?;

    let data: Vec<Value> = overrides
        .into_iter()
        .map(|o| {
            json!({
                "id": o.id,
                "component_id": o.component_id,
                "site_id": o.site_id,
                "site_name": o.site_name,
                "site_code": o.site_code,
                "site_type": o.site_type,
                "agent_id_override": o.agent_id_override,
                "agent_hostname": o.agent_hostname,
                "check_cmd_override": o.check_cmd_override,
                "start_cmd_override": o.start_cmd_override,
                "stop_cmd_override": o.stop_cmd_override,
                "rebuild_cmd_override": o.rebuild_cmd_override,
                "env_vars_override": o.env_vars_override,
                "created_at": o.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "overrides": data })))
}

#[derive(Debug, Deserialize)]
pub struct UpsertSiteOverrideRequest {
    pub agent_id_override: Option<Uuid>,
    pub check_cmd_override: Option<String>,
    pub start_cmd_override: Option<String>,
    pub stop_cmd_override: Option<String>,
    pub rebuild_cmd_override: Option<String>,
    pub env_vars_override: Option<Value>,
}

/// PUT /api/v1/components/:id/site-overrides/:site_id
///
/// Create or update a site override for a component.
pub async fn upsert_site_override(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, site_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpsertSiteOverrideRequest>,
) -> Result<Json<Value>, ApiError> {
    // First get the app_id to check permissions
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Verify site exists
    let _site_exists = sqlx::query_scalar::<_, DbUuid>("SELECT id FROM sites WHERE id = $1")
        .bind(crate::db::bind_id(site_id))
        .fetch_optional(&state.db)
        .await?
        .ok_or_not_found()?;

    // Log before execute
    log_action(
        &state.db,
        user.user_id,
        "upsert_site_override",
        "component",
        component_id,
        json!({
            "site_id": site_id,
            "agent_id_override": req.agent_id_override,
        }),
    )
    .await
    .ok();

    // Upsert
    let id = sqlx::query_scalar::<_, DbUuid>(
        r#"
        INSERT INTO site_overrides (component_id, site_id, agent_id_override,
            check_cmd_override, start_cmd_override, stop_cmd_override,
            rebuild_cmd_override, env_vars_override)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (component_id, site_id) DO UPDATE SET
            agent_id_override = EXCLUDED.agent_id_override,
            check_cmd_override = EXCLUDED.check_cmd_override,
            start_cmd_override = EXCLUDED.start_cmd_override,
            stop_cmd_override = EXCLUDED.stop_cmd_override,
            rebuild_cmd_override = EXCLUDED.rebuild_cmd_override,
            env_vars_override = EXCLUDED.env_vars_override
        RETURNING id
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(crate::db::bind_id(site_id))
    .bind(req.agent_id_override)
    .bind(&req.check_cmd_override)
    .bind(&req.start_cmd_override)
    .bind(&req.stop_cmd_override)
    .bind(&req.rebuild_cmd_override)
    .bind(&req.env_vars_override)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({
        "id": id,
        "component_id": component_id,
        "site_id": site_id,
    })))
}

/// DELETE /api/v1/components/:id/site-overrides/:site_id
///
/// Remove a site override for a component.
pub async fn delete_site_override(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, site_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    // First get the app_id to check permissions
    let app_id =
        sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(crate::db::bind_id(component_id))
            .fetch_optional(&state.db)
            .await?
            .ok_or_not_found()?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    log_action(
        &state.db,
        user.user_id,
        "delete_site_override",
        "component",
        component_id,
        json!({ "site_id": site_id }),
    )
    .await
    .ok();

    sqlx::query("DELETE FROM site_overrides WHERE component_id = $1 AND site_id = $2")
        .bind(crate::db::bind_id(component_id))
        .bind(crate::db::bind_id(site_id))
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
