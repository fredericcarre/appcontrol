//! Permission repository — all permission-related database queries.

use async_trait::async_trait;
use uuid::Uuid;

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
