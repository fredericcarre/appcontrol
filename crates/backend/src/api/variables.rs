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
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VariableRow {
    pub id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub is_secret: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVariableRequest {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub is_secret: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVariableRequest {
    pub value: Option<String>,
    pub description: Option<String>,
    pub is_secret: Option<bool>,
}

/// List all variables for an application.
/// Secret values are masked unless user has Edit+ permission.
pub async fn list_variables(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let variables = sqlx::query_as::<_, VariableRow>(
        "SELECT id, application_id, name, value, description, is_secret, created_at, updated_at \
         FROM app_variables WHERE application_id = $1 ORDER BY name",
    )
    .bind(app_id)
    .fetch_all(&state.db)
    .await?;

    // Mask secret values for users below Edit level
    let variables: Vec<Value> = variables
        .into_iter()
        .map(|v| {
            if v.is_secret && perm < PermissionLevel::Edit {
                json!({
                    "id": v.id,
                    "application_id": v.application_id,
                    "name": v.name,
                    "value": "********",
                    "description": v.description,
                    "is_secret": true,
                    "created_at": v.created_at,
                    "updated_at": v.updated_at,
                })
            } else {
                json!({
                    "id": v.id,
                    "application_id": v.application_id,
                    "name": v.name,
                    "value": v.value,
                    "description": v.description,
                    "is_secret": v.is_secret,
                    "created_at": v.created_at,
                    "updated_at": v.updated_at,
                })
            }
        })
        .collect();

    Ok(Json(json!({ "variables": variables })))
}

/// Create a new application variable.
pub async fn create_variable(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateVariableRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_length("name", &body.name, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let var_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_variable",
        "app_variable",
        var_id,
        json!({"name": body.name, "app_id": app_id}),
    )
    .await?;

    let variable = sqlx::query_as::<_, VariableRow>(
        r#"
        INSERT INTO app_variables (id, application_id, name, value, description, is_secret)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, application_id, name, value, description, is_secret, created_at, updated_at
        "#,
    )
    .bind(var_id)
    .bind(app_id)
    .bind(&body.name)
    .bind(&body.value)
    .bind(&body.description)
    .bind(body.is_secret.unwrap_or(false))
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(json!(variable))))
}

/// Update an application variable.
pub async fn update_variable(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, var_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateVariableRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_optional_length("description", &body.description, 2000)?;

    log_action(
        &state.db,
        user.user_id,
        "update_variable",
        "app_variable",
        var_id,
        json!({"app_id": app_id}),
    )
    .await?;

    let variable = sqlx::query_as::<_, VariableRow>(
        r#"
        UPDATE app_variables SET
            value = COALESCE($3, value),
            description = COALESCE($4, description),
            is_secret = COALESCE($5, is_secret),
            updated_at = now()
        WHERE id = $2 AND application_id = $1
        RETURNING id, application_id, name, value, description, is_secret, created_at, updated_at
        "#,
    )
    .bind(app_id)
    .bind(var_id)
    .bind(&body.value)
    .bind(&body.description)
    .bind(body.is_secret)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(variable)))
}

/// Delete an application variable.
pub async fn delete_variable(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, var_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_variable",
        "app_variable",
        var_id,
        json!({"app_id": app_id}),
    )
    .await?;

    let result = sqlx::query("DELETE FROM app_variables WHERE id = $1 AND application_id = $2")
        .bind(var_id)
        .bind(app_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Resolve all variables for an application into a HashMap.
/// Used by the executor to interpolate $(var) in commands.
#[allow(dead_code)]
pub async fn resolve_variables(
    db: &sqlx::PgPool,
    app_id: Uuid,
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT name, value FROM app_variables WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().collect())
}

/// Interpolate $(var_name) patterns in a command string using the provided variables.
#[allow(dead_code)]
pub fn interpolate_variables(
    command: &str,
    variables: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = command.to_string();
    for (name, value) in variables {
        let pattern = format!("$({})", name);
        result = result.replace(&pattern, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_interpolate_variables_basic() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "10.0.0.1".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());

        let cmd = "curl http://$(HOST):$(PORT)/health";
        let result = interpolate_variables(cmd, &vars);
        assert_eq!(result, "curl http://10.0.0.1:8080/health");
    }

    #[test]
    fn test_interpolate_variables_no_match() {
        let vars = HashMap::new();
        let cmd = "echo hello";
        let result = interpolate_variables(cmd, &vars);
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn test_interpolate_variables_multiple_occurrences() {
        let mut vars = HashMap::new();
        vars.insert("ENV".to_string(), "prod".to_string());

        let cmd = "deploy --env=$(ENV) --tag=$(ENV)-latest";
        let result = interpolate_variables(cmd, &vars);
        assert_eq!(result, "deploy --env=prod --tag=prod-latest");
    }

    #[test]
    fn test_interpolate_variables_unknown_left_as_is() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "server1".to_string());

        let cmd = "curl $(HOST):$(UNKNOWN_PORT)";
        let result = interpolate_variables(cmd, &vars);
        assert_eq!(result, "curl server1:$(UNKNOWN_PORT)");
    }
}
