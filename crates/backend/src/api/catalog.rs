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
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::middleware::audit::log_action;
use crate::repository::catalog as catalog_repo;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateCatalogEntryRequest {
    pub type_key: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub category: Option<String>,
    pub default_check_cmd: Option<String>,
    pub default_start_cmd: Option<String>,
    pub default_stop_cmd: Option<String>,
    pub default_env_vars: Option<Value>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCatalogEntryRequest {
    pub label: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub category: Option<String>,
    pub default_check_cmd: Option<String>,
    pub default_start_cmd: Option<String>,
    pub default_stop_cmd: Option<String>,
    pub default_env_vars: Option<Value>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ImportCatalogRequest {
    pub entries: Vec<CreateCatalogEntryRequest>,
}

/// GET /api/v1/catalog/component-types — list all catalog entries for the user's org.
pub async fn list_catalog(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let entries = catalog_repo::list_catalog(&state.db, *user.organization_id).await?;
    Ok(Json(json!({ "entries": entries })))
}

/// POST /api/v1/catalog/component-types — create a new catalog entry.
/// Requires admin role.
pub async fn create_catalog_entry(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateCatalogEntryRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_length("type_key", &body.type_key, 1, 50)?;
    validate_length("label", &body.label, 1, 200)?;
    validate_optional_length("description", &body.description, 2000)?;

    let entry_id = Uuid::new_v4();
    log_action(
        &state.db,
        user.user_id,
        "create_catalog_entry",
        "component_catalog",
        entry_id,
        json!({"type_key": body.type_key, "label": body.label}),
    )
    .await?;

    let entry = catalog_repo::create_catalog_entry(
        &state.db,
        entry_id,
        *user.organization_id,
        &body.type_key,
        &body.label,
        body.description.as_deref(),
        body.icon.as_deref().unwrap_or("box"),
        body.color.as_deref().unwrap_or("#455A64"),
        body.category.as_deref(),
        body.default_check_cmd.as_deref(),
        body.default_start_cmd.as_deref(),
        body.default_stop_cmd.as_deref(),
        body.default_env_vars.as_ref(),
        body.display_order.unwrap_or(0),
        false,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!(entry))))
}

/// PUT /api/v1/catalog/component-types/:id — update a catalog entry.
/// Requires admin role.
pub async fn update_catalog_entry(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(entry_id): Path<Uuid>,
    Json(body): Json<UpdateCatalogEntryRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    if let Some(ref label) = body.label {
        validate_length("label", label, 1, 200)?;
    }
    validate_optional_length("description", &body.description, 2000)?;

    log_action(
        &state.db,
        user.user_id,
        "update_catalog_entry",
        "component_catalog",
        entry_id,
        json!({"entry_id": entry_id}),
    )
    .await?;

    let entry = catalog_repo::update_catalog_entry(
        &state.db,
        entry_id,
        *user.organization_id,
        body.label.as_deref(),
        body.description.as_deref(),
        body.icon.as_deref(),
        body.color.as_deref(),
        body.category.as_deref(),
        body.default_check_cmd.as_deref(),
        body.default_start_cmd.as_deref(),
        body.default_stop_cmd.as_deref(),
        body.default_env_vars.as_ref(),
        body.display_order,
    )
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(entry)))
}

/// DELETE /api/v1/catalog/component-types/:id — delete a custom catalog entry.
/// Builtin entries cannot be deleted. Requires admin role.
pub async fn delete_catalog_entry(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(entry_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "delete_catalog_entry",
        "component_catalog",
        entry_id,
        json!({}),
    )
    .await?;

    if !catalog_repo::delete_catalog_entry(&state.db, entry_id, *user.organization_id).await? {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/catalog/component-types/import — bulk import catalog entries.
/// Requires admin role. Skips duplicates (by type_key).
pub async fn import_catalog(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<ImportCatalogRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    log_action(
        &state.db,
        user.user_id,
        "import_catalog",
        "component_catalog",
        Uuid::nil(),
        json!({"count": body.entries.len()}),
    )
    .await?;

    let mut created = 0u32;
    let mut skipped = 0u32;

    for entry in &body.entries {
        validate_length("type_key", &entry.type_key, 1, 50)?;
        validate_length("label", &entry.label, 1, 200)?;

        // Check if already exists
        let existing = catalog_repo::get_catalog_entry_by_key(
            &state.db,
            *user.organization_id,
            &entry.type_key,
        )
        .await?;

        if existing.is_some() {
            skipped += 1;
            continue;
        }

        catalog_repo::create_catalog_entry(
            &state.db,
            Uuid::new_v4(),
            *user.organization_id,
            &entry.type_key,
            &entry.label,
            entry.description.as_deref(),
            entry.icon.as_deref().unwrap_or("box"),
            entry.color.as_deref().unwrap_or("#455A64"),
            entry.category.as_deref(),
            entry.default_check_cmd.as_deref(),
            entry.default_start_cmd.as_deref(),
            entry.default_stop_cmd.as_deref(),
            entry.default_env_vars.as_ref(),
            entry.display_order.unwrap_or(0),
            false,
        )
        .await?;
        created += 1;
    }

    Ok(Json(json!({
        "created": created,
        "skipped": skipped,
        "total": body.entries.len()
    })))
}

/// POST /api/v1/catalog/component-types/seed — seed builtin types for the user's org.
/// Idempotent. Requires admin role.
pub async fn seed_catalog(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    let inserted = catalog_repo::seed_builtin_types(&state.db, *user.organization_id).await?;
    Ok(Json(json!({ "seeded": inserted })))
}
