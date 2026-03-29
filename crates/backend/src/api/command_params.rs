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
use crate::db::DbUuid;
use crate::core::permissions::effective_permission;
use crate::error::{validate_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::AppState;
use appcontrol_common::PermissionLevel;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InputParamRow {
    pub id: DbUuid,
    pub command_id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
    pub required: bool,
    pub display_order: i32,
    pub param_type: String,
    pub enum_values: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInputParamRequest {
    pub name: String,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
    pub required: Option<bool>,
    pub display_order: Option<i32>,
    pub param_type: Option<String>,
    pub enum_values: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct UpdateInputParamRequest {
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub validation_regex: Option<String>,
    pub required: Option<bool>,
    pub display_order: Option<i32>,
    pub param_type: Option<String>,
    pub enum_values: Option<serde_json::Value>,
}

/// Resolve the application_id for a command through the component chain.
async fn app_id_for_command(db: &crate::db::DbPool, command_id: DbUuid) -> Result<Uuid, ApiError> {
    sqlx::query_scalar::<_, DbUuid>(
        "SELECT c.application_id FROM component_commands cc \
         JOIN components c ON c.id = cc.component_id \
         WHERE cc.id = $1",
    )
    .bind(command_id)
    .fetch_optional(db)
    .await?
    .ok_or_not_found()
}

/// List all input parameters for a command.
pub async fn list_params(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(command_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let app_id = app_id_for_command(&state.db, DbUuid::from(command_id)).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    let params = sqlx::query_as::<_, InputParamRow>(
        "SELECT id, command_id, name, description, default_value, validation_regex, required, display_order, \
         param_type, enum_values, created_at \
         FROM command_input_params WHERE command_id = $1 ORDER BY display_order, name",
    )
    .bind(command_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "params": params })))
}

/// Create a new input parameter for a command.
pub async fn create_param(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(command_id): Path<Uuid>,
    Json(body): Json<CreateInputParamRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let app_id = app_id_for_command(&state.db, DbUuid::from(command_id)).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    // Input validation
    validate_length("name", &body.name, 1, 200)?;

    // Validate the regex is valid if provided
    if let Some(ref regex) = body.validation_regex {
        if regex::Regex::new(regex).is_err() {
            return Err(ApiError::Validation(format!(
                "Invalid regex pattern: {}",
                regex
            )));
        }
    }

    let param_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_input_param",
        "command_input_param",
        param_id,
        json!({"name": body.name, "command_id": command_id}),
    )
    .await?;

    let param_type = body.param_type.as_deref().unwrap_or("string");

    let param = sqlx::query_as::<_, InputParamRow>(
        r#"
        INSERT INTO command_input_params (id, command_id, name, description, default_value, validation_regex, required, display_order, param_type, enum_values)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id, command_id, name, description, default_value, validation_regex, required, display_order, param_type, enum_values, created_at
        "#,
    )
    .bind(param_id)
    .bind(command_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.default_value)
    .bind(&body.validation_regex)
    .bind(body.required.unwrap_or(true))
    .bind(body.display_order.unwrap_or(0))
    .bind(param_type)
    .bind(&body.enum_values)
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(json!(param))))
}

/// Delete an input parameter.
pub async fn delete_param(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((command_id, param_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let app_id = app_id_for_command(&state.db, DbUuid::from(command_id)).await?;
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Edit {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_input_param",
        "command_input_param",
        param_id,
        json!({"command_id": command_id}),
    )
    .await?;

    let result = sqlx::query("DELETE FROM command_input_params WHERE id = $1 AND command_id = $2")
        .bind(param_id)
        .bind(command_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Validate input parameter values against their definitions.
/// Returns Ok(interpolated_command) or Err with validation errors.
#[allow(dead_code)]
pub fn validate_and_interpolate_params(
    command: &str,
    params: &[InputParamRow],
    values: &std::collections::HashMap<String, String>,
) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let mut result = command.to_string();

    for param in params {
        let value = values.get(&param.name).or(param.default_value.as_ref());

        match value {
            None if param.required => {
                errors.push(format!("Missing required parameter: {}", param.name));
            }
            Some(val) => {
                // Validate against regex if specified
                if let Some(ref regex_str) = param.validation_regex {
                    if let Ok(re) = regex::Regex::new(regex_str) {
                        if !re.is_match(val) {
                            errors.push(format!(
                                "Parameter '{}' value '{}' does not match pattern '{}'",
                                param.name, val, regex_str
                            ));
                        }
                    }
                }
                // Interpolate $(param_name) in command
                let pattern = format!("$({})", param.name);
                result = result.replace(&pattern, val);
            }
            None => {
                // Optional param with no value — leave pattern or remove
                let pattern = format!("$({})", param.name);
                result = result.replace(&pattern, "");
            }
        }
    }

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_param(
        name: &str,
        required: bool,
        default: Option<&str>,
        regex: Option<&str>,
    ) -> InputParamRow {
        InputParamRow {
            id: Uuid::new_v4(),
            command_id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            default_value: default.map(|s| s.to_string()),
            validation_regex: regex.map(|s| s.to_string()),
            required,
            display_order: 0,
            param_type: "string".to_string(),
            enum_values: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_validate_required_param_present() {
        let params = vec![make_param("days", true, None, Some(r"^\d+$"))];
        let mut values = HashMap::new();
        values.insert("days".to_string(), "30".to_string());

        let result = validate_and_interpolate_params("purge --days=$(days)", &params, &values);
        assert_eq!(result.unwrap(), "purge --days=30");
    }

    #[test]
    fn test_validate_required_param_missing() {
        let params = vec![make_param("days", true, None, None)];
        let values = HashMap::new();

        let result = validate_and_interpolate_params("purge --days=$(days)", &params, &values);
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("Missing required"));
    }

    #[test]
    fn test_validate_default_value() {
        let params = vec![make_param("days", true, Some("30"), None)];
        let values = HashMap::new();

        let result = validate_and_interpolate_params("purge --days=$(days)", &params, &values);
        assert_eq!(result.unwrap(), "purge --days=30");
    }

    #[test]
    fn test_validate_regex_failure() {
        let params = vec![make_param("days", true, None, Some(r"^\d+$"))];
        let mut values = HashMap::new();
        values.insert("days".to_string(), "abc".to_string());

        let result = validate_and_interpolate_params("purge --days=$(days)", &params, &values);
        assert!(result.is_err());
        assert!(result.unwrap_err()[0].contains("does not match"));
    }

    #[test]
    fn test_validate_optional_param_absent() {
        let params = vec![make_param("verbose", false, None, None)];
        let values = HashMap::new();

        let result = validate_and_interpolate_params("cmd $(verbose)", &params, &values);
        assert_eq!(result.unwrap(), "cmd ");
    }
}
