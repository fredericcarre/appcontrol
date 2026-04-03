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

// ============================================================================
// Operation schedule queries (core/operation_scheduler.rs)
// ============================================================================

/// Row returned when querying for due operation schedules.
#[derive(Debug, sqlx::FromRow)]
pub struct DueOperationSchedule {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub application_id: Option<DbUuid>,
    pub component_id: Option<DbUuid>,
    pub name: String,
    pub operation: String,
    pub cron_expression: String,
    pub timezone: String,
}

/// Fetch due operation schedules (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn fetch_due_operation_schedules(pool: &DbPool) -> Result<Vec<DueOperationSchedule>, sqlx::Error> {
    sqlx::query_as::<_, DueOperationSchedule>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, operation, cron_expression, timezone
        FROM operation_schedules
        WHERE is_enabled = true
          AND next_run_at IS NOT NULL
          AND next_run_at <= now()
        ORDER BY next_run_at ASC
        LIMIT 10
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Fetch due operation schedules (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn fetch_due_operation_schedules(pool: &DbPool) -> Result<Vec<DueOperationSchedule>, sqlx::Error> {
    sqlx::query_as::<_, DueOperationSchedule>(
        r#"
        SELECT id, organization_id, application_id, component_id, name, operation, cron_expression, timezone
        FROM operation_schedules
        WHERE is_enabled = 1
          AND next_run_at IS NOT NULL
          AND next_run_at <= datetime('now')
        ORDER BY next_run_at ASC
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await
}
