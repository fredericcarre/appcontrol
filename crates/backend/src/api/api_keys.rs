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

    #[cfg(feature = "postgres")]
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
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(raw_key.as_bytes());
        let key_hash = hex::encode(hasher.finalize());
        sqlx::query(
            r#"
            INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, scopes, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(DbUuid::from(key_id))
        .bind(DbUuid::from(user.user_id))
        .bind(&body.name)
        .bind(&key_hash)
        .bind(key_prefix)
        .bind(&scopes)
        .bind(body.expires_at)
        .execute(&state.db)
        .await?;
    }

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
    let keys = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            Value,
            bool,
            Option<chrono::DateTime<chrono::Utc>>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT id, name, key_prefix, scopes, is_active, expires_at, created_at
        FROM api_keys
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;

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
) -> Result<StatusCode, ApiError> {
    #[cfg(feature = "postgres")]
    let result =
        sqlx::query("UPDATE api_keys SET is_active = false WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.user_id)
            .execute(&state.db)
            .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result =
        sqlx::query("UPDATE api_keys SET is_active = 0 WHERE id = $1 AND user_id = $2")
            .bind(DbUuid::from(id))
            .bind(DbUuid::from(user.user_id))
            .execute(&state.db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
