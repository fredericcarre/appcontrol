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
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
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

pub async fn list_teams(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let teams = state.team_repo.list_teams(*user.organization_id).await?;

    let result: Vec<Value> = teams
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id,
                "organization_id": t.organization_id,
                "name": t.name,
                "description": t.description,
                "created_at": t.created_at,
                "updated_at": t.updated_at,
            })
        })
        .collect();

    Ok(Json(json!({ "teams": result })))
}

pub async fn get_team(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let team = state.team_repo.get_team(id).await?.ok_or_not_found()?;

    Ok(Json(json!({
        "id": team.id,
        "organization_id": team.organization_id,
        "name": team.name,
        "description": team.description,
        "created_at": team.created_at,
        "updated_at": team.updated_at,
    })))
}

pub async fn create_team(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateTeamRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Input validation
    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let team_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_team",
        "team",
        team_id,
        json!({"name": body.name}),
    )
    .await?;

    let team = state
        .team_repo
        .create_team(
            team_id,
            *user.organization_id,
            &body.name,
            body.description.as_deref(),
            *user.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": team.id,
            "organization_id": team.organization_id,
            "name": team.name,
            "description": team.description,
            "created_at": team.created_at,
            "updated_at": team.updated_at,
        })),
    ))
}

pub async fn update_team(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTeamRequest>,
) -> Result<Json<Value>, ApiError> {
    // Input validation
    if let Some(ref name) = body.name {
        validate_length("name", name, 1, 200)?;
    }
    validate_optional_length("description", &body.description, 2000)?;

    log_action(
        &state.db,
        user.user_id,
        "update_team",
        "team",
        id,
        json!({}),
    )
    .await?;

    let team = state
        .team_repo
        .update_team(id, body.name.as_deref(), body.description.as_deref())
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!({
        "id": team.id,
        "organization_id": team.organization_id,
        "name": team.name,
        "description": team.description,
        "created_at": team.created_at,
        "updated_at": team.updated_at,
    })))
}

pub async fn delete_team(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    log_action(
        &state.db,
        user.user_id,
        "delete_team",
        "team",
        id,
        json!({}),
    )
    .await?;

    let deleted = state.team_repo.delete_team(id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let members = state.team_repo.list_members(id).await?;

    let result: Vec<Value> = members
        .into_iter()
        .map(|m| {
            let name = m.display_name.unwrap_or_else(|| m.email.clone());
            json!({
                "id": m.id,
                "user_id": m.user_id,
                "role": m.role,
                "joined_at": m.joined_at,
                "email": m.email,
                "name": name
            })
        })
        .collect();

    Ok(Json(json!({ "members": result })))
}

pub async fn add_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Only admins and team leads can add members
    if !user.is_admin() {
        let is_lead = state.team_repo.is_team_lead(id, *user.user_id).await?;
        if !is_lead {
            return Err(ApiError::Forbidden);
        }
    }

    log_action(
        &state.db,
        user.user_id,
        "add_team_member",
        "team",
        id,
        json!({"user_id": body.user_id}),
    )
    .await?;

    let member_id = state
        .team_repo
        .add_member(id, body.user_id, body.role.as_deref().unwrap_or("member"))
        .await?;

    Ok((StatusCode::CREATED, Json(json!({"id": member_id}))))
}

pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((team_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    // Only admins and team leads can remove members
    if !user.is_admin() {
        let is_lead = state.team_repo.is_team_lead(team_id, *user.user_id).await?;
        if !is_lead {
            return Err(ApiError::Forbidden);
        }
    }

    log_action(
        &state.db,
        user.user_id,
        "remove_team_member",
        "team",
        team_id,
        json!({"user_id": user_id}),
    )
    .await?;

    state.team_repo.remove_member(team_id, user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SearchTeamsQuery {
    pub q: Option<String>,
}

/// Search teams by name (for pickers/autocomplete).
pub async fn search_teams(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<SearchTeamsQuery>,
) -> Result<Json<Value>, ApiError> {
    let teams = state.team_repo.list_teams(*user.organization_id).await?;

    let query = params.q.unwrap_or_default().to_lowercase();
    let results: Vec<Value> = teams
        .into_iter()
        .filter(|t| query.is_empty() || t.name.to_lowercase().contains(&query))
        .take(20)
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "description": t.description,
            })
        })
        .collect();

    Ok(Json(json!({ "teams": results })))
}
