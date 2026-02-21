use sqlx::PgPool;
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
    pool: &PgPool,
    user_id: Uuid,
    app_id: Uuid,
    is_org_admin: bool,
) -> PermissionLevel {
    // 1. Org admin = implicit Owner
    if is_org_admin {
        return PermissionLevel::Owner;
    }

    // 2. Direct user permission
    let direct = sqlx::query_scalar::<_, String>(
        r#"
        SELECT permission_level
        FROM app_permissions_users
        WHERE application_id = $1 AND user_id = $2
          AND (expires_at IS NULL OR expires_at > now())
        "#,
    )
    .bind(app_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|s| PermissionLevel::from_str_level(&s))
    .unwrap_or(PermissionLevel::None);

    // 3. Team permissions
    let team_perms = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT apt.permission_level
        FROM app_permissions_teams apt
        JOIN team_members tm ON tm.team_id = apt.team_id
        WHERE apt.application_id = $1 AND tm.user_id = $2
          AND (apt.expires_at IS NULL OR apt.expires_at > now())
        "#,
    )
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
