//! Query functions for discovery domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Agent queries for discovery
// ============================================================================

/// List active agent IDs for an organization.
pub async fn list_active_agent_ids(
    pool: &DbPool,
    org_id: Uuid,
) -> Result<Vec<DbUuid>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM agents WHERE organization_id = $1 AND is_active = true",
        )
        .bind(org_id)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar::<_, DbUuid>(
            "SELECT id FROM agents WHERE organization_id = $1 AND is_active = 1",
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(pool)
        .await
    }
}

/// Get agent IP addresses (JSONB for postgres, TEXT for sqlite).
pub async fn get_agent_ip_addresses(
    pool: &DbPool,
    agent_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT COALESCE(ip_addresses, '[]'::jsonb) FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT COALESCE(ip_addresses, '[]') FROM agents WHERE id = $1",
        )
        .bind(DbUuid::from(agent_id))
        .fetch_optional(pool)
        .await
    }
}
