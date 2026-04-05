//! Binding Profile Management API
//!
//! Endpoints for managing binding profiles:
//! - List profiles for an application
//! - Create a new profile
//! - Activate a profile (switchover)
//! - Delete a profile
//! - Manage DR pattern rules

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::DbUuid;
use crate::db::UuidArray;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

// ══════════════════════════════════════════════════════════════════════
// Data structures
// ══════════════════════════════════════════════════════════════════════

/// A binding profile
#[derive(Debug, Serialize, FromRow)]
pub struct BindingProfile {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub profile_type: String,
    pub is_active: bool,
    pub gateway_ids: UuidArray,
    pub auto_failover: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<DbUuid>,
}

/// Profile with mapping count
#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub profile_type: String,
    pub is_active: bool,
    pub gateway_ids: UuidArray,
    pub auto_failover: bool,
    pub mapping_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A binding profile mapping
#[derive(Debug, Serialize, FromRow)]
pub struct ProfileMapping {
    pub id: DbUuid,
    pub profile_id: DbUuid,
    pub component_name: String,
    pub host: String,
    pub agent_id: DbUuid,
    pub resolved_via: String,
}

/// Request to create a profile
#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub description: Option<String>,
    pub profile_type: String,
    pub gateway_ids: Vec<Uuid>,
    pub auto_failover: Option<bool>,
    /// Optional: copy mappings from another profile
    pub copy_from_profile_id: Option<DbUuid>,
    /// Manual mappings (if not copying)
    pub mappings: Option<Vec<CreateMappingRequest>>,
}

/// A mapping to create
#[derive(Debug, Deserialize)]
pub struct CreateMappingRequest {
    pub component_name: String,
    /// Host is optional — resolved from agent hostname when not provided
    #[serde(default)]
    pub host: Option<String>,
    pub agent_id: DbUuid,
    pub resolved_via: String,
}

/// DR Pattern Rule
#[derive(Debug, Serialize, FromRow)]
pub struct DrPatternRule {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub search_pattern: String,
    pub replace_pattern: String,
    pub priority: i32,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Request to create/update a DR pattern rule
#[derive(Debug, Deserialize)]
pub struct DrPatternRuleRequest {
    pub name: String,
    pub search_pattern: String,
    pub replace_pattern: String,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

// ══════════════════════════════════════════════════════════════════════
// Profile endpoints
// ══════════════════════════════════════════════════════════════════════

/// GET /api/v1/apps/:app_id/profiles
///
/// List all binding profiles for an application
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Vec<ProfileSummary>>, ApiError> {
    // Check permission
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if !perm.can_view() {
        return Err(ApiError::Forbidden);
    }

    #[derive(Debug, FromRow)]
    struct ProfileRow {
        id: DbUuid,
        name: String,
        description: Option<String>,
        profile_type: String,
        is_active: bool,
        gateway_ids: UuidArray,
        auto_failover: bool,
        created_at: chrono::DateTime<chrono::Utc>,
        mapping_count: Option<i64>,
    }

    let profiles: Vec<ProfileRow> =
        crate::repository::misc_queries::list_profiles_with_count(&state.db, app_id).await?;

    let summaries = profiles
        .into_iter()
        .map(|p| ProfileSummary {
            id: p.id,
            name: p.name,
            description: p.description,
            profile_type: p.profile_type,
            is_active: p.is_active,
            gateway_ids: p.gateway_ids,
            auto_failover: p.auto_failover,
            mapping_count: p.mapping_count.unwrap_or(0),
            created_at: p.created_at,
        })
        .collect();

    Ok(Json(summaries))
}

/// GET /api/v1/apps/:app_id/profiles/:name
///
/// Get a specific profile with its mappings
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, name)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Check permission
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if !perm.can_view() {
        return Err(ApiError::Forbidden);
    }

    let profile: Option<BindingProfile> =
        crate::repository::misc_queries::get_profile_by_name(&state.db, app_id, &name).await?;
    let profile = profile.ok_or(ApiError::NotFound)?;

    let mappings: Vec<ProfileMapping> =
        crate::repository::misc_queries::get_profile_mappings(&state.db, profile.id).await?;

    Ok(Json(json!({
        "profile": profile,
        "mappings": mappings
    })))
}

/// POST /api/v1/apps/:app_id/profiles
///
/// Create a new binding profile
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<CreateProfileRequest>,
) -> Result<(StatusCode, Json<BindingProfile>), ApiError> {
    // Check permission (need at least 'edit')
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if !perm.can_edit() {
        return Err(ApiError::Forbidden);
    }

    // Validate profile type
    if !["primary", "dr", "custom"].contains(&body.profile_type.as_str()) {
        return Err(ApiError::Validation(
            "profile_type must be 'primary', 'dr', or 'custom'".into(),
        ));
    }

    // Check if name already exists
    if crate::repository::misc_queries::profile_name_exists(&state.db, app_id, &body.name).await? {
        return Err(ApiError::Validation(format!(
            "Profile '{}' already exists",
            body.name
        )));
    }

    let profile_id = Uuid::new_v4();

    // Log action
    log_action(
        &state.db,
        user.user_id,
        "create_profile",
        "binding_profile",
        profile_id,
        json!({
            "application_id": app_id,
            "name": &body.name,
            "profile_type": &body.profile_type
        }),
    )
    .await?;

    // Create profile
    let profile: BindingProfile = crate::repository::misc_queries::create_binding_profile(
        &state.db,
        profile_id,
        app_id,
        &body.name,
        body.description.as_deref(),
        &body.profile_type,
        &UuidArray::from(body.gateway_ids.clone()),
        body.auto_failover.unwrap_or(false),
        *user.user_id,
    )
    .await?;

    // Copy mappings from another profile if specified
    if let Some(copy_from_id) = body.copy_from_profile_id {
        crate::repository::misc_queries::copy_profile_mappings(&state.db, profile_id, copy_from_id)
            .await?;
    } else if let Some(ref mappings) = body.mappings {
        // Create manual mappings
        for m in mappings {
            // Resolve host from agent hostname if not provided
            let host = match &m.host {
                Some(h) if !h.is_empty() => h.clone(),
                _ => {
                    // Look up agent hostname
                    crate::repository::misc_queries::get_agent_hostname(&state.db, *m.agent_id)
                        .await?
                        .unwrap_or_default()
                }
            };
            crate::repository::misc_queries::insert_profile_mapping(
                &state.db,
                profile_id,
                &m.component_name,
                &host,
                m.agent_id,
                &m.resolved_via,
            )
            .await?;
        }
    }

    // Auto-activate the first primary profile so agents start checking immediately.
    // Best-effort: if activation fails (e.g. missing FK references), the profile
    // is still created and can be activated manually later.
    if body.profile_type == "primary" {
        let active_exists =
            crate::repository::misc_queries::get_active_profile_name(&state.db, app_id)
                .await
                .ok()
                .flatten()
                .is_some();
        if !active_exists {
            if let Err(e) =
                crate::repository::misc_queries::activate_profile(&state.db, profile_id).await
            {
                tracing::warn!("Auto-activate profile failed (will need manual activation): {e}");
            } else if let Err(e) = crate::repository::misc_queries::apply_profile_mappings(
                &state.db, app_id, profile_id,
            )
            .await
            {
                tracing::warn!("Auto-apply profile mappings failed: {e}");
            } else {
                crate::websocket::push_config_to_affected_agents(&state, Some(app_id), None, None)
                    .await;
            }
        }
    }

    Ok((StatusCode::CREATED, Json(profile)))
}

/// PUT /api/v1/apps/:app_id/profiles/:name/activate
///
/// Activate a profile (switchover). Deactivates all other profiles.
pub async fn activate_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, name)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Check permission (need at least 'operate')
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if !perm.can_operate() {
        return Err(ApiError::Forbidden);
    }

    // Get profile
    let profile: Option<BindingProfile> =
        crate::repository::misc_queries::get_profile_by_name(&state.db, app_id, &name).await?;

    let profile = profile.ok_or(ApiError::NotFound)?;

    if profile.is_active {
        return Ok(Json(json!({
            "message": "Profile is already active",
            "profile": profile
        })));
    }

    // Get currently active profile name for logging
    let current_active =
        crate::repository::misc_queries::get_active_profile_name(&state.db, app_id).await?;

    // Log switchover action
    log_action(
        &state.db,
        user.user_id,
        "activate_profile",
        "binding_profile",
        profile.id,
        json!({
            "application_id": app_id,
            "profile_name": &name,
            "previous_profile": current_active.map(|c| c.0)
        }),
    )
    .await?;

    // Deactivate all profiles and activate the selected one
    crate::repository::misc_queries::deactivate_all_profiles(&state.db, app_id).await?;
    crate::repository::misc_queries::activate_profile(&state.db, *profile.id).await?;

    // Update component agent_ids based on profile mappings
    crate::repository::misc_queries::apply_profile_mappings(&state.db, app_id, *profile.id).await?;

    // Log to switchover_log
    let switchover_id = Uuid::new_v4();
    crate::repository::misc_queries::log_profile_activation(
        &state.db,
        switchover_id,
        app_id,
        &name,
        *profile.id,
    )
    .await?;

    // Notify agents about the new bindings so they start health checks
    crate::websocket::push_config_to_affected_agents(&state, Some(app_id), None, None).await;

    Ok(Json(json!({
        "message": "Profile activated successfully",
        "profile": name,
        "switchover_id": switchover_id
    })))
}

/// DELETE /api/v1/apps/:app_id/profiles/:name
///
/// Delete a profile (cannot delete active profile)
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path((app_id, name)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    // Check permission (need at least 'manage')
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if !perm.can_manage() {
        return Err(ApiError::Forbidden);
    }

    // Get profile
    let profile: Option<BindingProfile> =
        crate::repository::misc_queries::get_profile_by_name(&state.db, app_id, &name).await?;

    let profile = profile.ok_or(ApiError::NotFound)?;

    if profile.is_active {
        return Err(ApiError::Validation(
            "Cannot delete the active profile. Activate another profile first.".into(),
        ));
    }

    // Log action
    log_action(
        &state.db,
        user.user_id,
        "delete_profile",
        "binding_profile",
        profile.id,
        json!({
            "application_id": app_id,
            "name": &name
        }),
    )
    .await?;

    // Delete profile (mappings cascade)
    crate::repository::misc_queries::delete_binding_profile(&state.db, profile.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ══════════════════════════════════════════════════════════════════════
// DR Pattern Rules endpoints
// ══════════════════════════════════════════════════════════════════════

/// GET /api/v1/dr-pattern-rules
///
/// List DR pattern rules for the organization
pub async fn list_dr_pattern_rules(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<DrPatternRule>>, ApiError> {
    let rules: Vec<DrPatternRule> =
        crate::repository::misc_queries::list_dr_pattern_rules(&state.db, *user.organization_id)
            .await?;

    Ok(Json(rules))
}

/// POST /api/v1/dr-pattern-rules
///
/// Create a new DR pattern rule
pub async fn create_dr_pattern_rule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<DrPatternRuleRequest>,
) -> Result<(StatusCode, Json<DrPatternRule>), ApiError> {
    // Only admins can create pattern rules
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Validate regex
    if regex::Regex::new(&body.search_pattern).is_err() {
        return Err(ApiError::Validation(format!(
            "Invalid regex pattern: {}",
            body.search_pattern
        )));
    }

    let rule_id = Uuid::new_v4();

    log_action(
        &state.db,
        user.user_id,
        "create_dr_pattern_rule",
        "dr_pattern_rule",
        rule_id,
        json!({
            "name": &body.name,
            "search_pattern": &body.search_pattern,
            "replace_pattern": &body.replace_pattern
        }),
    )
    .await?;

    let rule: DrPatternRule = crate::repository::misc_queries::create_dr_pattern_rule(
        &state.db,
        rule_id,
        *user.organization_id,
        &body.name,
        &body.search_pattern,
        &body.replace_pattern,
        body.priority.unwrap_or(0),
        body.is_active.unwrap_or(true),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(rule)))
}

/// PUT /api/v1/dr-pattern-rules/:id
///
/// Update a DR pattern rule
pub async fn update_dr_pattern_rule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(rule_id): Path<Uuid>,
    Json(body): Json<DrPatternRuleRequest>,
) -> Result<Json<DrPatternRule>, ApiError> {
    // Only admins can update pattern rules
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Validate regex
    if regex::Regex::new(&body.search_pattern).is_err() {
        return Err(ApiError::Validation(format!(
            "Invalid regex pattern: {}",
            body.search_pattern
        )));
    }

    log_action(
        &state.db,
        user.user_id,
        "update_dr_pattern_rule",
        "dr_pattern_rule",
        rule_id,
        json!({
            "name": &body.name,
            "search_pattern": &body.search_pattern,
            "replace_pattern": &body.replace_pattern
        }),
    )
    .await?;

    let rule: DrPatternRule = crate::repository::misc_queries::update_dr_pattern_rule(
        &state.db,
        rule_id,
        *user.organization_id,
        &body.name,
        &body.search_pattern,
        &body.replace_pattern,
        body.priority.unwrap_or(0),
        body.is_active.unwrap_or(true),
    )
    .await
    .map_err(|_| ApiError::NotFound)?;

    Ok(Json(rule))
}

/// DELETE /api/v1/dr-pattern-rules/:id
///
/// Delete a DR pattern rule
pub async fn delete_dr_pattern_rule(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(rule_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Only admins can delete pattern rules
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_dr_pattern_rule",
        "dr_pattern_rule",
        rule_id,
        json!({}),
    )
    .await?;

    let rows_affected = crate::repository::misc_queries::delete_dr_pattern_rule(
        &state.db,
        rule_id,
        *user.organization_id,
    )
    .await?;

    if rows_affected == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
