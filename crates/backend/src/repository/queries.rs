//! Centralized SQL query functions for all remaining database operations.
//!
//! Each function takes `&DbPool` and returns domain types.
//! PG/SQLite differences are handled internally via `#[cfg]` attributes.
//! This module exists to move ALL sqlx queries out of handler files
//! into the repository layer.

#![allow(unused_imports)]

use crate::db::{DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// Re-export for convenience
pub use crate::db::bind_id;

// ============================================================================
// Generic helpers used across many modules
// ============================================================================

/// Insert a config version snapshot (cross-database).
pub async fn insert_config_version(
    pool: &DbPool,
    resource_type: &str,
    resource_id: Uuid,
    changed_by: Uuid,
    before_snapshot: &str,
    after_snapshot: &str,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot) \
         VALUES ($1, $2, $3, $4::jsonb, $5::jsonb)",
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(changed_by)
    .bind(before_snapshot)
    .bind(after_snapshot)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO config_versions (id, resource_type, resource_id, changed_by, before_snapshot, after_snapshot) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(DbUuid::new_v4())
    .bind(resource_type)
    .bind(DbUuid::from(resource_id))
    .bind(DbUuid::from(changed_by))
    .bind(before_snapshot)
    .bind(after_snapshot)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get organization_id for a user.
pub async fn get_user_org_id(pool: &DbPool, user_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>("SELECT organization_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>("SELECT organization_id FROM users WHERE id = $1")
            .bind(DbUuid::from(user_id))
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|u| u.into_inner()))
    }
}

/// Get a single organization ID (first one found).
pub async fn get_first_org_id(pool: &DbPool) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM organizations LIMIT 1")
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>("SELECT id FROM organizations LIMIT 1")
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|u| u.into_inner()))
    }
}

/// Check if a gateway is active.
pub async fn is_gateway_active(pool: &DbPool, gateway_id: Uuid) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: Option<bool> = sqlx::query_scalar("SELECT COALESCE(is_active, true) FROM gateways WHERE id = $1")
            .bind(gateway_id)
            .fetch_optional(pool)
            .await?;
        Ok(val.unwrap_or(false))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let val: Option<i32> = sqlx::query_scalar("SELECT COALESCE(is_active, 1) FROM gateways WHERE id = $1")
            .bind(DbUuid::from(gateway_id))
            .fetch_optional(pool)
            .await?;
        Ok(val.map(|v| v != 0).unwrap_or(false))
    }
}

/// Update gateway heartbeat.
pub async fn update_gateway_heartbeat(pool: &DbPool, gateway_id: Uuid) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let sql = format!(
            "UPDATE gateways SET last_heartbeat_at = {}, is_active = true WHERE id = $1",
            crate::db::sql::now(),
        );
        sqlx::query(&sql).bind(gateway_id).execute(pool).await?;
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let sql = format!(
            "UPDATE gateways SET last_heartbeat_at = {}, is_active = 1 WHERE id = $1",
            crate::db::sql::now(),
        );
        sqlx::query(&sql).bind(DbUuid::from(gateway_id)).execute(pool).await?;
    }
    Ok(())
}

/// Get gateway name by ID.
pub async fn get_gateway_name(pool: &DbPool, gateway_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar("SELECT name FROM gateways WHERE id = $1")
            .bind(gateway_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar("SELECT name FROM gateways WHERE id = $1")
            .bind(DbUuid::from(gateway_id))
            .fetch_optional(pool)
            .await
    }
}

/// Get agent hostname by ID.
pub async fn get_agent_hostname(pool: &DbPool, agent_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar("SELECT hostname FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar("SELECT hostname FROM agents WHERE id = $1")
            .bind(DbUuid::from(agent_id))
            .fetch_optional(pool)
            .await
    }
}

/// Get component application_id.
pub async fn get_component_app_id(pool: &DbPool, component_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(component_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>("SELECT application_id FROM components WHERE id = $1")
            .bind(DbUuid::from(component_id))
            .fetch_optional(pool)
            .await?;
        Ok(row.map(|u| u.into_inner()))
    }
}

/// Update component state.
pub async fn update_component_state(pool: &DbPool, component_id: Uuid, state: &str) -> Result<(), sqlx::Error> {
    let sql = format!(
        "UPDATE components SET current_state = $2, updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    );
    #[cfg(feature = "postgres")]
    sqlx::query(&sql).bind(component_id).bind(state).execute(pool).await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(&sql).bind(DbUuid::from(component_id)).bind(state).execute(pool).await?;
    Ok(())
}

/// Insert a state transition record.
pub async fn insert_state_transition(
    pool: &DbPool,
    component_id: Uuid,
    from_state: &str,
    to_state: &str,
    trigger: &str,
    details: Option<&str>,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO state_transitions (component_id, from_state, to_state, trigger, details) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(component_id)
    .bind(from_state)
    .bind(to_state)
    .bind(trigger)
    .bind(details)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO state_transitions (id, component_id, from_state, to_state, trigger, details) VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(component_id))
    .bind(from_state)
    .bind(to_state)
    .bind(trigger)
    .bind(details)
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert a check event record.
pub async fn insert_check_event(
    pool: &DbPool,
    component_id: Uuid,
    agent_id: Uuid,
    check_type: &str,
    exit_code: i16,
    stdout: Option<&str>,
    stderr: Option<&str>,
    duration_ms: i32,
    metrics: Option<&Value>,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO check_events (component_id, agent_id, check_type, exit_code, stdout, stderr, duration_ms, metrics) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    )
    .bind(component_id)
    .bind(agent_id)
    .bind(check_type)
    .bind(exit_code)
    .bind(stdout)
    .bind(stderr)
    .bind(duration_ms)
    .bind(metrics)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO check_events (component_id, agent_id, check_type, exit_code, stdout, stderr, duration_ms, metrics) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    )
    .bind(DbUuid::from(component_id))
    .bind(DbUuid::from(agent_id))
    .bind(check_type)
    .bind(exit_code)
    .bind(stdout)
    .bind(stderr)
    .bind(duration_ms)
    .bind(metrics.map(|v| serde_json::to_string(v).unwrap_or_default()))
    .execute(pool)
    .await?;

    Ok(())
}

/// Get application name.
pub async fn get_app_name(pool: &DbPool, app_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
            .bind(app_id)
            .fetch_optional(pool)
            .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar("SELECT name FROM applications WHERE id = $1")
            .bind(DbUuid::from(app_id))
            .fetch_optional(pool)
            .await
    }
}

/// Check if an application is suspended.
pub async fn is_app_suspended(pool: &DbPool, app_id: Uuid) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: Option<bool> = sqlx::query_scalar("SELECT is_suspended FROM applications WHERE id = $1")
            .bind(app_id)
            .fetch_optional(pool)
            .await?;
        Ok(val.unwrap_or(false))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let val: Option<bool> = sqlx::query_scalar("SELECT is_suspended FROM applications WHERE id = $1")
            .bind(DbUuid::from(app_id))
            .fetch_optional(pool)
            .await?;
        Ok(val.unwrap_or(false))
    }
}

/// Get heartbeat timeout for an organization.
pub async fn get_org_heartbeat_timeout(pool: &DbPool, org_id: Uuid) -> Result<i32, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: Option<i32> = sqlx::query_scalar(
            "SELECT heartbeat_timeout_seconds FROM organizations WHERE id = $1"
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await?;
        Ok(val.unwrap_or(180))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let val: Option<i32> = sqlx::query_scalar(
            "SELECT heartbeat_timeout_seconds FROM organizations WHERE id = $1"
        )
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(val.unwrap_or(180))
    }
}

/// Verify an application belongs to an organization.
pub async fn verify_app_org(pool: &DbPool, app_id: Uuid, org_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM applications WHERE id = $1 AND organization_id = $2"
        )
        .bind(app_id)
        .bind(org_id)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row = sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM applications WHERE id = $1 AND organization_id = $2"
        )
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|u| u.into_inner()))
    }
}

/// Check if an agent is active.
pub async fn is_agent_active(pool: &DbPool, agent_id: Uuid) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let val: Option<bool> = sqlx::query_scalar(
            "SELECT COALESCE(is_active, true) FROM agents WHERE id = $1"
        )
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;
        Ok(val.unwrap_or(false))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let val: Option<i32> = sqlx::query_scalar(
            "SELECT COALESCE(is_active, 1) FROM agents WHERE id = $1"
        )
        .bind(DbUuid::from(agent_id))
        .fetch_optional(pool)
        .await?;
        Ok(val.map(|v| v != 0).unwrap_or(false))
    }
}

/// Update agent heartbeat.
pub async fn update_agent_heartbeat(
    pool: &DbPool,
    agent_id: Uuid,
    version: Option<&str>,
    os_info: Option<&str>,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let sql = format!(
            "UPDATE agents SET last_heartbeat_at = {}, is_active = true, \
             version = COALESCE($2, version), os_info = COALESCE($3, os_info) WHERE id = $1",
            crate::db::sql::now(),
        );
        sqlx::query(&sql).bind(agent_id).bind(version).bind(os_info).execute(pool).await?;
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let sql = format!(
            "UPDATE agents SET last_heartbeat_at = {}, is_active = 1, \
             version = COALESCE($2, version), os_info = COALESCE($3, os_info) WHERE id = $1",
            crate::db::sql::now(),
        );
        sqlx::query(&sql).bind(DbUuid::from(agent_id)).bind(version).bind(os_info).execute(pool).await?;
    }
    Ok(())
}

/// Check if a site exists.
pub async fn site_exists(pool: &DbPool, site_id: Uuid) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let row: Option<Uuid> = sqlx::query_scalar("SELECT id FROM sites WHERE id = $1")
            .bind(site_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let row: Option<DbUuid> = sqlx::query_scalar("SELECT id FROM sites WHERE id = $1")
            .bind(DbUuid::from(site_id))
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }
}
