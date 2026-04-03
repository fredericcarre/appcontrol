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
#[allow(unused_imports)]
use crate::db::DbUuid;
use crate::error::{validate_length, ApiError};
use crate::middleware::audit::log_action;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub allowed_actions: Option<Vec<String>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // Input validation
    validate_length("name", &body.name, 1, 200)?;

    let key_id = Uuid::new_v4();
    let raw_key = format!("ac_{}", Uuid::new_v4().simple());
    let key_prefix = &raw_key[..10];

    let scopes = body
        .allowed_actions
        .as_ref()
        .map(|a| json!(a))
        .unwrap_or(json!(["*"]));

    log_action(
        &state.db,
        user.user_id,
        "create_api_key",
        "api_key",
        key_id,
        json!({"name": body.name}),
    )
    .await?;

    crate::repository::misc_queries::create_api_key(
        &state.db,
        key_id,
        *user.user_id,
        &body.name,
        raw_key.as_bytes(),
        key_prefix,
        &scopes,
        body.expires_at,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": key_id,
            "name": body.name,
            "key": raw_key,
            "key_prefix": key_prefix,
            "scopes": scopes,
        })),
    ))
}

pub async fn list_api_keys(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let keys = crate::repository::misc_queries::list_api_keys(&state.db, *user.user_id).await?;

    let data: Vec<Value> = keys
        .iter()
        .map(|k| {
            let scopes_val: Value = serde_json::from_str(&k.scopes).unwrap_or(json!([]));
            json!({
                "id": k.id,
                "name": k.name,
                "key_prefix": k.key_prefix,
                "scopes": scopes_val,
                "is_active": k.is_active,
                "expires_at": k.expires_at,
                "created_at": k.created_at,
            })
        })
        .collect();

    Ok(Json(json!(data)))
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let rows_affected =
        crate::repository::misc_queries::deactivate_api_key(&state.db, id, *user.user_id).await?;

    if rows_affected == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
