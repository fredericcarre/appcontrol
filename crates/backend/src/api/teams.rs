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
use crate::middleware::audit::log_action;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTeamRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
    pub role: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TeamRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_teams(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, StatusCode> {
    let teams = sqlx::query_as::<_, TeamRow>(
        "SELECT id, organization_id, name, description, created_at, updated_at FROM teams WHERE organization_id = $1 ORDER BY name",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "teams": teams })))
}

pub async fn get_team(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let team = sqlx::query_as::<_, TeamRow>(
        "SELECT id, organization_id, name, description, created_at, updated_at FROM teams WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!(team)))
}

pub async fn create_team(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateTeamRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let team_id = Uuid::new_v4();
    log_action(&state.db, user.user_id, "create_team", "team", team_id, json!({"name": body.name}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let team = sqlx::query_as::<_, TeamRow>(
        r#"
        INSERT INTO teams (id, organization_id, name, description)
        VALUES ($1, $2, $3, $4)
        RETURNING id, organization_id, name, description, created_at, updated_at
        "#,
    )
    .bind(team_id)
    .bind(user.organization_id)
    .bind(&body.name)
    .bind(&body.description)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Add creator as team lead
    let _ = sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'lead')",
    )
    .bind(team_id)
    .bind(user.user_id)
    .execute(&state.db)
    .await;

    Ok((StatusCode::CREATED, Json(json!(team))))
}

pub async fn update_team(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTeamRequest>,
) -> Result<Json<Value>, StatusCode> {
    log_action(&state.db, user.user_id, "update_team", "team", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let team = sqlx::query_as::<_, TeamRow>(
        r#"
        UPDATE teams SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            updated_at = now()
        WHERE id = $1
        RETURNING id, organization_id, name, description, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.description)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!(team)))
}

pub async fn delete_team(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    log_action(&state.db, user.user_id, "delete_team", "team", id, json!({}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let members = sqlx::query_as::<_, (Uuid, Uuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, user_id, role, joined_at FROM team_members WHERE team_id = $1",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<Value> = members
        .iter()
        .map(|(mid, uid, role, joined)| json!({"id": mid, "user_id": uid, "role": role, "joined_at": joined}))
        .collect();

    Ok(Json(json!({ "members": result })))
}

pub async fn add_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    log_action(&state.db, user.user_id, "add_team_member", "team", id, json!({"user_id": body.user_id}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let member_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO team_members (team_id, user_id, role)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(body.user_id)
    .bind(body.role.as_deref().unwrap_or("member"))
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(json!({"id": member_id}))))
}

pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((team_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    log_action(&state.db, user.user_id, "remove_team_member", "team", team_id, json!({"user_id": user_id}))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(team_id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}
