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
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Deserialize)]
pub struct CreateComponentRequest {
    pub name: String,
    pub component_type: String,
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

#[derive(Debug, Deserialize)]
pub struct UpdateComponentRequest {
    pub name: Option<String>,
    pub component_type: Option<String>,
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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComponentRow {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub component_type: String,
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
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    let components = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, created_at, updated_at
        FROM components WHERE application_id = $1 ORDER BY name
        "#,
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "components": components })))
}

pub async fn get_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        SELECT id, application_id, name, component_type, agent_id, check_cmd, start_cmd, stop_cmd,
               check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
               position_x, position_y, created_at, updated_at
        FROM components WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, component.application_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(json!(component)))
}

pub async fn create_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateComponentRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    let comp_id = Uuid::new_v4();
    log_action(&state.db, user.user_id, "create_component", "component", comp_id, json!({"name": body.name}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        INSERT INTO components (id, application_id, name, component_type, agent_id, check_cmd, start_cmd, stop_cmd,
                                check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
                                position_x, position_y, env_vars, tags)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        RETURNING id, application_id, name, component_type, agent_id, check_cmd, start_cmd, stop_cmd,
                  check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
                  position_x, position_y, created_at, updated_at
        "#,
    )
    .bind(comp_id)
    .bind(app_id)
    .bind(&body.name)
    .bind(&body.component_type)
    .bind(&body.agent_id)
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
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!(component))))
}

pub async fn update_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateComponentRequest>,
) -> Result<Json<Value>, StatusCode> {
    // Get current component to check app permission
    let current = sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, current, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "update_component", "component", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let component = sqlx::query_as::<_, ComponentRow>(
        r#"
        UPDATE components SET
            name = COALESCE($2, name),
            component_type = COALESCE($3, component_type),
            check_cmd = COALESCE($4, check_cmd),
            start_cmd = COALESCE($5, start_cmd),
            stop_cmd = COALESCE($6, stop_cmd),
            check_interval_seconds = COALESCE($7, check_interval_seconds),
            start_timeout_seconds = COALESCE($8, start_timeout_seconds),
            stop_timeout_seconds = COALESCE($9, stop_timeout_seconds),
            is_optional = COALESCE($10, is_optional),
            position_x = COALESCE($11, position_x),
            position_y = COALESCE($12, position_y),
            updated_at = now()
        WHERE id = $1
        RETURNING id, application_id, name, component_type, agent_id, check_cmd, start_cmd, stop_cmd,
                  check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
                  position_x, position_y, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.component_type)
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
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!(component)))
}

pub async fn delete_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let app_id = sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "delete_component", "component", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM components WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let app_id = sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "start_component", "component", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Trigger FSM transition to Starting
    crate::core::fsm::transition_component(&state, id, appcontrol_common::ComponentState::Starting)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;

    Ok(Json(json!({ "status": "starting" })))
}

pub async fn stop_component(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let app_id = sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "stop_component", "component", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    crate::core::fsm::transition_component(&state, id, appcontrol_common::ComponentState::Stopping)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;

    Ok(Json(json!({ "status": "stopping" })))
}

pub async fn execute_command(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((id, cmd)): Path<(Uuid, String)>,
) -> Result<Json<Value>, StatusCode> {
    let app_id = sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Operate {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "execute_command", "component", id, json!({"command": cmd}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Look up the command from component_commands
    let command = sqlx::query_scalar::<_, String>(
        "SELECT command FROM component_commands WHERE component_id = $1 AND name = $2",
    )
    .bind(id)
    .bind(&cmd)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let request_id = Uuid::new_v4();
    Ok(Json(json!({ "request_id": request_id, "command": command, "status": "executing" })))
}

pub async fn list_dependencies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    let deps = sqlx::query_as::<_, DependencyRow>(
        "SELECT id, application_id, from_component_id, to_component_id, created_at FROM dependencies WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "dependencies": deps })))
}

pub async fn create_dependency(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateDependencyRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    // Check for cycles before inserting
    if let Err(_) = crate::core::dag::validate_no_cycle(&state.db, app_id, body.from_component_id, body.to_component_id).await {
        return Err(StatusCode::CONFLICT);
    }

    let dep_id = Uuid::new_v4();
    log_action(&state.db, user.user_id, "create_dependency", "dependency", dep_id, json!({"from": body.from_component_id, "to": body.to_component_id}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!(dep))))
}

pub async fn delete_dependency(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let app_id = sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM dependencies WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(&state.db, user.user_id, "delete_dependency", "dependency", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM dependencies WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
