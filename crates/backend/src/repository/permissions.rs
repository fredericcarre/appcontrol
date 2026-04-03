//! Permission repository — all permission-related database queries.

use async_trait::async_trait;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct UserPermission {
    pub id: Uuid,
    pub user_id: Uuid,
    pub permission_level: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug)]
pub struct TeamPermission {
    pub id: Uuid,
    pub team_id: Uuid,
    pub permission_level: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug)]
pub struct ShareLink {
    pub id: Uuid,
    pub token: String,
    pub permission_level: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub max_uses: Option<i32>,
    pub current_uses: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait PermissionRepository: Send + Sync {
    /// List user permissions for an application.
    async fn list_user_permissions(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<UserPermission>, sqlx::Error>;

    /// Grant (or upsert) a user permission.
    async fn grant_user_permission(
        &self,
        app_id: Uuid,
        user_id: Uuid,
        level: &str,
        granted_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid, sqlx::Error>;

    /// List team permissions for an application.
    async fn list_team_permissions(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<TeamPermission>, sqlx::Error>;

    /// Grant (or upsert) a team permission.
    async fn grant_team_permission(
        &self,
        app_id: Uuid,
        team_id: Uuid,
        level: &str,
        granted_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid, sqlx::Error>;

    /// Create a share link for an application.
    async fn create_share_link(
        &self,
        app_id: Uuid,
        token: &str,
        level: &str,
        created_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        max_uses: Option<i32>,
    ) -> Result<ShareLink, sqlx::Error>;

    /// List share links for an application.
    async fn list_share_links(&self, app_id: Uuid) -> Result<Vec<ShareLink>, sqlx::Error>;

    /// Revoke a share link.
    async fn revoke_share_link(&self, id: Uuid, app_id: Uuid) -> Result<bool, sqlx::Error>;

    /// Look up a share link by token.
    async fn get_share_link_by_token(
        &self,
        token: &str,
    ) -> Result<Option<ShareLink>, sqlx::Error>;

    /// Get (site_id, organization_id) for an application.
    async fn app_site_info(&self, app_id: Uuid) -> Result<Option<(Uuid, Uuid)>, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgPermissionRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgPermissionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl PermissionRepository for PgPermissionRepository {
    async fn list_user_permissions(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
            ),
        >(
            "SELECT id, user_id, permission_level, expires_at \
             FROM app_permissions_users WHERE application_id = $1",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, user_id, permission_level, expires_at)| UserPermission {
                id,
                user_id,
                permission_level,
                expires_at,
            })
            .collect())
    }

    async fn grant_user_permission(
        &self,
        app_id: Uuid,
        user_id: Uuid,
        level: &str,
        granted_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid, sqlx::Error> {
        let id = sqlx::query_scalar::<_, Uuid>(
            &format!(
                "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by, expires_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (application_id, user_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = {} \
                 RETURNING id",
                crate::db::sql::now()
            ),
        )
        .bind(app_id)
        .bind(user_id)
        .bind(level)
        .bind(granted_by)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn list_team_permissions(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<TeamPermission>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
            ),
        >(
            "SELECT id, team_id, permission_level, expires_at \
             FROM app_permissions_teams WHERE application_id = $1",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, team_id, permission_level, expires_at)| TeamPermission {
                id,
                team_id,
                permission_level,
                expires_at,
            })
            .collect())
    }

    async fn grant_team_permission(
        &self,
        app_id: Uuid,
        team_id: Uuid,
        level: &str,
        granted_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid, sqlx::Error> {
        let id = sqlx::query_scalar::<_, Uuid>(
            &format!(
                "INSERT INTO app_permissions_teams (application_id, team_id, permission_level, granted_by, expires_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (application_id, team_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = {} \
                 RETURNING id",
                crate::db::sql::now()
            ),
        )
        .bind(app_id)
        .bind(team_id)
        .bind(level)
        .bind(granted_by)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn create_share_link(
        &self,
        app_id: Uuid,
        token: &str,
        level: &str,
        created_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        max_uses: Option<i32>,
    ) -> Result<ShareLink, sqlx::Error> {
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "INSERT INTO share_links (application_id, token, permission_level, created_by, expires_at, max_uses) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, token, permission_level, expires_at, max_uses, current_uses, created_at",
        )
        .bind(app_id)
        .bind(token)
        .bind(level)
        .bind(created_by)
        .bind(expires_at)
        .bind(max_uses)
        .fetch_one(&self.pool)
        .await?;

        Ok(ShareLink {
            id: row.0,
            token: row.1,
            permission_level: row.2,
            expires_at: row.3,
            max_uses: row.4,
            current_uses: row.5,
            created_at: row.6,
        })
    }

    async fn list_share_links(&self, app_id: Uuid) -> Result<Vec<ShareLink>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "SELECT id, token, permission_level, expires_at, max_uses, current_uses, created_at \
             FROM share_links WHERE application_id = $1 ORDER BY created_at DESC",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ShareLink {
                id: r.0,
                token: r.1,
                permission_level: r.2,
                expires_at: r.3,
                max_uses: r.4,
                current_uses: r.5,
                created_at: r.6,
            })
            .collect())
    }

    async fn revoke_share_link(&self, id: Uuid, app_id: Uuid) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM share_links WHERE id = $1 AND application_id = $2")
                .bind(id)
                .bind(app_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_share_link_by_token(
        &self,
        token: &str,
    ) -> Result<Option<ShareLink>, sqlx::Error> {
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "SELECT id, token, permission_level, expires_at, max_uses, current_uses, created_at \
             FROM share_links WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| ShareLink {
            id: r.0,
            token: r.1,
            permission_level: r.2,
            expires_at: r.3,
            max_uses: r.4,
            current_uses: r.5,
            created_at: r.6,
        }))
    }

    async fn app_site_info(&self, app_id: Uuid) -> Result<Option<(Uuid, Uuid)>, sqlx::Error> {
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT site_id, organization_id FROM applications WHERE id = $1",
        )
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqlitePermissionRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqlitePermissionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl PermissionRepository for SqlitePermissionRepository {
    async fn list_user_permissions(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<UserPermission>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                DbUuid,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
            ),
        >(
            "SELECT id, user_id, permission_level, expires_at \
             FROM app_permissions_users WHERE application_id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, user_id, permission_level, expires_at)| UserPermission {
                id: id.into_inner(),
                user_id: user_id.into_inner(),
                permission_level,
                expires_at,
            })
            .collect())
    }

    async fn grant_user_permission(
        &self,
        app_id: Uuid,
        user_id: Uuid,
        level: &str,
        granted_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid, sqlx::Error> {
        let id = sqlx::query_scalar::<_, DbUuid>(
            &format!(
                "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by, expires_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (application_id, user_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = {} \
                 RETURNING id",
                crate::db::sql::now()
            ),
        )
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(user_id))
        .bind(level)
        .bind(DbUuid::from(granted_by))
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id.into_inner())
    }

    async fn list_team_permissions(
        &self,
        app_id: Uuid,
    ) -> Result<Vec<TeamPermission>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                DbUuid,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
            ),
        >(
            "SELECT id, team_id, permission_level, expires_at \
             FROM app_permissions_teams WHERE application_id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, team_id, permission_level, expires_at)| TeamPermission {
                id: id.into_inner(),
                team_id: team_id.into_inner(),
                permission_level,
                expires_at,
            })
            .collect())
    }

    async fn grant_team_permission(
        &self,
        app_id: Uuid,
        team_id: Uuid,
        level: &str,
        granted_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid, sqlx::Error> {
        let id = sqlx::query_scalar::<_, DbUuid>(
            &format!(
                "INSERT INTO app_permissions_teams (application_id, team_id, permission_level, granted_by, expires_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (application_id, team_id) DO UPDATE SET permission_level = $3, expires_at = $5, updated_at = {} \
                 RETURNING id",
                crate::db::sql::now()
            ),
        )
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(team_id))
        .bind(level)
        .bind(DbUuid::from(granted_by))
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id.into_inner())
    }

    async fn create_share_link(
        &self,
        app_id: Uuid,
        token: &str,
        level: &str,
        created_by: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        max_uses: Option<i32>,
    ) -> Result<ShareLink, sqlx::Error> {
        let new_id = DbUuid::new_v4();
        let row = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "INSERT INTO share_links (id, application_id, token, permission_level, created_by, expires_at, max_uses) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id, token, permission_level, expires_at, max_uses, current_uses, created_at",
        )
        .bind(new_id)
        .bind(DbUuid::from(app_id))
        .bind(token)
        .bind(level)
        .bind(DbUuid::from(created_by))
        .bind(expires_at)
        .bind(max_uses)
        .fetch_one(&self.pool)
        .await?;

        Ok(ShareLink {
            id: row.0.into_inner(),
            token: row.1,
            permission_level: row.2,
            expires_at: row.3,
            max_uses: row.4,
            current_uses: row.5,
            created_at: row.6,
        })
    }

    async fn list_share_links(&self, app_id: Uuid) -> Result<Vec<ShareLink>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "SELECT id, token, permission_level, expires_at, max_uses, current_uses, created_at \
             FROM share_links WHERE application_id = $1 ORDER BY created_at DESC",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ShareLink {
                id: r.0.into_inner(),
                token: r.1,
                permission_level: r.2,
                expires_at: r.3,
                max_uses: r.4,
                current_uses: r.5,
                created_at: r.6,
            })
            .collect())
    }

    async fn revoke_share_link(&self, id: Uuid, app_id: Uuid) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM share_links WHERE id = $1 AND application_id = $2")
                .bind(DbUuid::from(id))
                .bind(DbUuid::from(app_id))
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_share_link_by_token(
        &self,
        token: &str,
    ) -> Result<Option<ShareLink>, sqlx::Error> {
        let row = sqlx::query_as::<
            _,
            (
                DbUuid,
                String,
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<i32>,
                i32,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "SELECT id, token, permission_level, expires_at, max_uses, current_uses, created_at \
             FROM share_links WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| ShareLink {
            id: r.0.into_inner(),
            token: r.1,
            permission_level: r.2,
            expires_at: r.3,
            max_uses: r.4,
            current_uses: r.5,
            created_at: r.6,
        }))
    }

    async fn app_site_info(&self, app_id: Uuid) -> Result<Option<(Uuid, Uuid)>, sqlx::Error> {
        let row: Option<(DbUuid, DbUuid)> = sqlx::query_as(
            "SELECT site_id, organization_id FROM applications WHERE id = $1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(s, o)| (s.into_inner(), o.into_inner())))
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_permission_repository(pool: DbPool) -> Box<dyn PermissionRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgPermissionRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqlitePermissionRepository::new(pool))
    }
}

// ============================================================================
// Free functions (api/permissions.rs queries)
// ============================================================================

/// Check if workspace feature is configured for an organization.
pub async fn has_workspace_sites(pool: &DbPool, org_id: Uuid) -> bool {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM workspace_sites ws JOIN workspaces w ON w.id = ws.workspace_id WHERE w.organization_id = $1)",
        )
        .bind(org_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let count: i32 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workspace_sites ws JOIN workspaces w ON w.id = ws.workspace_id WHERE w.organization_id = $1",
        )
        .bind(DbUuid::from(org_id))
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        count > 0
    }
}

/// Check if a team has workspace access to a site.
pub async fn team_has_site_access(pool: &DbPool, site_id: Uuid, team_id: Uuid) -> bool {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM workspace_sites ws
                JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
                WHERE ws.site_id = $1 AND wm.team_id = $2
            )
            "#,
        )
        .bind(site_id)
        .bind(team_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let count: i32 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM workspace_sites ws
            JOIN workspace_members wm ON wm.workspace_id = ws.workspace_id
            WHERE ws.site_id = $1 AND wm.team_id = $2
            "#,
        )
        .bind(DbUuid::from(site_id))
        .bind(DbUuid::from(team_id))
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        count > 0
    }
}

/// List active share links for an application.
pub async fn list_active_share_links(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, String, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (DbUuid, String, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>(
            "SELECT id, token, permission_level, expires_at, max_uses, use_count FROM app_share_links WHERE application_id = $1 AND is_active = true",
        )
        .bind(app_id)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (DbUuid, String, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>(
            "SELECT id, token, permission_level, expires_at, max_uses, use_count FROM app_share_links WHERE application_id = $1 AND is_active = 1",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(pool)
        .await
    }
}

/// Create a share link. Returns the new ID.
pub async fn insert_share_link(
    pool: &DbPool,
    app_id: Uuid,
    token: &str,
    permission_level: &str,
    created_by: Uuid,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    max_uses: Option<i32>,
) -> Result<DbUuid, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_scalar::<_, DbUuid>(
            "INSERT INTO app_share_links (application_id, token, permission_level, created_by, expires_at, max_uses)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(app_id)
        .bind(token)
        .bind(permission_level)
        .bind(created_by)
        .bind(expires_at)
        .bind(max_uses)
        .fetch_one(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let new_id = DbUuid::new_v4();
        sqlx::query(
            "INSERT INTO app_share_links (id, application_id, token, permission_level, created_by, expires_at, max_uses)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(new_id)
        .bind(DbUuid::from(app_id))
        .bind(token)
        .bind(permission_level)
        .bind(DbUuid::from(created_by))
        .bind(expires_at)
        .bind(max_uses)
        .execute(pool)
        .await?;
        Ok(new_id)
    }
}

/// Delete a user permission entry. Returns rows affected.
pub async fn delete_user_permission(
    pool: &DbPool,
    perm_id: Uuid,
    app_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM app_permissions_users WHERE id = $1 AND application_id = $2")
        .bind(crate::db::bind_id(perm_id))
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Delete a team permission entry. Returns rows affected.
pub async fn delete_team_permission(
    pool: &DbPool,
    perm_id: Uuid,
    app_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM app_permissions_teams WHERE id = $1 AND application_id = $2")
        .bind(crate::db::bind_id(perm_id))
        .bind(crate::db::bind_id(app_id))
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Search users in an organization (empty query returns all).
pub async fn list_org_users(
    pool: &DbPool,
    org_id: Uuid,
    limit: i64,
) -> Result<Vec<(DbUuid, String, Option<String>, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
            "SELECT id, email, display_name, role FROM users WHERE organization_id = $1 AND is_active = true ORDER BY display_name, email LIMIT $2",
        )
        .bind(org_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
            "SELECT id, email, display_name, role FROM users WHERE organization_id = $1 AND is_active = 1 ORDER BY display_name, email LIMIT $2",
        )
        .bind(DbUuid::from(org_id))
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

/// Search users by pattern (ILIKE for postgres, LIKE for sqlite).
pub async fn search_users_by_pattern(
    pool: &DbPool,
    org_id: Uuid,
    pattern: &str,
    limit: i64,
) -> Result<Vec<(DbUuid, String, Option<String>, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
            r#"SELECT id, email, display_name, role FROM users
               WHERE organization_id = $1 AND is_active = true
               AND (email ILIKE $2 OR display_name ILIKE $2)
               ORDER BY display_name, email LIMIT $3"#,
        )
        .bind(org_id)
        .bind(pattern)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (DbUuid, String, Option<String>, String)>(
            r#"SELECT id, email, display_name, role FROM users
               WHERE organization_id = $1 AND is_active = 1
               AND (email LIKE $2 OR display_name LIKE $2)
               ORDER BY display_name, email LIMIT $3"#,
        )
        .bind(DbUuid::from(org_id))
        .bind(pattern)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

/// Look up a share link by token (for consuming).
pub async fn get_share_link_for_consume(
    pool: &DbPool,
    token: &str,
) -> Result<Option<(DbUuid, DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (DbUuid, DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>(
            "SELECT id, application_id, permission_level, expires_at, max_uses, use_count FROM app_share_links WHERE token = $1 AND is_active = true",
        )
        .bind(token)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (DbUuid, DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32)>(
            "SELECT id, application_id, permission_level, expires_at, max_uses, use_count FROM app_share_links WHERE token = $1 AND is_active = 1",
        )
        .bind(token)
        .fetch_optional(pool)
        .await
    }
}

/// Grant permission to user via share link (upsert).
pub async fn grant_permission_via_share_link(
    pool: &DbPool,
    app_id: Uuid,
    user_id: Uuid,
    permission_level: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        &format!(
            "INSERT INTO app_permissions_users (application_id, user_id, permission_level, granted_by, expires_at)
             VALUES ($1, $2, $3, $4, NULL)
             ON CONFLICT (application_id, user_id) DO UPDATE SET
                 permission_level = CASE
                     WHEN EXCLUDED.permission_level > app_permissions_users.permission_level
                     THEN EXCLUDED.permission_level
                     ELSE app_permissions_users.permission_level
                 END,
                 updated_at = {}",
            crate::db::sql::now()
        ),
    )
    .bind(crate::db::bind_id(app_id))
    .bind(crate::db::bind_id(user_id))
    .bind(permission_level)
    .bind(crate::db::bind_id(user_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment share link use count.
pub async fn increment_share_link_use_count(
    pool: &DbPool,
    link_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE app_share_links SET use_count = use_count + 1 WHERE id = $1")
        .bind(crate::db::bind_id(link_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Revoke a share link (set is_active = false). Returns rows affected.
pub async fn revoke_share_link_by_id(
    pool: &DbPool,
    link_id: Uuid,
    app_id: Uuid,
) -> Result<u64, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let result = sqlx::query(
            "UPDATE app_share_links SET is_active = false WHERE id = $1 AND application_id = $2",
        )
        .bind(link_id)
        .bind(app_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let result = sqlx::query(
            "UPDATE app_share_links SET is_active = 0 WHERE id = $1 AND application_id = $2",
        )
        .bind(DbUuid::from(link_id))
        .bind(DbUuid::from(app_id))
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}

/// List all permissions (users + teams) for an application.
pub async fn list_all_user_permissions(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, DbUuid, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT apu.id, apu.user_id, apu.permission_level, u.email, apu.expires_at
           FROM app_permissions_users apu
           LEFT JOIN users u ON u.id = apu.user_id
           WHERE apu.application_id = $1"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// List all team permissions for an application (with team names).
pub async fn list_all_team_permissions(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<(DbUuid, DbUuid, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT apt.id, apt.team_id, apt.permission_level, t.name, apt.expires_at
           FROM app_permissions_teams apt
           LEFT JOIN teams t ON t.id = apt.team_id
           WHERE apt.application_id = $1"#,
    )
    .bind(crate::db::bind_id(app_id))
    .fetch_all(pool)
    .await
}

/// Get share link info (public preview).
pub async fn get_share_link_info(
    pool: &DbPool,
    token: &str,
) -> Result<Option<(DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32, String)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        sqlx::query_as::<_, (DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32, String)>(
            r#"SELECT sl.application_id, sl.permission_level, sl.expires_at, sl.max_uses, sl.use_count, a.name
               FROM app_share_links sl JOIN applications a ON a.id = sl.application_id
               WHERE sl.token = $1 AND sl.is_active = true"#,
        )
        .bind(token)
        .fetch_optional(pool)
        .await
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query_as::<_, (DbUuid, String, Option<chrono::DateTime<chrono::Utc>>, Option<i32>, i32, String)>(
            r#"SELECT sl.application_id, sl.permission_level, sl.expires_at, sl.max_uses, sl.use_count, a.name
               FROM app_share_links sl JOIN applications a ON a.id = sl.application_id
               WHERE sl.token = $1 AND sl.is_active = 1"#,
        )
        .bind(token)
        .fetch_optional(pool)
        .await
    }
}
