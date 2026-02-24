//! Air-gap agent update API.
//!
//! Endpoints:
//! - POST /api/v1/admin/agent-binaries          — upload a new agent binary
//! - GET  /api/v1/admin/agent-binaries           — list uploaded binaries
//! - POST /api/v1/admin/agents/:id/update        — push update to a specific agent
//! - POST /api/v1/admin/agents/update-batch      — push update to multiple agents
//! - GET  /api/v1/admin/agent-update-tasks       — list update tasks

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

const CHUNK_SIZE: usize = 256 * 1024; // 256KB per chunk

/// Upload a new agent binary for air-gap distribution.
#[derive(Debug, Deserialize)]
pub struct UploadBinaryRequest {
    pub version: String,
    pub platform: Option<String>,
    /// Base64-encoded agent binary.
    pub binary_base64: String,
    pub checksum_sha256: String,
}

pub async fn upload_binary(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<UploadBinaryRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let binary = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &body.binary_base64,
    )
    .map_err(|e| ApiError::Validation(format!("Invalid base64: {}", e)))?;

    // Verify checksum
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&binary);
    let computed = hex::encode(hash);
    if computed != body.checksum_sha256 {
        return Err(ApiError::Validation(format!(
            "Checksum mismatch: expected {}, got {}",
            body.checksum_sha256, computed
        )));
    }

    let platform = body.platform.unwrap_or_else(|| "linux-amd64".to_string());
    let id = Uuid::new_v4();

    log_action(
        &state.db,
        user.user_id,
        "agent_binary_upload",
        "agent_binary",
        id,
        json!({
            "version": &body.version,
            "platform": &platform,
            "size": binary.len(),
        }),
    )
    .await?;

    sqlx::query(
        "INSERT INTO agent_binaries (id, version, platform, checksum_sha256, size_bytes, binary_data, uploaded_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&body.version)
    .bind(&platform)
    .bind(&body.checksum_sha256)
    .bind(binary.len() as i64)
    .bind(&binary)
    .bind(user.user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "id": id,
        "version": body.version,
        "platform": platform,
        "size_bytes": binary.len(),
    })))
}

/// List uploaded agent binaries.
pub async fn list_binaries(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = sqlx::query_as::<_, (Uuid, String, String, String, i64, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, version, platform, checksum_sha256, size_bytes, uploaded_at
         FROM agent_binaries ORDER BY uploaded_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let binaries: Vec<Value> = rows
        .iter()
        .map(|(id, version, platform, checksum, size, uploaded_at)| {
            json!({
                "id": id,
                "version": version,
                "platform": platform,
                "checksum_sha256": checksum,
                "size_bytes": size,
                "uploaded_at": uploaded_at,
            })
        })
        .collect();

    Ok(Json(json!({ "binaries": binaries })))
}

/// Push an update to a specific agent via WebSocket binary chunks.
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub version: String,
}

pub async fn push_update_to_agent(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<UpdateAgentRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Get the binary
    let binary_row = sqlx::query_as::<_, (Vec<u8>, String, i64)>(
        "SELECT binary_data, checksum_sha256, size_bytes FROM agent_binaries WHERE version = $1",
    )
    .bind(&body.version)
    .fetch_optional(&state.db)
    .await?;

    let (binary_data, checksum, size) = binary_row.ok_or(ApiError::NotFound)?;

    let update_id = Uuid::new_v4();
    let total_chunks = (size as usize).div_ceil(CHUNK_SIZE) as u32;

    log_action(
        &state.db,
        user.user_id,
        "agent_update_push",
        "agent",
        agent_id,
        json!({
            "update_id": update_id,
            "version": &body.version,
            "total_chunks": total_chunks,
        }),
    )
    .await?;

    // Create tracking task
    sqlx::query(
        "INSERT INTO agent_update_tasks (id, agent_id, target_version, status, total_chunks)
         VALUES ($1, $2, $3, 'in_progress', $4)",
    )
    .bind(update_id)
    .bind(agent_id)
    .bind(&body.version)
    .bind(total_chunks as i32)
    .execute(&state.db)
    .await?;

    // Send chunks via WebSocket in background
    let state_clone = state.clone();
    let version = body.version.clone();
    tokio::spawn(async move {
        for i in 0..total_chunks {
            let start = i as usize * CHUNK_SIZE;
            let end = std::cmp::min(start + CHUNK_SIZE, binary_data.len());
            let chunk = &binary_data[start..end];
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                chunk,
            );

            let msg = appcontrol_common::BackendMessage::UpdateBinaryChunk {
                update_id,
                target_version: version.clone(),
                checksum_sha256: checksum.clone(),
                chunk_index: i,
                total_chunks,
                total_size: size as u64,
                data: encoded,
            };

            if !state_clone.ws_hub.send_to_agent(agent_id, msg) {
                tracing::error!(
                    agent_id = %agent_id,
                    chunk = i,
                    "Failed to send binary chunk — agent unreachable"
                );
                let _ = sqlx::query(
                    "UPDATE agent_update_tasks SET status = 'failed', error = 'Agent unreachable', completed_at = now() WHERE id = $1",
                )
                .bind(update_id)
                .execute(&state_clone.db)
                .await;
                return;
            }

            // Update progress
            let _ = sqlx::query(
                "UPDATE agent_update_tasks SET chunks_sent = $2 WHERE id = $1",
            )
            .bind(update_id)
            .bind((i + 1) as i32)
            .execute(&state_clone.db)
            .await;

            // Small delay between chunks to avoid overwhelming the WebSocket
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        tracing::info!(
            agent_id = %agent_id,
            update_id = %update_id,
            "All {} chunks sent for version {}",
            total_chunks,
            version
        );
    });

    Ok(Json(json!({
        "update_id": update_id,
        "agent_id": agent_id,
        "version": body.version,
        "total_chunks": total_chunks,
        "status": "in_progress",
    })))
}

/// Push update to multiple agents (batch).
#[derive(Debug, Deserialize)]
pub struct BatchUpdateRequest {
    pub version: String,
    pub agent_ids: Vec<Uuid>,
}

pub async fn push_update_batch(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<BatchUpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "agent_update_batch",
        "system",
        Uuid::nil(),
        json!({
            "version": &body.version,
            "agent_count": body.agent_ids.len(),
        }),
    )
    .await?;

    let mut results = Vec::new();
    for agent_id in &body.agent_ids {
        let req = UpdateAgentRequest {
            version: body.version.clone(),
        };
        match push_update_to_agent(
            State(state.clone()),
            Extension(user.clone()),
            Path(*agent_id),
            Json(req),
        )
        .await
        {
            Ok(Json(v)) => results.push(v),
            Err(e) => results.push(json!({
                "agent_id": agent_id,
                "error": e.to_string(),
            })),
        }
    }

    Ok(Json(json!({ "updates": results })))
}

/// List agent update tasks.
pub async fn list_update_tasks(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, i32, i32, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, agent_id, target_version, status, chunks_sent, total_chunks, error, started_at
         FROM agent_update_tasks
         ORDER BY started_at DESC
         LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    let tasks: Vec<Value> = rows
        .iter()
        .map(|(id, agent_id, version, status, sent, total, error, started_at)| {
            json!({
                "id": id,
                "agent_id": agent_id,
                "target_version": version,
                "status": status,
                "chunks_sent": sent,
                "total_chunks": total,
                "progress_pct": if *total > 0 { (*sent * 100 / *total) as u32 } else { 0 },
                "error": error,
                "started_at": started_at,
            })
        })
        .collect();

    Ok(Json(json!({ "tasks": tasks })))
}
