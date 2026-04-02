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
    pub host: String,
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

    let profiles: Vec<ProfileRow> = sqlx::query_as(
        r#"
        SELECT
            p.id, p.name, p.description, p.profile_type, p.is_active,
            p.gateway_ids, p.auto_failover, p.created_at,
            COUNT(m.id) as mapping_count
        FROM binding_profiles p
        LEFT JOIN binding_profile_mappings m ON p.id = m.profile_id
        WHERE p.application_id = $1
        GROUP BY p.id
        ORDER BY p.profile_type, p.name
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(&state.db)
    .await?;

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

    let profile: Option<BindingProfile> = sqlx::query_as(
        r#"
        SELECT id, application_id, name, description, profile_type, is_active,
               gateway_ids, auto_failover, created_at, created_by
        FROM binding_profiles
        WHERE application_id = $1 AND name = $2
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;

    let profile = profile.ok_or(ApiError::NotFound)?;

    let mappings: Vec<ProfileMapping> = sqlx::query_as(
        r#"
        SELECT id, profile_id, component_name, host, agent_id, resolved_via
        FROM binding_profile_mappings
        WHERE profile_id = $1
        ORDER BY component_name
        "#,
    )
    .bind(profile.id)
    .fetch_all(&state.db)
    .await?;

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
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM binding_profiles WHERE application_id = $1 AND name = $2")
            .bind(crate::db::bind_id(app_id))
            .bind(&body.name)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_some() {
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
    let profile: BindingProfile = sqlx::query_as(
        r#"
        INSERT INTO binding_profiles (id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_by)
        VALUES ($1, $2, $3, $4, $5, false, $6, $7, $8)
        RETURNING id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_at, created_by
        "#,
    )
    .bind(profile_id)
    .bind(crate::db::bind_id(app_id))
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.profile_type)
    .bind(UuidArray::from(body.gateway_ids.clone()))
    .bind(body.auto_failover.unwrap_or(false))
    .bind(crate::db::bind_id(user.user_id))
    .fetch_one(&state.db)
    .await?;

    // Copy mappings from another profile if specified
    if let Some(copy_from_id) = body.copy_from_profile_id {
        sqlx::query(
            r#"
            INSERT INTO binding_profile_mappings (profile_id, component_name, host, agent_id, resolved_via)
            SELECT $1, component_name, host, agent_id, resolved_via
            FROM binding_profile_mappings
            WHERE profile_id = $2
            "#,
        )
        .bind(profile_id)
        .bind(copy_from_id)
        .execute(&state.db)
        .await?;
    } else if let Some(ref mappings) = body.mappings {
        // Create manual mappings
        for m in mappings {
            sqlx::query(
                r#"
                INSERT INTO binding_profile_mappings (profile_id, component_name, host, agent_id, resolved_via)
                VALUES ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(profile_id)
            .bind(&m.component_name)
            .bind(&m.host)
            .bind(m.agent_id)
            .bind(&m.resolved_via)
            .execute(&state.db)
            .await?;
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
    let profile: Option<BindingProfile> = sqlx::query_as(
        "SELECT id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_at, created_by FROM binding_profiles WHERE application_id = $1 AND name = $2",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;

    let profile = profile.ok_or(ApiError::NotFound)?;

    if profile.is_active {
        return Ok(Json(json!({
            "message": "Profile is already active",
            "profile": profile
        })));
    }

    // Get currently active profile name for logging
    #[cfg(feature = "postgres")]
    let current_active: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM binding_profiles WHERE application_id = $1 AND is_active = true",
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_optional(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let current_active: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM binding_profiles WHERE application_id = $1 AND is_active = 1",
    )
    .bind(DbUuid::from(app_id))
    .fetch_optional(&state.db)
    .await?;

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

    // Deactivate all profiles
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE binding_profiles SET is_active = false WHERE application_id = $1")
        .bind(crate::db::bind_id(app_id))
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE binding_profiles SET is_active = 0 WHERE application_id = $1")
        .bind(DbUuid::from(app_id))
        .execute(&state.db)
        .await?;

    // Activate the selected profile
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE binding_profiles SET is_active = true WHERE id = $1")
        .bind(profile.id)
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("UPDATE binding_profiles SET is_active = 1 WHERE id = $1")
        .bind(profile.id)
        .execute(&state.db)
        .await?;

    // Update component agent_ids based on profile mappings
    sqlx::query(
        r#"
        UPDATE components c
        SET agent_id = m.agent_id
        FROM binding_profile_mappings m
        JOIN binding_profiles p ON m.profile_id = p.id
        WHERE c.application_id = $1
          AND p.id = $2
          AND c.name = m.component_name
        "#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(profile.id)
    .execute(&state.db)
    .await?;

    // Log to switchover_log
    let switchover_id = Uuid::new_v4();
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO switchover_log (switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, 'COMMIT', 'completed', $3)
        "#,
    )
    .bind(crate::db::bind_id(switchover_id))
    .bind(crate::db::bind_id(app_id))
    .bind(json!({
        "type": "profile_activation",
        "profile_name": &name,
        "profile_id": profile.id
    }))
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        r#"
        INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, 'COMMIT', 'completed', $4)
        "#,
    )
    .bind(crate::db::bind_id(Uuid::new_v4()))
    .bind(crate::db::bind_id(switchover_id))
    .bind(crate::db::bind_id(app_id))
    .bind(
        json!({
            "type": "profile_activation",
            "profile_name": &name,
            "profile_id": profile.id
        })
        .to_string(),
    )
    .execute(&state.db)
    .await?;

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
    let profile: Option<BindingProfile> = sqlx::query_as(
        "SELECT id, application_id, name, description, profile_type, is_active, gateway_ids, auto_failover, created_at, created_by FROM binding_profiles WHERE application_id = $1 AND name = $2",
    )
    .bind(crate::db::bind_id(app_id))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;

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
    sqlx::query("DELETE FROM binding_profiles WHERE id = $1")
        .bind(profile.id)
        .execute(&state.db)
        .await?;

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
    let rules: Vec<DrPatternRule> = sqlx::query_as(
        r#"
        SELECT id, organization_id, name, search_pattern, replace_pattern, priority, is_active, created_at
        FROM dr_pattern_rules
        WHERE organization_id = $1
        ORDER BY priority DESC, name
        "#,
    )
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_all(&state.db)
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

    let rule: DrPatternRule = sqlx::query_as(
        r#"
        INSERT INTO dr_pattern_rules (id, organization_id, name, search_pattern, replace_pattern, priority, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, organization_id, name, search_pattern, replace_pattern, priority, is_active, created_at
        "#,
    )
    .bind(rule_id)
    .bind(crate::db::bind_id(user.organization_id))
    .bind(&body.name)
    .bind(&body.search_pattern)
    .bind(&body.replace_pattern)
    .bind(body.priority.unwrap_or(0))
    .bind(body.is_active.unwrap_or(true))
    .fetch_one(&state.db)
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

    let rule: DrPatternRule = sqlx::query_as(
        r#"
        UPDATE dr_pattern_rules
        SET name = $2, search_pattern = $3, replace_pattern = $4, priority = $5, is_active = $6
        WHERE id = $1 AND organization_id = $7
        RETURNING id, organization_id, name, search_pattern, replace_pattern, priority, is_active, created_at
        "#,
    )
    .bind(rule_id)
    .bind(&body.name)
    .bind(&body.search_pattern)
    .bind(&body.replace_pattern)
    .bind(body.priority.unwrap_or(0))
    .bind(body.is_active.unwrap_or(true))
    .bind(crate::db::bind_id(user.organization_id))
    .fetch_one(&state.db)
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

    let result = sqlx::query("DELETE FROM dr_pattern_rules WHERE id = $1 AND organization_id = $2")
        .bind(rule_id)
        .bind(crate::db::bind_id(user.organization_id))
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
