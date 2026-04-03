//! Query functions for switchover domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Switchover validation queries
// ============================================================================

/// Find a binding profile for an app that has gateways in a target site.
/// Returns (profile_id, profile_name, mapping_count).
pub async fn find_profile_for_site(
    pool: &DbPool,
    app_id: Uuid,
    target_site_id: Uuid,
) -> Result<Option<(DbUuid, String, i64)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = r#"
        SELECT bp.id, bp.name,
               (SELECT COUNT(*) FROM binding_profile_mappings WHERE profile_id = bp.id) as mapping_count
        FROM binding_profiles bp
        WHERE bp.application_id = $1
          AND EXISTS (
            SELECT 1 FROM unnest(bp.gateway_ids) AS gw_id
            JOIN gateways g ON g.id = gw_id
            WHERE g.site_id = $2
          )
        LIMIT 1
        "#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = r#"
        SELECT bp.id, bp.name,
               (SELECT COUNT(*) FROM binding_profile_mappings WHERE profile_id = bp.id) as mapping_count
        FROM binding_profiles bp
        WHERE bp.application_id = $1
          AND EXISTS (
            SELECT 1 FROM json_each(bp.gateway_ids) AS gw
            JOIN gateways g ON g.id = gw.value
            WHERE g.site_id = $2
          )
        LIMIT 1
        "#;
    sqlx::query_as::<_, (DbUuid, String, i64)>(sql)
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(target_site_id))
        .fetch_optional(pool)
        .await
}

/// Get the active profile for an application. Returns (profile_id, profile_name).
pub async fn get_active_profile(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<(DbUuid, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = 1";
    sqlx::query_as::<_, (DbUuid, String)>(sql)
        .bind(DbUuid::from(app_id))
        .fetch_optional(pool)
        .await
}

/// Get all gateways for a site with active status.
pub async fn get_active_gateways_for_site(
    pool: &DbPool,
    site_id: Uuid,
    org_id: Uuid,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql = "SELECT id, name FROM gateways WHERE site_id = $1 AND organization_id = $2 AND is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql = "SELECT id, name FROM gateways WHERE site_id = $1 AND organization_id = $2 AND is_active = 1";
    sqlx::query_as::<_, (DbUuid, String)>(sql)
        .bind(DbUuid::from(site_id))
        .bind(DbUuid::from(org_id))
        .fetch_optional(pool)
        .await
        .map(|opt| opt.into_iter().collect())
}

/// Get components at a target site (bound to agents on that site).
pub async fn get_components_at_site(
    pool: &DbPool,
    app_id: Uuid,
    site_id: Uuid,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        r#"SELECT c.id, c.name
           FROM components c
           JOIN agents a ON c.agent_id = a.id
           JOIN gateways g ON a.gateway_id = g.id
           WHERE c.application_id = $1
             AND g.site_id = $2"#,
    )
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(site_id))
    .fetch_all(pool)
    .await
}

/// Deactivate all profiles and activate a specific one.
pub async fn switch_active_profile(
    pool: &DbPool,
    app_id: Uuid,
    profile_id: Uuid,
) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query("UPDATE binding_profiles SET is_active = false WHERE application_id = $1")
            .bind(app_id)
            .execute(pool)
            .await?;
        sqlx::query("UPDATE binding_profiles SET is_active = true WHERE id = $1")
            .bind(profile_id)
            .execute(pool)
            .await?;
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query("UPDATE binding_profiles SET is_active = 0 WHERE application_id = $1")
            .bind(DbUuid::from(app_id))
            .execute(pool)
            .await?;
        sqlx::query("UPDATE binding_profiles SET is_active = 1 WHERE id = $1")
            .bind(DbUuid::from(profile_id))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Update component agent bindings from profile mappings.
#[cfg(feature = "postgres")]
pub async fn apply_profile_agent_bindings(
    pool: &DbPool,
    app_id: Uuid,
    profile_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"UPDATE components c
           SET agent_id = m.agent_id
           FROM binding_profile_mappings m
           JOIN binding_profiles p ON m.profile_id = p.id
           WHERE c.application_id = $1
             AND p.id = $2
             AND c.name = m.component_name"#,
    )
    .bind(app_id)
    .bind(profile_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn apply_profile_agent_bindings(
    pool: &DbPool,
    app_id: Uuid,
    profile_id: Uuid,
) -> Result<u64, sqlx::Error> {
    // SQLite doesn't support UPDATE ... FROM, so we do it in two steps
    let mappings = sqlx::query_as::<_, (String, DbUuid)>(
        "SELECT component_name, agent_id FROM binding_profile_mappings WHERE profile_id = $1",
    )
    .bind(DbUuid::from(profile_id))
    .fetch_all(pool)
    .await?;

    let mut updated = 0u64;
    for (comp_name, agent_id) in mappings {
        let result = sqlx::query(
            "UPDATE components SET agent_id = $1 WHERE application_id = $2 AND name = $3",
        )
        .bind(agent_id)
        .bind(DbUuid::from(app_id))
        .bind(&comp_name)
        .execute(pool)
        .await?;
        updated += result.rows_affected();
    }
    Ok(updated)
}
