use crate::db::{self, DbPool, DbUuid};
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

/// Compute the effective permission for a user on an application.
///
/// Algorithm:
/// 1. Check org role — admin = Owner everywhere
/// 2. Get direct permission from app_permissions_users (check expires_at)
/// 3. Get all team permissions from app_permissions_teams (via team_members)
/// 4. Return MAX of all
pub async fn effective_permission(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    app_id: impl Into<Uuid>,
    is_org_admin: bool,
) -> PermissionLevel {
    let user_id: Uuid = user_id.into();
    let app_id: Uuid = app_id.into();
    // 1. Org admin = implicit Owner
    if is_org_admin {
        return PermissionLevel::Owner;
    }

    // 2. Direct user permission
    let direct_sql = format!(
        "SELECT permission_level FROM app_permissions_users \
         WHERE application_id = $1 AND user_id = $2 \
         AND (expires_at IS NULL OR expires_at > {})",
        db::sql::now()
    );
    let direct = sqlx::query_scalar::<_, String>(&direct_sql)
        .bind(app_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|s| PermissionLevel::from_str_level(&s))
        .unwrap_or(PermissionLevel::None);

    // 3. Team permissions
    let team_sql = format!(
        "SELECT apt.permission_level \
         FROM app_permissions_teams apt \
         JOIN team_members tm ON tm.team_id = apt.team_id \
         WHERE apt.application_id = $1 AND tm.user_id = $2 \
         AND (apt.expires_at IS NULL OR apt.expires_at > {})",
        db::sql::now()
    );
    let team_perms = sqlx::query_as::<_, (String,)>(&team_sql)
        .bind(app_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let max_team = team_perms
        .iter()
        .filter_map(|(s,)| PermissionLevel::from_str_level(s))
        .max()
        .unwrap_or(PermissionLevel::None);

    // 4. Return MAX
    std::cmp::max(direct, max_team)
}

/// Check if a user can access a specific site through workspace membership.
///
/// Access rules:
/// - Org admin → always true (implicit access to all sites)
/// - If NO workspace_sites rows exist in the org → open access (workspace feature not configured)
/// - Otherwise, user must be in a workspace that includes the given site
///   (directly as user, or via team membership)
pub async fn can_access_site(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    site_id: impl Into<Uuid>,
    organization_id: impl Into<Uuid>,
    is_org_admin: bool,
) -> bool {
    let user_id: Uuid = user_id.into();
    let site_id: Uuid = site_id.into();
    let organization_id: Uuid = organization_id.into();
    // Org admin bypasses all workspace restrictions
    if is_org_admin {
        return true;
    }

    // Check if workspace-site access control is configured at all.
    // If no workspace_sites rows exist for this org, the feature is not enabled → open access.
    let has_any_workspace_sites = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM workspace_sites ws
            JOIN workspaces w ON w.id = ws.workspace_id
            WHERE w.organization_id = $1
        )
        "#,
    )
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if !has_any_workspace_sites {
        return true; // Workspace feature not configured → open access
    }

    // Check if user has access to this site via workspace membership
    // (direct user membership OR team membership)
    let has_access = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM workspace_sites ws
            JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
            WHERE ws.site_id = $1
              AND (
                  wm.user_id = $2
                  OR wm.team_id IN (
                      SELECT team_id FROM team_members WHERE user_id = $2
                  )
              )
        )
        "#,
    )
    .bind(site_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    has_access
}

/// Check if a user can operate on a specific component.
/// This combines app-level permission AND site-level workspace access.
///
/// Returns the effective permission level if the user has site access,
/// or None if they lack site access entirely.
pub async fn can_operate_component(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    component_id: impl Into<Uuid>,
    is_org_admin: bool,
) -> PermissionLevel {
    let user_id: Uuid = user_id.into();
    let component_id: Uuid = component_id.into();
    // Get component's app_id and site info
    let comp_info = sqlx::query_as::<_, (DbUuid, Option<DbUuid>, DbUuid)>(
        r#"
        SELECT c.application_id, a.gateway_id, app.organization_id
        FROM components c
        JOIN applications app ON app.id = c.application_id
        LEFT JOIN agents a ON a.id = c.agent_id
        WHERE c.id = $1
        "#,
    )
    .bind(component_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (app_id, _gateway_id, organization_id) = match comp_info {
        Some((a, g, o)) => (a.into_inner(), g.map(DbUuid::into_inner), o.into_inner()),
        None => return PermissionLevel::None,
    };

    // Check app-level permission
    let app_perm = effective_permission(pool, user_id, app_id, is_org_admin).await;
    if app_perm == PermissionLevel::None {
        return PermissionLevel::None;
    }

    // Check site-level access via application's site
    let site_id = sqlx::query_scalar::<_, DbUuid>("SELECT site_id FROM applications WHERE id = $1")
        .bind(app_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(DbUuid::into_inner);

    if let Some(site_id) = site_id {
        if !can_access_site(pool, user_id, site_id, organization_id, is_org_admin).await {
            return PermissionLevel::None; // Site access denied
        }
    }

    app_perm
}
