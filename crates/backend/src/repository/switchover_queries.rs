//! Query functions for switchover domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{DbJson, DbPool, DbUuid};
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

/// Insert a new switchover log entry.
pub async fn insert_switchover_log(
    pool: &DbPool,
    switchover_id: Uuid,
    app_id: Uuid,
    phase: &str,
    status: &str,
    details: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO switchover_log (id, switchover_id, application_id, phase, status, details)
        VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(DbUuid::new_v4())
    .bind(DbUuid::from(switchover_id))
    .bind(DbUuid::from(app_id))
    .bind(phase)
    .bind(status)
    .bind(DbJson::from(details))
    .execute(pool)
    .await?;
    Ok(())
}

/// Get the current active switchover phase. Returns (switchover_id, phase, status).
pub async fn get_active_switchover(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<(DbUuid, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String)>(
        r#"SELECT switchover_id, phase, status FROM switchover_log
        WHERE application_id = $1 AND status = 'in_progress'
        ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(DbUuid::from(app_id))
    .fetch_optional(pool)
    .await
}

/// Get switchover details from the PREPARE phase entry.
pub async fn get_switchover_details_from_prepare(
    pool: &DbPool,
    switchover_id: Uuid,
) -> Result<Option<DbJson>, sqlx::Error> {
    sqlx::query_scalar::<_, DbJson>(
        r#"SELECT details FROM switchover_log
        WHERE switchover_id = $1 AND phase = 'PREPARE'
        ORDER BY created_at ASC LIMIT 1"#,
    )
    .bind(DbUuid::from(switchover_id))
    .fetch_optional(pool)
    .await
}

/// Get the active profile for an application. Returns (profile_id, profile_name).
pub async fn get_active_profile(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<(DbUuid, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql =
        "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = true";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql = "SELECT id, name FROM binding_profiles WHERE application_id = $1 AND is_active = 1";
    sqlx::query_as::<_, (DbUuid, String)>(sql)
        .bind(DbUuid::from(app_id))
        .fetch_optional(pool)
        .await
}

/// Get site info (name, is_active) by id.
pub async fn get_site_info(
    pool: &DbPool,
    site_id: Uuid,
) -> Result<Option<(String, bool)>, sqlx::Error> {
    sqlx::query_as::<_, (String, bool)>("SELECT name, is_active FROM sites WHERE id = $1")
        .bind(DbUuid::from(site_id))
        .fetch_optional(pool)
        .await
}

/// Get all components for an app. Returns Vec<(id, name)>.
pub async fn get_app_components(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        "SELECT id, name FROM components WHERE application_id = $1",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(pool)
    .await
}

/// Check if a binding profile mapping exists for a given profile and component name.
pub async fn has_profile_mapping(
    pool: &DbPool,
    profile_id: impl Into<DbUuid>,
    component_name: &str,
) -> Result<bool, sqlx::Error> {
    let profile_id = profile_id.into();
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM binding_profile_mappings WHERE profile_id = $1 AND component_name = $2)",
    )
    .bind(profile_id)
    .bind(component_name)
    .fetch_one(pool)
    .await
}

/// Get distinct target agents for a profile. Returns Vec<(agent_id, hostname)>.
pub async fn get_target_agents_for_profile(
    pool: &DbPool,
    profile_id: impl Into<DbUuid>,
) -> Result<Vec<(DbUuid, String)>, sqlx::Error> {
    let profile_id = profile_id.into();
    sqlx::query_as::<_, (DbUuid, String)>(
        r#"SELECT DISTINCT bpm.agent_id, a.hostname FROM binding_profile_mappings bpm
        JOIN agents a ON a.id = bpm.agent_id WHERE bpm.profile_id = $1"#,
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await
}

/// Get the last heartbeat timestamp for an agent.
pub async fn get_agent_last_heartbeat(
    pool: &DbPool,
    agent_id: impl Into<DbUuid>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
    let agent_id = agent_id.into();
    sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "SELECT last_heartbeat_at FROM agents WHERE id = $1",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
}

/// Get the latest switchover entry for an app (any status). Returns (switchover_id, phase).
pub async fn get_latest_switchover(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        r#"SELECT switchover_id, phase FROM switchover_log
        WHERE application_id = $1 ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(DbUuid::from(app_id))
    .fetch_optional(pool)
    .await
}

/// Get the latest in-progress switchover entry for an app. Returns (switchover_id, phase).
pub async fn get_active_switchover_for_commit(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Option<(DbUuid, String)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String)>(
        r#"SELECT switchover_id, phase FROM switchover_log
        WHERE application_id = $1 AND status = 'in_progress'
        ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(DbUuid::from(app_id))
    .fetch_optional(pool)
    .await
}

/// Get recent switchover log entries for status. Returns Vec<(switchover_id, phase, status, created_at)>.
pub async fn get_switchover_history(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT switchover_id, phase, status, created_at FROM switchover_log
        WHERE application_id = $1 ORDER BY created_at DESC LIMIT 20"#,
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(pool)
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

/// Update switchover details in the PREPARE phase entry.
pub async fn update_switchover_prepare_details(
    pool: &DbPool,
    switchover_id: Uuid,
    details: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE switchover_log SET details = $2 WHERE switchover_id = $1 AND phase = 'PREPARE'",
    )
    .bind(DbUuid::from(switchover_id))
    .bind(DbJson::from(details))
    .execute(pool)
    .await?;
    Ok(())
}

/// Count non-optional components still running (not STOPPED/UNKNOWN).
pub async fn count_running_non_optional(pool: &DbPool, app_id: Uuid) -> Result<i64, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = "SELECT COUNT(*) FROM components WHERE application_id = $1 AND is_optional = false AND current_state NOT IN ('STOPPED', 'UNKNOWN')";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = "SELECT COUNT(*) FROM components WHERE application_id = $1 AND is_optional = 0 AND current_state NOT IN ('STOPPED', 'UNKNOWN')";

    sqlx::query_scalar::<_, i64>(sql)
        .bind(DbUuid::from(app_id))
        .fetch_one(pool)
        .await
}

/// Find a target binding profile for an app at a given site. Returns (profile_id, profile_name).
pub async fn find_target_profile(
    pool: &DbPool,
    app_id: Uuid,
    target_site_id: Uuid,
) -> Result<Option<(DbUuid, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = r#"SELECT bp.id, bp.name FROM binding_profiles bp WHERE bp.application_id = $1
        AND EXISTS (SELECT 1 FROM unnest(bp.gateway_ids) AS gw_id JOIN gateways g ON g.id = gw_id WHERE g.site_id = $2) LIMIT 1"#;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = r#"SELECT bp.id, bp.name FROM binding_profiles bp WHERE bp.application_id = $1
        AND EXISTS (SELECT 1 FROM json_each(bp.gateway_ids) AS gw JOIN gateways g ON g.id = gw.value WHERE g.site_id = $2) LIMIT 1"#;

    sqlx::query_as::<_, (DbUuid, String)>(sql)
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(target_site_id))
        .fetch_optional(pool)
        .await
}

/// Deactivate all binding profiles for an app.
pub async fn deactivate_all_profiles(pool: &DbPool, app_id: Uuid) -> Result<(), sqlx::Error> {
    #[cfg(feature = "postgres")]
    let sql: &str = "UPDATE binding_profiles SET is_active = false WHERE application_id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = "UPDATE binding_profiles SET is_active = 0 WHERE application_id = $1";
    sqlx::query(sql)
        .bind(DbUuid::from(app_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Activate a specific binding profile.
pub async fn activate_profile(
    pool: &DbPool,
    profile_id: impl Into<DbUuid>,
) -> Result<(), sqlx::Error> {
    let profile_id = profile_id.into();
    #[cfg(feature = "postgres")]
    let sql: &str = "UPDATE binding_profiles SET is_active = true WHERE id = $1";
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let sql: &str = "UPDATE binding_profiles SET is_active = 1 WHERE id = $1";
    sqlx::query(sql).bind(profile_id).execute(pool).await?;
    Ok(())
}

/// Update application site_id.
pub async fn update_app_site(
    pool: &DbPool,
    app_id: Uuid,
    site_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE applications SET site_id = $2, updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    ))
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(site_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Get all binding profile mappings for a profile. Returns Vec<(component_name, agent_id)>.
pub async fn get_profile_mappings(
    pool: &DbPool,
    profile_id: impl Into<DbUuid>,
) -> Result<Vec<(String, DbUuid)>, sqlx::Error> {
    let profile_id = profile_id.into();
    sqlx::query_as::<_, (String, DbUuid)>(
        "SELECT component_name, agent_id FROM binding_profile_mappings WHERE profile_id = $1",
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await
}

/// Get component info for switchover. Returns (id, agent_id, check_cmd, start_cmd, stop_cmd).
pub async fn get_component_for_switchover(
    pool: &DbPool,
    app_id: Uuid,
    comp_name: &str,
) -> Result<
    Option<(
        DbUuid,
        Option<DbUuid>,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as::<_, (DbUuid, Option<DbUuid>, Option<String>, Option<String>, Option<String>)>(
        "SELECT id, agent_id, check_cmd, start_cmd, stop_cmd FROM components WHERE application_id = $1 AND name = $2",
    )
    .bind(DbUuid::from(app_id))
    .bind(comp_name)
    .fetch_optional(pool)
    .await
}

/// Get site overrides for a component at a site.
pub async fn get_site_cmd_overrides(
    pool: &DbPool,
    component_id: impl Into<DbUuid>,
    site_id: Uuid,
) -> Result<Option<(Option<String>, Option<String>, Option<String>)>, sqlx::Error> {
    let component_id = component_id.into();
    sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
        r#"SELECT check_cmd_override, start_cmd_override, stop_cmd_override FROM site_overrides WHERE component_id = $1 AND site_id = $2"#,
    )
    .bind(component_id)
    .bind(DbUuid::from(site_id))
    .fetch_optional(pool)
    .await
}

/// Update component agent and command overrides during switchover.
pub async fn update_component_for_switchover(
    pool: &DbPool,
    component_id: impl Into<DbUuid>,
    new_agent_id: &DbUuid,
    check_override: &Option<String>,
    start_override: &Option<String>,
    stop_override: &Option<String>,
) -> Result<(), sqlx::Error> {
    let component_id = component_id.into();
    sqlx::query(&format!(
        "UPDATE components SET agent_id = $2, check_cmd = COALESCE($3, check_cmd), start_cmd = COALESCE($4, start_cmd), stop_cmd = COALESCE($5, stop_cmd), updated_at = {} WHERE id = $1",
        crate::db::sql::now()
    ))
    .bind(component_id)
    .bind(new_agent_id)
    .bind(check_override)
    .bind(start_override)
    .bind(stop_override)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a config version snapshot for switchover.
pub async fn record_switchover_config_version(
    pool: &DbPool,
    component_id: impl Into<DbUuid>,
    initiated_by: Uuid,
    before: Value,
    after: Value,
) -> Result<(), sqlx::Error> {
    let component_id = component_id.into();
    #[cfg(feature = "postgres")]
    sqlx::query("INSERT INTO config_versions (resource_type, resource_id, changed_by, before_snapshot, after_snapshot) VALUES ('component_switchover', $1, $2, $3, $4)")
        .bind(component_id)
        .bind(DbUuid::from(initiated_by))
        .bind(DbJson::from(before))
        .bind(DbJson::from(after))
        .execute(pool)
        .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("INSERT INTO config_versions (id, resource_type, resource_id, changed_by, before_snapshot, after_snapshot) VALUES ($1, 'component_switchover', $2, $3, $4, $5)")
        .bind(crate::db::bind_id(uuid::Uuid::new_v4()))
        .bind(component_id)
        .bind(DbUuid::from(initiated_by))
        .bind(DbJson::from(before))
        .bind(DbJson::from(after))
        .execute(pool)
        .await?;
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
