use crate::db::{DbPool, DbUuid};
use uuid::Uuid;

use appcontrol_common::PermissionLevel;
use crate::repository::core_queries;

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
    let direct = core_queries::get_direct_user_permission(pool, app_id, user_id)
        .await
        .and_then(|s| PermissionLevel::from_str_level(&s))
        .unwrap_or(PermissionLevel::None);

    // 3. Team permissions
    let team_perms = core_queries::get_team_permissions(pool, app_id, user_id).await;

    let max_team = team_perms
        .iter()
        .filter_map(|(s,)| PermissionLevel::from_str_level(s))
        .max()
        .unwrap_or(PermissionLevel::None);

    // 4. Return MAX
    std::cmp::max(direct, max_team)
}

/// Check if a user can access a specific site through workspace membership.
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
    if !core_queries::has_any_workspace_sites(pool, organization_id).await {
        return true; // Workspace feature not configured → open access
    }

    // Check if user has access to this site via workspace membership
    core_queries::has_site_access(pool, site_id, user_id).await
}

/// Check if a user can operate on a specific component.
/// This combines app-level permission AND site-level workspace access.
pub async fn can_operate_component(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    component_id: impl Into<Uuid>,
    is_org_admin: bool,
) -> PermissionLevel {
    let user_id: Uuid = user_id.into();
    let component_id: Uuid = component_id.into();

    // Get component's app_id and site info
    let comp_info = core_queries::get_component_permission_info(pool, component_id).await;

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
    let site_id = core_queries::get_app_site_id(pool, app_id).await;

    if let Some(site_id) = site_id {
        if !can_access_site(pool, user_id, site_id, organization_id, is_org_admin).await {
            return PermissionLevel::None; // Site access denied
        }
    }

    app_perm
}
