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
pub struct LinkRow {
    pub id: Uuid,
    pub component_id: Uuid,
    pub label: String,
    pub url: String,
    pub link_type: String,
    pub display_order: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLinkRequest {
    pub label: String,
    pub url: String,
    pub link_type: Option<String>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLinkRequest {
    pub label: Option<String>,
    pub url: Option<String>,
    pub link_type: Option<String>,
    pub display_order: Option<i32>,
}

/// List all links for a component.
pub async fn list_links(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(StatusCode::FORBIDDEN);
    }

    let links = sqlx::query_as::<_, LinkRow>(
        "SELECT id, component_id, label, url, link_type, display_order, created_at \
         FROM component_links WHERE component_id = $1 ORDER BY display_order, label",
    )
    .bind(component_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "links": links })))
}

/// Create a new component link.
pub async fn create_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(component_id): Path<Uuid>,
    Json(body): Json<CreateLinkRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    let link_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_link",
        "component_link",
        link_id,
        json!({"label": body.label, "component_id": component_id}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let link = sqlx::query_as::<_, LinkRow>(
        r#"
        INSERT INTO component_links (id, component_id, label, url, link_type, display_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, component_id, label, url, link_type, display_order, created_at
        "#,
    )
    .bind(link_id)
    .bind(component_id)
    .bind(&body.label)
    .bind(&body.url)
    .bind(body.link_type.as_deref().unwrap_or("documentation"))
    .bind(body.display_order.unwrap_or(0))
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!(link))))
}

/// Update a component link.
pub async fn update_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, link_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateLinkRequest>,
) -> Result<Json<Value>, StatusCode> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(
        &state.db,
        user.user_id,
        "update_link",
        "component_link",
        link_id,
        json!({"component_id": component_id}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let link = sqlx::query_as::<_, LinkRow>(
        r#"
        UPDATE component_links SET
            label = COALESCE($3, label),
            url = COALESCE($4, url),
            link_type = COALESCE($5, link_type),
            display_order = COALESCE($6, display_order)
        WHERE id = $2 AND component_id = $1
        RETURNING id, component_id, label, url, link_type, display_order, created_at
        "#,
    )
    .bind(component_id)
    .bind(link_id)
    .bind(&body.label)
    .bind(&body.url)
    .bind(&body.link_type)
    .bind(body.display_order)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!(link)))
}

/// Delete a component link.
pub async fn delete_link(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((component_id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let app_id =
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(StatusCode::FORBIDDEN);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_link",
        "component_link",
        link_id,
        json!({"component_id": component_id}),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM component_links WHERE id = $1 AND component_id = $2")
        .bind(link_id)
        .bind(component_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
