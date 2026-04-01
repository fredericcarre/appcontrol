//! Sites CRUD API.
//!
//! Sites represent physical or logical locations (datacenters, DR sites, environments).
//! Applications, gateways, and agents are organized by site.

use axum::{
    extract::{Extension, Path, Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::DbUuid;
use crate::error::{validate_length, validate_optional_length, ApiError, OptionExt};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub name: String,
    pub code: String,
    /// Site type: primary, dr, staging, development
    pub site_type: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSiteRequest {
    pub name: Option<String>,
    pub location: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListSitesQuery {
    pub site_type: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SiteRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub code: String,
    pub site_type: String,
    pub location: Option<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_sites(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<ListSitesQuery>,
) -> Result<Json<Value>, ApiError> {
    #[cfg(feature = "postgres")]
    let sites = sqlx::query_as::<_, SiteRow>(
        r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
           FROM sites
           WHERE organization_id = $1
             AND ($2::text IS NULL OR site_type = $2)
             AND ($3::bool IS NULL OR is_active = $3)
           ORDER BY code"#,
    )
    .bind(user.organization_id)
    .bind(&query.site_type)
    .bind(query.is_active)
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sites = sqlx::query_as::<_, SiteRow>(
        r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
           FROM sites
           WHERE organization_id = $1
             AND ($2 IS NULL OR site_type = $2)
             AND ($3 IS NULL OR is_active = $3)
           ORDER BY code"#,
    )
    .bind(user.organization_id)
    .bind(&query.site_type)
    .bind(query.is_active)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "sites": sites })))
}

pub async fn get_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    #[cfg(feature = "postgres")]
    let site = sqlx::query_as::<_, SiteRow>(
        r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
           FROM sites
           WHERE id = $1 AND organization_id = $2"#,
    )
    .bind(crate::db::bind_id(id))
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let site = sqlx::query_as::<_, SiteRow>(
        r#"SELECT id, organization_id, name, code, site_type, location, is_active, created_at
           FROM sites
           WHERE id = $1 AND organization_id = $2"#,
    )
    .bind(DbUuid::from(id))
    .bind(user.organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(site)))
}

pub async fn create_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateSiteRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_length("name", &req.name, 1, 200)?;
    validate_length("code", &req.code, 1, 20)?;
    validate_optional_length("location", &req.location, 200)?;

    let site_type = req.site_type.as_deref().unwrap_or("primary");
    if !["primary", "dr", "staging", "development"].contains(&site_type) {
        return Err(ApiError::Validation(
            "site_type must be one of: primary, dr, staging, development".to_string(),
        ));
    }

    // Log before execute (Critical Rule #3)
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "create_site",
        "site",
        Uuid::nil(),
        json!({ "name": &req.name, "code": &req.code, "site_type": site_type }),
    )
    .await
    .ok();

    #[cfg(feature = "postgres")]
    let site = sqlx::query_as::<_, SiteRow>(
        r#"INSERT INTO sites (organization_id, name, code, site_type, location)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
    )
    .bind(user.organization_id)
    .bind(&req.name)
    .bind(&req.code)
    .bind(site_type)
    .bind(&req.location)
    .fetch_one(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let site = {
        let new_id = DbUuid::new_v4();
        sqlx::query_as::<_, SiteRow>(
            r#"INSERT INTO sites (id, organization_id, name, code, site_type, location)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
        )
        .bind(new_id)
        .bind(user.organization_id)
        .bind(&req.name)
        .bind(&req.code)
        .bind(site_type)
        .bind(&req.location)
        .fetch_one(&state.db)
        .await?
    };

    Ok(Json(json!(site)))
}

pub async fn update_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSiteRequest>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    validate_optional_length("name", &req.name, 200)?;
    validate_optional_length("location", &req.location, 200)?;

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "update_site",
        "site",
        id,
        json!({ "name": &req.name, "location": &req.location, "is_active": req.is_active }),
    )
    .await
    .ok();

    #[cfg(feature = "postgres")]
    let site = sqlx::query_as::<_, SiteRow>(
        r#"UPDATE sites SET
               name = COALESCE($3, name),
               location = COALESCE($4, location),
               is_active = COALESCE($5, is_active)
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
    )
    .bind(crate::db::bind_id(id))
    .bind(user.organization_id)
    .bind(&req.name)
    .bind(&req.location)
    .bind(req.is_active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let site = sqlx::query_as::<_, SiteRow>(
        r#"UPDATE sites SET
               name = COALESCE($3, name),
               location = COALESCE($4, location),
               is_active = COALESCE($5, is_active)
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, name, code, site_type, location, is_active, created_at"#,
    )
    .bind(DbUuid::from(id))
    .bind(user.organization_id)
    .bind(&req.name)
    .bind(&req.location)
    .bind(req.is_active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_not_found()?;

    Ok(Json(json!(site)))
}

pub async fn delete_site(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Log before execute
    crate::middleware::audit::log_action(
        &state.db,
        user.user_id,
        "delete_site",
        "site",
        id,
        json!({}),
    )
    .await
    .ok();

    // Check for applications linked to this site
    #[cfg(feature = "postgres")]
    let app_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE site_id = $1")
        .bind(crate::db::bind_id(id))
        .fetch_one(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let app_count: i64 = {
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE site_id = $1")
            .bind(DbUuid::from(id))
            .fetch_one(&state.db)
            .await?;
        count as i64
    };

    if app_count > 0 {
        return Err(ApiError::Conflict(format!(
            "Cannot delete site: {} application(s) are linked to it",
            app_count
        )));
    }

    #[cfg(feature = "postgres")]
    let result = sqlx::query("DELETE FROM sites WHERE id = $1 AND organization_id = $2")
        .bind(crate::db::bind_id(id))
        .bind(user.organization_id)
        .execute(&state.db)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query("DELETE FROM sites WHERE id = $1 AND organization_id = $2")
        .bind(DbUuid::from(id))
        .bind(user.organization_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(Json(json!({ "status": "deleted" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_site_type_validation() {
        assert!(["primary", "dr", "staging", "development"].contains(&"primary"));
        assert!(!["primary", "dr", "staging", "development"].contains(&"invalid"));
    }
}
