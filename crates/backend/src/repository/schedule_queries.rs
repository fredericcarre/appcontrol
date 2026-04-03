//! Query functions for schedule domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

/// Get the application name.
pub async fn get_app_name(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT name FROM applications WHERE id = $1")
        .bind(crate::db::bind_id(app_id))
        .fetch_optional(pool)
        .await
}

/// Get a component's display name (display_name or name fallback).
pub async fn get_component_display_name(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(display_name, name) FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
}

/// Get the application_id for a component.
pub async fn get_component_app_id(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT application_id FROM components WHERE id = $1",
        )
        .bind(component_id)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT application_id FROM components WHERE id = $1",
        )
        .bind(DbUuid::from(component_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|v| v.into_inner()))
    }
}

/// Get the organization_id for an application.
pub async fn get_app_org_id(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT organization_id FROM applications WHERE id = $1",
        )
        .bind(app_id)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT organization_id FROM applications WHERE id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|v| v.into_inner()))
    }
}
