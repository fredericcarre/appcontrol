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

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateApprovalRequest {
    pub operation_type: String, // start, stop, switchover, rebuild
    pub resource_type: String,  // application, component
    pub resource_id: Uuid,
    pub reason: Option<String>,
    pub payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ApprovalDecisionRequest {
    pub decision: String, // approved, rejected
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ApprovalRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub operation_type: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub risk_level: String,
    pub requested_by: Uuid,
    pub request_payload: Value,
    pub status: String,
    pub required_approvals: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ============================================================================
// Risk Classification
// ============================================================================

fn classify_risk(operation_type: &str) -> &'static str {
    match operation_type {
        "diagnose" | "check" => "low",
        "start" | "stop" | "restart" => "medium",
        "switchover" | "rebuild" => "high",
        "break_glass" | "dr_commit" => "critical",
        _ => "medium",
    }
}

fn default_timeout_minutes(risk_level: &str) -> i32 {
    match risk_level {
        "low" => 5,
        "medium" => 15,
        "high" => 30,
        "critical" => 60,
        _ => 15,
    }
}

fn required_approvals_for_risk(risk_level: &str) -> i32 {
    match risk_level {
        "low" => 0,      // No approval needed
        "medium" => 1,   // Configurable
        "high" => 1,     // Required
        "critical" => 2, // Two approvers
        _ => 1,
    }
}

// ============================================================================
// Check if approval is required for an operation
// ============================================================================

/// Check whether an operation requires approval based on org policies.
/// Returns None if no approval needed, or Some(risk_level) if approval is required.
pub async fn check_approval_required(
    pool: &sqlx::PgPool,
    organization_id: Uuid,
    operation_type: &str,
) -> Option<String> {
    let risk_level = classify_risk(operation_type);

    // Check org-specific policy
    let policy = sqlx::query_as::<_, (bool,)>(
        "SELECT enabled FROM approval_policies WHERE organization_id = $1 AND operation_type = $2",
    )
    .bind(organization_id)
    .bind(operation_type)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match policy {
        Some((true,)) => Some(risk_level.to_string()),
        Some((false,)) => None,
        None => {
            // Default: require approval for high and critical
            if risk_level == "high" || risk_level == "critical" {
                Some(risk_level.to_string())
            } else {
                None
            }
        }
    }
}

// ============================================================================
// API Handlers
// ============================================================================

/// Create an approval request for a critical operation.
pub async fn create_approval_request(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateApprovalRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let risk_level = classify_risk(&body.operation_type);
    let timeout = default_timeout_minutes(risk_level);
    let required = required_approvals_for_risk(risk_level);

    let request_id = Uuid::new_v4();

    // Log before execute
    log_action(
        &state.db,
        user.user_id,
        "create_approval_request",
        &body.resource_type,
        body.resource_id,
        json!({
            "operation_type": body.operation_type,
            "risk_level": risk_level,
        }),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, ApprovalRow>(
        r#"
        INSERT INTO approval_requests (
            id, organization_id, operation_type, resource_type, resource_id,
            risk_level, requested_by, request_payload, required_approvals, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now() + make_interval(mins => $10))
        RETURNING id, organization_id, operation_type, resource_type, resource_id,
                  risk_level, requested_by, request_payload, status, required_approvals,
                  created_at, expires_at, resolved_at
        "#,
    )
    .bind(request_id)
    .bind(user.organization_id)
    .bind(&body.operation_type)
    .bind(&body.resource_type)
    .bind(body.resource_id)
    .bind(risk_level)
    .bind(user.user_id)
    .bind(body.payload.as_ref().unwrap_or(&json!({})))
    .bind(required)
    .bind(timeout)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Broadcast approval request event via WebSocket
    if body.resource_type == "application" {
        state.ws_hub.broadcast(
            body.resource_id,
            appcontrol_common::WsEvent::SwitchoverProgress {
                app_id: body.resource_id,
                phase: "approval_requested".to_string(),
                status: "pending".to_string(),
                message: format!(
                    "Approval required for {} (risk: {})",
                    body.operation_type, risk_level
                ),
            },
        );
    }

    Ok((StatusCode::CREATED, Json(json!(row))))
}

/// List pending approval requests for the user's organization.
pub async fn list_approval_requests(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, StatusCode> {
    let requests = sqlx::query_as::<_, ApprovalRow>(
        r#"
        SELECT id, organization_id, operation_type, resource_type, resource_id,
               risk_level, requested_by, request_payload, status, required_approvals,
               created_at, expires_at, resolved_at
        FROM approval_requests
        WHERE organization_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "requests": requests })))
}

/// Approve or reject an approval request. 4-eyes: requester cannot approve their own request.
pub async fn decide_approval(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(request_id): Path<Uuid>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> Result<Json<Value>, StatusCode> {
    // Fetch the request
    let request = sqlx::query_as::<_, ApprovalRow>(
        r#"
        SELECT id, organization_id, operation_type, resource_type, resource_id,
               risk_level, requested_by, request_payload, status, required_approvals,
               created_at, expires_at, resolved_at
        FROM approval_requests
        WHERE id = $1 AND organization_id = $2
        "#,
    )
    .bind(request_id)
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // 4-eyes: requester cannot approve their own request
    if request.requested_by == user.user_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Must be pending
    if request.status != "pending" {
        return Err(StatusCode::CONFLICT);
    }

    // Check if expired
    if request.expires_at < chrono::Utc::now() {
        let _ = sqlx::query(
            "UPDATE approval_requests SET status = 'expired', resolved_at = now() WHERE id = $1",
        )
        .bind(request_id)
        .execute(&state.db)
        .await;
        return Err(StatusCode::GONE);
    }

    // Check approver has sufficient permission on the resource
    let perm = effective_permission(
        &state.db,
        user.user_id,
        request.resource_id,
        user.is_admin(),
    )
    .await;
    if perm < PermissionLevel::Operate {
        return Err(StatusCode::FORBIDDEN);
    }

    // Record the decision
    let decision_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO approval_decisions (id, request_id, decided_by, decision, reason) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(decision_id)
    .bind(request_id)
    .bind(user.user_id)
    .bind(&body.decision)
    .bind(&body.reason)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Count approvals
    let approval_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM approval_decisions WHERE request_id = $1 AND decision = 'approved'",
    )
    .bind(request_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let new_status = if body.decision == "rejected" {
        "rejected"
    } else if approval_count >= request.required_approvals as i64 {
        "approved"
    } else {
        "pending" // Still waiting for more approvals
    };

    if new_status != "pending" {
        sqlx::query("UPDATE approval_requests SET status = $2, resolved_at = now() WHERE id = $1")
            .bind(request_id)
            .bind(new_status)
            .execute(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    log_action(
        &state.db,
        user.user_id,
        &format!("approval_{}", body.decision),
        "approval_request",
        request_id,
        json!({
            "operation_type": request.operation_type,
            "resource_id": request.resource_id,
            "new_status": new_status,
        }),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "request_id": request_id,
        "decision": body.decision,
        "status": new_status,
        "approvals": approval_count,
        "required": request.required_approvals,
    })))
}

/// List approval policies for the organization.
pub async fn list_approval_policies(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, StatusCode> {
    if !user.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let policies = sqlx::query_as::<_, (Uuid, String, String, i32, i32, bool)>(
        "SELECT id, operation_type, risk_level, required_approvals, timeout_minutes, enabled \
         FROM approval_policies WHERE organization_id = $1 ORDER BY operation_type",
    )
    .bind(user.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<Value> = policies
        .into_iter()
        .map(|(id, op, risk, req, timeout, enabled)| {
            json!({
                "id": id,
                "operation_type": op,
                "risk_level": risk,
                "required_approvals": req,
                "timeout_minutes": timeout,
                "enabled": enabled,
            })
        })
        .collect();

    Ok(Json(json!({ "policies": result })))
}

/// Create or update an approval policy for an operation type.
#[derive(Debug, Deserialize)]
pub struct UpsertPolicyRequest {
    pub operation_type: String,
    pub required_approvals: Option<i32>,
    pub timeout_minutes: Option<i32>,
    pub enabled: Option<bool>,
}

pub async fn upsert_approval_policy(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<UpsertPolicyRequest>,
) -> Result<Json<Value>, StatusCode> {
    if !user.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let risk_level = classify_risk(&body.operation_type);
    let required = body
        .required_approvals
        .unwrap_or_else(|| required_approvals_for_risk(risk_level));
    let timeout = body
        .timeout_minutes
        .unwrap_or_else(|| default_timeout_minutes(risk_level));
    let enabled = body.enabled.unwrap_or(true);

    sqlx::query(
        r#"
        INSERT INTO approval_policies (organization_id, operation_type, risk_level, required_approvals, timeout_minutes, enabled)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (organization_id, operation_type)
        DO UPDATE SET risk_level = $3, required_approvals = $4, timeout_minutes = $5, enabled = $6
        "#,
    )
    .bind(user.organization_id)
    .bind(&body.operation_type)
    .bind(risk_level)
    .bind(required)
    .bind(timeout)
    .bind(enabled)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    log_action(
        &state.db,
        user.user_id,
        "upsert_approval_policy",
        "organization",
        user.organization_id,
        json!({ "operation_type": body.operation_type, "enabled": enabled }),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "operation_type": body.operation_type,
        "risk_level": risk_level,
        "required_approvals": required,
        "timeout_minutes": timeout,
        "enabled": enabled,
    })))
}
