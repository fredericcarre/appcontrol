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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GroupRow {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: Option<i32>,
}

/// List all component groups for an application.
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    let groups = sqlx::query_as::<_, GroupRow>(
        "SELECT id, application_id, name, description, color, display_order, created_at \
         FROM component_groups WHERE application_id = $1 ORDER BY display_order, name",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "groups": groups })))
}

/// Create a new component group.
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    let group_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_group",
        "component_group",
        group_id,
        json!({"name": body.name, "app_id": app_id}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let group = sqlx::query_as::<_, GroupRow>(
        r#"
        INSERT INTO component_groups (id, application_id, name, description, color, display_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, application_id, name, description, color, display_order, created_at
        "#,
    )
    .bind(group_id)
    .bind(app_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.color.as_deref().unwrap_or("#6366F1"))
    .bind(body.display_order.unwrap_or(0))
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!(group))))
}

/// Update a component group.
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, group_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<Value>, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(
        &state.db,
        user.user_id,
        "update_group",
        "component_group",
        group_id,
        json!({"app_id": app_id}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let group = sqlx::query_as::<_, GroupRow>(
        r#"
        UPDATE component_groups SET
            name = COALESCE($3, name),
            description = COALESCE($4, description),
            color = COALESCE($5, color),
            display_order = COALESCE($6, display_order)
        WHERE id = $2 AND application_id = $1
        RETURNING id, application_id, name, description, color, display_order, created_at
        "#,
    )
    .bind(app_id)
    .bind(group_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.color)
    .bind(body.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!(group)))
}

/// Delete a component group.
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, group_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_group",
        "component_group",
        group_id,
        json!({"app_id": app_id}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM component_groups WHERE id = $1 AND application_id = $2")
        .bind(group_id)
        .bind(app_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
