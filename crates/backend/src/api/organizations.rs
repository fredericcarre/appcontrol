//! Organization management API.
//!
//! Only platform super-admins can create and manage organizations.
//! Each organization is an isolated tenant with its own PKI, users, sites, and apps.

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
    /// Email of the initial org admin (created as local user)
    pub admin_email: String,
    /// Display name for the org admin
    pub admin_display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrgRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrgRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Check if the user is a platform super-admin.
fn require_super_admin(_user: &AuthUser, db_platform_role: &Option<String>) -> Result<(), ApiError> {
    match db_platform_role {
        Some(role) if role == "super_admin" => Ok(()),
        _ => Err(ApiError::Forbidden),
    }
}

/// Fetch the platform_role for a user from the database.
async fn get_platform_role(db: &sqlx::PgPool, user_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT platform_role FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .flatten()
}

pub async fn list_organizations(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = get_platform_role(&state.db, user.user_id).await;
    require_super_admin(&user, &platform_role)?;

    let orgs = sqlx::query_as::<_, OrgRow>(
        "SELECT id, name, slug, created_at, updated_at FROM organizations ORDER BY name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "organizations": orgs })))
}

pub async fn get_organization(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = get_platform_role(&state.db, user.user_id).await;

    // Super-admins can view any org; regular admins can view their own
    if platform_role.as_deref() != Some("super_admin") && user.organization_id != id {
        return Err(ApiError::Forbidden);
    }

    let org = sqlx::query_as::<_, OrgRow>(
        "SELECT id, name, slug, created_at, updated_at FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(org)))
}

pub async fn create_organization(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateOrgRequest>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = get_platform_role(&state.db, user.user_id).await;
    require_super_admin(&user, &platform_role)?;

    validate_length("name", &req.name, 1, 200)?;
    validate_length("slug", &req.slug, 1, 100)?;
    validate_length("admin_email", &req.admin_email, 3, 300)?;
    validate_length("admin_display_name", &req.admin_display_name, 1, 200)?;

    // Validate slug format (alphanumeric + hyphens only)
    if !req
        .slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(ApiError::Validation(
            "slug must contain only alphanumeric characters and hyphens".to_string(),
        ));
    }

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_organization",
        "organization",
        Uuid::nil(),
        json!({ "name": &req.name, "slug": &req.slug, "admin_email": &req.admin_email }),
    )
    .await
    .ok();

    // Create org + admin user in a transaction
    let mut tx = state.db.begin().await?;

    let org = sqlx::query_as::<_, OrgRow>(
        r#"INSERT INTO organizations (name, slug)
           VALUES ($1, $2)
           RETURNING id, name, slug, created_at, updated_at"#,
    )
    .bind(&req.name)
    .bind(&req.slug)
    .fetch_one(&mut *tx)
    .await?;

    // Create the org admin user
    let admin_id = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO users (organization_id, external_id, email, display_name, role, auth_provider)
           VALUES ($1, $2, $3, $4, 'admin', 'local')
           RETURNING id"#,
    )
    .bind(org.id)
    .bind(format!("local-admin-{}", org.slug))
    .bind(&req.admin_email)
    .bind(&req.admin_display_name)
    .fetch_one(&mut *tx)
    .await?;

    // Auto-initialize PKI for the new org
    match appcontrol_common::generate_ca(&req.name, 3650) {
        Ok(ca) => {
            sqlx::query("UPDATE organizations SET ca_cert_pem = $2, ca_key_pem = $3 WHERE id = $1")
                .bind(org.id)
                .bind(&ca.cert_pem)
                .bind(&ca.key_pem)
                .execute(&mut *tx)
                .await?;
        }
        Err(e) => {
            tracing::warn!(org = %req.name, "Failed to auto-generate CA during org creation: {}", e);
        }
    }

    tx.commit().await?;

    Ok(Json(json!({
        "organization": org,
        "admin_user_id": admin_id,
        "admin_email": req.admin_email,
    })))
}

pub async fn update_organization(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOrgRequest>,
) -> Result<Json<Value>, ApiError> {
    let platform_role = get_platform_role(&state.db, user.user_id).await;
    require_super_admin(&user, &platform_role)?;

    validate_optional_length("name", &req.name, 200)?;

    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_organization",
        "organization",
        id,
        json!({ "name": &req.name }),
    )
    .await
    .ok();

    let org = sqlx::query_as::<_, OrgRow>(
        r#"UPDATE organizations SET
               name = COALESCE($2, name),
               updated_at = now()
           WHERE id = $1
           RETURNING id, name, slug, created_at, updated_at"#,
    )
    .bind(id)
    .bind(&req.name)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(org)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_slug_validation() {
        let valid = "my-org-01";
        assert!(valid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'));

        let invalid = "my org!";
        assert!(!invalid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
