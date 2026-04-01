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
use crate::db::DbUuid;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GroupRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
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
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    #[cfg(feature = "postgres")]
    let groups = sqlx::query_as::<_, GroupRow>(
        "SELECT id, application_id, name, description, color, display_order, created_at \
         FROM component_groups WHERE application_id = $1 ORDER BY display_order, name",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let groups = sqlx::query_as::<_, GroupRow>(
        "SELECT id, application_id, name, description, color, display_order, created_at \
         FROM component_groups WHERE application_id = $1 ORDER BY display_order, name",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "groups": groups })))
}

/// Create a new component group.
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let group_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_group",
        "component_group",
        group_id,
        json!({"name": body.name, "app_id": app_id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    let group = sqlx::query_as::<_, GroupRow>(
        r#"
        INSERT INTO component_groups (id, application_id, name, description, color, display_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, application_id, name, description, color, display_order, created_at
        "#,
    )
    .bind(group_id)
    .bind(crate::db::bind_id(app_id))
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.color.as_deref().unwrap_or("#6366F1"))
    .bind(body.display_order.unwrap_or(0))
    .fetch_one(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let group = sqlx::query_as::<_, GroupRow>(
        r#"
        INSERT INTO component_groups (id, application_id, name, description, color, display_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, application_id, name, description, color, display_order, created_at
        "#,
    )
    .bind(DbUuid::from(group_id))
    .bind(DbUuid::from(app_id))
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.color.as_deref().unwrap_or("#6366F1"))
    .bind(body.display_order.unwrap_or(0))
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(json!(group))))
}

/// Update a component group.
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, group_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    if let Some(ref name) = body.name {
        validate_length("name", name, 1, 200)?;
    }
    validate_optional_length("description", &body.description, 2000)?;

    log_action(
        &state.db,
        user.user_id,
        "update_group",
        "component_group",
        group_id,
        json!({"app_id": app_id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
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
    .bind(crate::db::bind_id(app_id))
    .bind(group_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.color)
    .bind(body.display_order)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
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
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(group_id))
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.color)
    .bind(body.display_order)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(group)))
}

/// Delete a component group.
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, group_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_group",
        "component_group",
        group_id,
        json!({"app_id": app_id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    let result = sqlx::query("DELETE FROM component_groups WHERE id = $1 AND application_id = $2")
        .bind(group_id)
        .bind(crate::db::bind_id(app_id))
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query("DELETE FROM component_groups WHERE id = $1 AND application_id = $2")
        .bind(DbUuid::from(group_id))
        .bind(DbUuid::from(app_id))
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
