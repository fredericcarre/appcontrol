//! Team repository — all team-related database queries.

use async_trait::async_trait;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::db::{DbPool, DbUuid};

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug)]
pub struct Team {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct TeamMember {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub email: String,
    pub display_name: Option<String>,
}

// ============================================================================
// Repository trait
// ============================================================================

#[async_trait]
pub trait TeamRepository: Send + Sync {
    /// List teams for an organization.
    async fn list_teams(&self, org_id: Uuid) -> Result<Vec<Team>, sqlx::Error>;

    /// Get a single team by ID.
    async fn get_team(&self, id: Uuid) -> Result<Option<Team>, sqlx::Error>;

    /// Create a new team and add creator as lead.
    async fn create_team(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
        creator_id: Uuid,
    ) -> Result<Team, sqlx::Error>;

    /// Update a team.
    async fn update_team(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<Team>, sqlx::Error>;

    /// Delete a team. Returns true if deleted.
    async fn delete_team(&self, id: Uuid) -> Result<bool, sqlx::Error>;

    /// List members of a team.
    async fn list_members(&self, team_id: Uuid) -> Result<Vec<TeamMember>, sqlx::Error>;

    /// Add a member to a team. Returns the new membership ID.
    async fn add_member(
        &self,
        team_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> Result<Uuid, sqlx::Error>;

    /// Remove a member from a team.
    async fn remove_member(&self, team_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error>;

    /// Check if a user is a lead of a team.
    async fn is_team_lead(&self, team_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error>;
}

// ============================================================================
// PostgreSQL implementation
// ============================================================================

#[cfg(feature = "postgres")]
pub struct PgTeamRepository {
    pool: DbPool,
}

#[cfg(feature = "postgres")]
impl PgTeamRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[derive(Debug, sqlx::FromRow)]
struct PgTeamRow {
    id: Uuid,
    organization_id: Uuid,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "postgres")]
impl From<PgTeamRow> for Team {
    fn from(r: PgTeamRow) -> Self {
        Team {
            id: r.id,
            organization_id: r.organization_id,
            name: r.name,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl TeamRepository for PgTeamRepository {
    async fn list_teams(&self, org_id: Uuid) -> Result<Vec<Team>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PgTeamRow>(
            "SELECT id, organization_id, name, description, created_at, updated_at \
             FROM teams WHERE organization_id = $1 ORDER BY name",
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_team(&self, id: Uuid) -> Result<Option<Team>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgTeamRow>(
            "SELECT id, organization_id, name, description, created_at, updated_at \
             FROM teams WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn create_team(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
        creator_id: Uuid,
    ) -> Result<Team, sqlx::Error> {
        let row = sqlx::query_as::<_, PgTeamRow>(
            "INSERT INTO teams (id, organization_id, name, description) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, organization_id, name, description, created_at, updated_at",
        )
        .bind(id)
        .bind(org_id)
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;

        // Add creator as lead
        let _ = sqlx::query(
            "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'lead')",
        )
        .bind(id)
        .bind(creator_id)
        .execute(&self.pool)
        .await;

        Ok(row.into())
    }

    async fn update_team(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<Team>, sqlx::Error> {
        let row = sqlx::query_as::<_, PgTeamRow>(
            &format!(
                "UPDATE teams SET \
                    name = COALESCE($2, name), \
                    description = COALESCE($3, description), \
                    updated_at = {} \
                 WHERE id = $1 \
                 RETURNING id, organization_id, name, description, created_at, updated_at",
                crate::db::sql::now()
            ),
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_team(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM teams WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_members(&self, team_id: Uuid) -> Result<Vec<TeamMember>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                chrono::DateTime<chrono::Utc>,
                String,
                Option<String>,
            ),
        >(
            "SELECT tm.id, tm.user_id, tm.role, tm.joined_at, u.email, u.display_name \
             FROM team_members tm JOIN users u ON u.id = tm.user_id \
             WHERE tm.team_id = $1 ORDER BY tm.role, u.display_name, u.email",
        )
        .bind(team_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, user_id, role, joined_at, email, display_name)| TeamMember {
                id,
                user_id,
                role,
                joined_at,
                email,
                display_name,
            })
            .collect())
    }

    async fn add_member(
        &self,
        team_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(team_id)
        .bind(user_id)
        .bind(role)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn remove_member(&self, team_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(team_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn is_team_lead(&self, team_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'lead')",
        )
        .bind(team_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }
}

// ============================================================================
// SQLite implementation
// ============================================================================

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub struct SqliteTeamRepository {
    pool: DbPool,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl SqliteTeamRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[derive(Debug, sqlx::FromRow)]
struct SqliteTeamRow {
    id: DbUuid,
    organization_id: DbUuid,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
impl From<SqliteTeamRow> for Team {
    fn from(r: SqliteTeamRow) -> Self {
        Team {
            id: r.id.into_inner(),
            organization_id: r.organization_id.into_inner(),
            name: r.name,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
#[async_trait]
impl TeamRepository for SqliteTeamRepository {
    async fn list_teams(&self, org_id: Uuid) -> Result<Vec<Team>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SqliteTeamRow>(
            "SELECT id, organization_id, name, description, created_at, updated_at \
             FROM teams WHERE organization_id = $1 ORDER BY name",
        )
        .bind(DbUuid::from(org_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_team(&self, id: Uuid) -> Result<Option<Team>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteTeamRow>(
            "SELECT id, organization_id, name, description, created_at, updated_at \
             FROM teams WHERE id = $1",
        )
        .bind(DbUuid::from(id))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn create_team(
        &self,
        id: Uuid,
        org_id: Uuid,
        name: &str,
        description: Option<&str>,
        creator_id: Uuid,
    ) -> Result<Team, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteTeamRow>(
            "INSERT INTO teams (id, organization_id, name, description) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, organization_id, name, description, created_at, updated_at",
        )
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(org_id))
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;

        // Add creator as lead
        let _ = sqlx::query(
            "INSERT INTO team_members (id, team_id, user_id, role) VALUES ($1, $2, $3, 'lead')",
        )
        .bind(DbUuid::new_v4())
        .bind(DbUuid::from(id))
        .bind(DbUuid::from(creator_id))
        .execute(&self.pool)
        .await;

        Ok(row.into())
    }

    async fn update_team(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Option<Team>, sqlx::Error> {
        let row = sqlx::query_as::<_, SqliteTeamRow>(
            &format!(
                "UPDATE teams SET \
                    name = COALESCE($2, name), \
                    description = COALESCE($3, description), \
                    updated_at = {} \
                 WHERE id = $1 \
                 RETURNING id, organization_id, name, description, created_at, updated_at",
                crate::db::sql::now()
            ),
        )
        .bind(DbUuid::from(id))
        .bind(name)
        .bind(description)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_team(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM teams WHERE id = $1")
            .bind(DbUuid::from(id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_members(&self, team_id: Uuid) -> Result<Vec<TeamMember>, sqlx::Error> {
        let rows = sqlx::query_as::<
            _,
            (
                DbUuid,
                DbUuid,
                String,
                chrono::DateTime<chrono::Utc>,
                String,
                Option<String>,
            ),
        >(
            "SELECT tm.id, tm.user_id, tm.role, tm.joined_at, u.email, u.display_name \
             FROM team_members tm JOIN users u ON u.id = tm.user_id \
             WHERE tm.team_id = $1 ORDER BY tm.role, u.display_name, u.email",
        )
        .bind(DbUuid::from(team_id))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, user_id, role, joined_at, email, display_name)| TeamMember {
                id: id.into_inner(),
                user_id: user_id.into_inner(),
                role,
                joined_at,
                email,
                display_name,
            })
            .collect())
    }

    async fn add_member(
        &self,
        team_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let new_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO team_members (id, team_id, user_id, role) VALUES ($1, $2, $3, $4)",
        )
        .bind(DbUuid::from(new_id))
        .bind(DbUuid::from(team_id))
        .bind(DbUuid::from(user_id))
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(new_id)
    }

    async fn remove_member(&self, team_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(DbUuid::from(team_id))
            .bind(DbUuid::from(user_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn is_team_lead(&self, team_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2 AND role = 'lead'",
        )
        .bind(DbUuid::from(team_id))
        .bind(DbUuid::from(user_id))
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }
}

// ============================================================================
// Factory function
// ============================================================================

pub fn create_team_repository(pool: DbPool) -> Box<dyn TeamRepository> {
    #[cfg(feature = "postgres")]
    {
        Box::new(PgTeamRepository::new(pool))
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        Box::new(SqliteTeamRepository::new(pool))
    }
}
