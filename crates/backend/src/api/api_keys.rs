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
) -> Result<(StatusCode, Json<Value>), StatusCode> {
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
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        r#"
        INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, scopes, expires_at)
        VALUES ($1, $2, $3, encode(sha256($4::bytea), 'hex'), $5, $6, $7)
        "#,
    )
    .bind(key_id)
    .bind(user.user_id)
    .bind(&body.name)
    .bind(raw_key.as_bytes())
    .bind(key_prefix)
    .bind(&scopes)
    .bind(body.expires_at)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
) -> Result<Json<Value>, StatusCode> {
    let keys = sqlx::query_as::<_, (Uuid, String, String, Value, bool, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT id, name, key_prefix, scopes, is_active, expires_at, created_at
        FROM api_keys
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data: Vec<Value> = keys
        .iter()
        .map(|(id, name, prefix, scopes, active, expires, created)| {
            json!({
                "id": id,
                "name": name,
                "key_prefix": prefix,
                "scopes": scopes,
                "is_active": active,
                "expires_at": expires,
                "created_at": created,
            })
        })
        .collect();

    Ok(Json(json!(data)))
}

pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query(
        "UPDATE api_keys SET is_active = false WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user.user_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
