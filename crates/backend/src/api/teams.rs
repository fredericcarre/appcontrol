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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TeamRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_teams(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    #[cfg(feature = "postgres")]
    let teams = sqlx::query_as::<_, TeamRow>(
        "SELECT id, organization_id, name, description, created_at, updated_at FROM teams WHERE organization_id = $1 ORDER BY name",
    )
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let teams = sqlx::query_as::<_, TeamRow>(
        "SELECT id, organization_id, name, description, created_at, updated_at FROM teams WHERE organization_id = $1 ORDER BY name",
    )
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "teams": teams })))
}

pub async fn get_team(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    #[cfg(feature = "postgres")]
    let team = sqlx::query_as::<_, TeamRow>(
        "SELECT id, organization_id, name, description, created_at, updated_at FROM teams WHERE id = $1",
    )
    .bind(crate::db::bind_id(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let team = sqlx::query_as::<_, TeamRow>(
        "SELECT id, organization_id, name, description, created_at, updated_at FROM teams WHERE id = $1",
    )
    .bind(DbUuid::from(id))
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(team)))
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

    #[cfg(feature = "postgres")]
    let team = sqlx::query_as::<_, TeamRow>(
        r#"
        INSERT INTO teams (id, organization_id, name, description)
        VALUES ($1, $2, $3, $4)
        RETURNING id, organization_id, name, description, created_at, updated_at
        "#,
    )
    .bind(crate::db::bind_id(team_id))
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&body.name)
    .bind(&body.description)
    .fetch_one(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let team = sqlx::query_as::<_, TeamRow>(
        r#"
        INSERT INTO teams (id, organization_id, name, description)
        VALUES ($1, $2, $3, $4)
        RETURNING id, organization_id, name, description, created_at, updated_at
        "#,
    )
    .bind(DbUuid::from(team_id))
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&body.name)
    .bind(&body.description)
    .fetch_one(&state.db)
    .await?;

    // Add creator as team lead
    #[cfg(feature = "postgres")]
    let _ =
        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'lead')")
            .bind(crate::db::bind_id(team_id))
            .bind(crate::db::bind_id(user.user_id))
            .execute(&state.db)
            .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let _ = sqlx::query(
        "INSERT INTO team_members (id, team_id, user_id, role) VALUES ($1, $2, $3, 'lead')",
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(team_id))
    .bind(crate::db::bind_id(user.user_id))
    .execute(&state.db)
    .await;

    Ok((StatusCode::CREATED, Json(json!(team))))
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

    let update_sql = format!(
        "UPDATE teams SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                updated_at = {}
            WHERE id = $1
            RETURNING id, organization_id, name, description, created_at, updated_at",
        crate::db::sql::now()
    );

    #[cfg(feature = "postgres")]
    let team = sqlx::query_as::<_, TeamRow>(&update_sql)
        .bind(crate::db::bind_id(id))
        .bind(&body.name)
        .bind(&body.description)
        .fetch_optional(&state.db)
        .await?
        .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let team = sqlx::query_as::<_, TeamRow>(&update_sql)
        .bind(DbUuid::from(id))
        .bind(&body.name)
        .bind(&body.description)
        .fetch_optional(&state.db)
        .await?
        .ok_or_not_found()?;

    Ok(Json(json!(team)))
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

    #[cfg(feature = "postgres")]
    let result = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(crate::db::bind_id(id))
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    #[cfg(feature = "postgres")]
    let members = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            chrono::DateTime<chrono::Utc>,
            String,
            Option<String>,
        ),
    >(
        r#"
        SELECT tm.id, tm.user_id, tm.role, tm.joined_at, u.email, u.display_name
        FROM team_members tm
        JOIN users u ON u.id = tm.user_id
        WHERE tm.team_id = $1
        ORDER BY tm.role, u.display_name, u.email
        "#,
    )
    .bind(crate::db::bind_id(id))
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let members = sqlx::query_as::<
        _,
        (
            DbUuid,
            DbUuid,
            String,
            chrono::DateTime<chrono::Utc>,
            String,
            Option<String>,
        ),
    >(
        r#"
        SELECT tm.id, tm.user_id, tm.role, tm.joined_at, u.email, u.display_name
        FROM team_members tm
        JOIN users u ON u.id = tm.user_id
        WHERE tm.team_id = $1
        ORDER BY tm.role, u.display_name, u.email
        "#,
    )
    .bind(DbUuid::from(id))
    .fetch_all(&state.db)
    .await?;

    let result: Vec<Value> = members
        .iter()
        .map(|(mid, uid, role, joined, email, name)| {
            json!({
                "id": mid,
                "user_id": uid,
                "role": role,
                "joined_at": joined,
                "email": email,
                "name": name.clone().unwrap_or_else(|| email.clone())
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
        #[cfg(feature = "postgres")]
        let is_lead = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'lead')",
        )
        .bind(crate::db::bind_id(id))
        .bind(crate::db::bind_id(user.user_id))
        .fetch_one(&state.db)
        .await?;

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let is_lead = {
            let count = sqlx::query_scalar::<_, i32>(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'lead'",
            )
            .bind(DbUuid::from(id))
            .bind(crate::db::bind_id(user.user_id))
            .fetch_one(&state.db)
            .await?;
            count > 0
        };

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

    #[cfg(feature = "postgres")]
    let member_id = sqlx::query_scalar::<_, DbUuid>(
        r#"
        INSERT INTO team_members (team_id, user_id, role)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(crate::db::bind_id(id))
    .bind(body.user_id)
    .bind(body.role.as_deref().unwrap_or("member"))
    .fetch_one(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let member_id = {
        let new_id = DbUuid::new_v4();
        sqlx::query_scalar::<_, DbUuid>(
            r#"
            INSERT INTO team_members (id, team_id, user_id, role)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(new_id)
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(body.user_id))
        .bind(body.role.as_deref().unwrap_or("member"))
        .fetch_one(&state.db)
        .await?
    };

    Ok((StatusCode::CREATED, Json(json!({"id": member_id}))))
}

pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((team_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    // Only admins and team leads can remove members
    if !user.is_admin() {
        #[cfg(feature = "postgres")]
        let is_lead = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'lead')",
        )
        .bind(crate::db::bind_id(team_id))
        .bind(crate::db::bind_id(user.user_id))
        .fetch_one(&state.db)
        .await?;

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let is_lead = {
            let count = sqlx::query_scalar::<_, i32>(
                "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'lead'",
            )
            .bind(DbUuid::from(team_id))
            .bind(crate::db::bind_id(user.user_id))
            .fetch_one(&state.db)
            .await?;
            count > 0
        };

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

    #[cfg(feature = "postgres")]
    sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(crate::db::bind_id(team_id))
        .bind(crate::db::bind_id(user_id))
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
        .bind(DbUuid::from(team_id))
        .bind(DbUuid::from(user_id))
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
