//! Query functions for misc domain (links, variables, groups, audit, rate-limit, users, break-glass, etc).

#![allow(unused_imports, dead_code)]
use crate::db::{self, DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

// ============================================================================
// Component Links
// ============================================================================

/// Link row returned from queries.
#[derive(Debug, serde::Serialize)]
pub struct LinkInfo {
    pub id: Uuid,
    pub component_id: Uuid,
    pub label: String,
    pub url: String,
    pub link_type: String,
    pub display_order: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// Internal row type for sqlx
#[derive(Debug, sqlx::FromRow)]
struct LinkRow {
    #[cfg(feature = "postgres")]
    id: Uuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    id: DbUuid,
    #[cfg(feature = "postgres")]
    component_id: Uuid,
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    component_id: DbUuid,
    label: String,
    url: String,
    link_type: String,
    display_order: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl LinkRow {
    fn into_info(self) -> LinkInfo {
        LinkInfo {
            #[cfg(feature = "postgres")]
            id: self.id,
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            id: self.id.into_inner(),
            #[cfg(feature = "postgres")]
            component_id: self.component_id,
            #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
            component_id: self.component_id.into_inner(),
            label: self.label,
            url: self.url,
            link_type: self.link_type,
            display_order: self.display_order,
            created_at: self.created_at,
        }
    }
}

/// Get component's application_id (needed for permission checks).
pub async fn get_component_app_id_checked(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    crate::repository::queries::get_component_app_id(pool, component_id).await
}

/// List all links for a component.
pub async fn list_component_links(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Vec<LinkInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let rows = sqlx::query_as::<_, LinkRow>(
        "SELECT id, component_id, label, url, link_type, display_order, created_at \
         FROM component_links WHERE component_id = $1 ORDER BY display_order, label",
    )
    .bind(component_id)
    .fetch_all(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rows = sqlx::query_as::<_, LinkRow>(
        "SELECT id, component_id, label, url, link_type, display_order, created_at \
         FROM component_links WHERE component_id = $1 ORDER BY display_order, label",
    )
    .bind(DbUuid::from(component_id))
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.into_info()).collect())
}

/// Create a new component link.
pub async fn create_component_link(
    pool: &DbPool,
    link_id: Uuid,
    component_id: Uuid,
    label: &str,
    url: &str,
    link_type: &str,
    display_order: i32,
) -> Result<LinkInfo, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let row = sqlx::query_as::<_, LinkRow>(
        r#"INSERT INTO component_links (id, component_id, label, url, link_type, display_order)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, component_id, label, url, link_type, display_order, created_at"#,
    )
    .bind(link_id)
    .bind(component_id)
    .bind(label)
    .bind(url)
    .bind(link_type)
    .bind(display_order)
    .fetch_one(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = sqlx::query_as::<_, LinkRow>(
        r#"INSERT INTO component_links (id, component_id, label, url, link_type, display_order)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, component_id, label, url, link_type, display_order, created_at"#,
    )
    .bind(DbUuid::from(link_id))
    .bind(DbUuid::from(component_id))
    .bind(label)
    .bind(url)
    .bind(link_type)
    .bind(display_order)
    .fetch_one(pool)
    .await?;

    Ok(row.into_info())
}

/// Update a component link.
pub async fn update_component_link(
    pool: &DbPool,
    component_id: Uuid,
    link_id: Uuid,
    label: Option<&str>,
    url: Option<&str>,
    link_type: Option<&str>,
    display_order: Option<i32>,
) -> Result<Option<LinkInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let row = sqlx::query_as::<_, LinkRow>(
        r#"UPDATE component_links SET
               label = COALESCE($3, label),
               url = COALESCE($4, url),
               link_type = COALESCE($5, link_type),
               display_order = COALESCE($6, display_order)
           WHERE id = $2 AND component_id = $1
           RETURNING id, component_id, label, url, link_type, display_order, created_at"#,
    )
    .bind(component_id)
    .bind(link_id)
    .bind(label)
    .bind(url)
    .bind(link_type)
    .bind(display_order)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = sqlx::query_as::<_, LinkRow>(
        r#"UPDATE component_links SET
               label = COALESCE($3, label),
               url = COALESCE($4, url),
               link_type = COALESCE($5, link_type),
               display_order = COALESCE($6, display_order)
           WHERE id = $2 AND component_id = $1
           RETURNING id, component_id, label, url, link_type, display_order, created_at"#,
    )
    .bind(DbUuid::from(component_id))
    .bind(DbUuid::from(link_id))
    .bind(label)
    .bind(url)
    .bind(link_type)
    .bind(display_order)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.into_info()))
}

/// Delete a component link. Returns true if a row was deleted.
pub async fn delete_component_link(
    pool: &DbPool,
    link_id: Uuid,
    component_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let result = sqlx::query("DELETE FROM component_links WHERE id = $1 AND component_id = $2")
        .bind(link_id)
        .bind(component_id)
        .execute(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query("DELETE FROM component_links WHERE id = $1 AND component_id = $2")
        .bind(DbUuid::from(link_id))
        .bind(DbUuid::from(component_id))
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ============================================================================
// Application Variables
// ============================================================================

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct VariableInfo {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub is_secret: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_app_variables(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<VariableInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, VariableInfo>(
        "SELECT id, application_id, name, value, description, is_secret, created_at, updated_at \
         FROM app_variables WHERE application_id = $1 ORDER BY name",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, VariableInfo>(
        "SELECT id, application_id, name, value, description, is_secret, created_at, updated_at \
         FROM app_variables WHERE application_id = $1 ORDER BY name",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(pool)
    .await;
}

pub async fn create_app_variable(
    pool: &DbPool,
    var_id: Uuid,
    app_id: Uuid,
    name: &str,
    value: &str,
    description: Option<&str>,
    is_secret: bool,
) -> Result<VariableInfo, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, VariableInfo>(
        r#"INSERT INTO app_variables (id, application_id, name, value, description, is_secret)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, application_id, name, value, description, is_secret, created_at, updated_at"#,
    )
    .bind(var_id)
    .bind(app_id)
    .bind(name)
    .bind(value)
    .bind(description)
    .bind(is_secret)
    .fetch_one(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, VariableInfo>(
        r#"INSERT INTO app_variables (id, application_id, name, value, description, is_secret)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, application_id, name, value, description, is_secret, created_at, updated_at"#,
    )
    .bind(DbUuid::from(var_id))
    .bind(DbUuid::from(app_id))
    .bind(name)
    .bind(value)
    .bind(description)
    .bind(is_secret)
    .fetch_one(pool)
    .await;
}

pub async fn update_app_variable(
    pool: &DbPool,
    app_id: Uuid,
    var_id: Uuid,
    value: Option<&str>,
    description: Option<&str>,
    is_secret: Option<bool>,
) -> Result<Option<VariableInfo>, sqlx::Error> {
    let sql = format!(
        "UPDATE app_variables SET \
            value = COALESCE($3, value), \
            description = COALESCE($4, description), \
            is_secret = COALESCE($5, is_secret), \
            updated_at = {} \
        WHERE id = $2 AND application_id = $1 \
        RETURNING id, application_id, name, value, description, is_secret, created_at, updated_at",
        crate::db::sql::now()
    );

    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, VariableInfo>(&sql)
        .bind(app_id)
        .bind(var_id)
        .bind(value)
        .bind(description)
        .bind(is_secret)
        .fetch_optional(pool)
        .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, VariableInfo>(&sql)
        .bind(DbUuid::from(app_id))
        .bind(DbUuid::from(var_id))
        .bind(value)
        .bind(description)
        .bind(is_secret)
        .fetch_optional(pool)
        .await;
}

pub async fn delete_app_variable(
    pool: &DbPool,
    var_id: Uuid,
    app_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let result = sqlx::query("DELETE FROM app_variables WHERE id = $1 AND application_id = $2")
        .bind(var_id)
        .bind(app_id)
        .execute(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query("DELETE FROM app_variables WHERE id = $1 AND application_id = $2")
        .bind(DbUuid::from(var_id))
        .bind(DbUuid::from(app_id))
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Resolve variables for an application into a HashMap (used by executor).
pub async fn resolve_variables(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT name, value FROM app_variables WHERE application_id = $1",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}

// ============================================================================
// Component Groups
// ============================================================================

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct GroupInfo {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub display_order: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_component_groups(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<GroupInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, GroupInfo>(
        "SELECT id, application_id, name, description, color, display_order, created_at \
         FROM component_groups WHERE application_id = $1 ORDER BY display_order, name",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, GroupInfo>(
        "SELECT id, application_id, name, description, color, display_order, created_at \
         FROM component_groups WHERE application_id = $1 ORDER BY display_order, name",
    )
    .bind(DbUuid::from(app_id))
    .fetch_all(pool)
    .await;
}

pub async fn create_component_group(
    pool: &DbPool,
    group_id: Uuid,
    app_id: Uuid,
    name: &str,
    description: Option<&str>,
    color: &str,
    display_order: i32,
) -> Result<GroupInfo, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, GroupInfo>(
        r#"INSERT INTO component_groups (id, application_id, name, description, color, display_order)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, application_id, name, description, color, display_order, created_at"#,
    )
    .bind(group_id)
    .bind(app_id)
    .bind(name)
    .bind(description)
    .bind(color)
    .bind(display_order)
    .fetch_one(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, GroupInfo>(
        r#"INSERT INTO component_groups (id, application_id, name, description, color, display_order)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, application_id, name, description, color, display_order, created_at"#,
    )
    .bind(DbUuid::from(group_id))
    .bind(DbUuid::from(app_id))
    .bind(name)
    .bind(description)
    .bind(color)
    .bind(display_order)
    .fetch_one(pool)
    .await;
}

pub async fn update_component_group(
    pool: &DbPool,
    app_id: Uuid,
    group_id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    color: Option<&str>,
    display_order: Option<i32>,
) -> Result<Option<GroupInfo>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    return sqlx::query_as::<_, GroupInfo>(
        r#"UPDATE component_groups SET
               name = COALESCE($3, name),
               description = COALESCE($4, description),
               color = COALESCE($5, color),
               display_order = COALESCE($6, display_order)
           WHERE id = $2 AND application_id = $1
           RETURNING id, application_id, name, description, color, display_order, created_at"#,
    )
    .bind(app_id)
    .bind(group_id)
    .bind(name)
    .bind(description)
    .bind(color)
    .bind(display_order)
    .fetch_optional(pool)
    .await;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    return sqlx::query_as::<_, GroupInfo>(
        r#"UPDATE component_groups SET
               name = COALESCE($3, name),
               description = COALESCE($4, description),
               color = COALESCE($5, color),
               display_order = COALESCE($6, display_order)
           WHERE id = $2 AND application_id = $1
           RETURNING id, application_id, name, description, color, display_order, created_at"#,
    )
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(group_id))
    .bind(name)
    .bind(description)
    .bind(color)
    .bind(display_order)
    .fetch_optional(pool)
    .await;
}

pub async fn delete_component_group(
    pool: &DbPool,
    group_id: Uuid,
    app_id: Uuid,
) -> Result<bool, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let result = sqlx::query("DELETE FROM component_groups WHERE id = $1 AND application_id = $2")
        .bind(group_id)
        .bind(app_id)
        .execute(pool)
        .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let result = sqlx::query("DELETE FROM component_groups WHERE id = $1 AND application_id = $2")
        .bind(DbUuid::from(group_id))
        .bind(DbUuid::from(app_id))
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ============================================================================
// Audit / Action Log
// ============================================================================

/// Log an action to the action_log table BEFORE the action executes.
/// Returns the action_log ID.
pub async fn log_action(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    action: &str,
    resource_type: &str,
    resource_id: impl Into<Uuid>,
    details: Value,
) -> Result<Uuid, sqlx::Error> {
    let user_id: Uuid = user_id.into();
    let resource_id: Uuid = resource_id.into();

    #[cfg(feature = "postgres")]
    let row = sqlx::query_scalar::<_, DbUuid>(
        "INSERT INTO action_log (user_id, action, resource_type, resource_id, details, status) \
         VALUES ($1, $2, $3, $4, $5, 'in_progress') RETURNING id",
    )
    .bind(crate::db::bind_id(user_id))
    .bind(action)
    .bind(resource_type)
    .bind(crate::db::bind_id(resource_id))
    .bind(&details)
    .fetch_one(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row = {
        let id = DbUuid::from(Uuid::new_v4());
        sqlx::query(
            "INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details, status) \
             VALUES ($1, $2, $3, $4, $5, $6, 'in_progress')",
        )
        .bind(crate::db::bind_id(id))
        .bind(DbUuid::from(user_id))
        .bind(action)
        .bind(resource_type)
        .bind(DbUuid::from(resource_id))
        .bind(serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string()))
        .execute(pool)
        .await?;
        id
    };

    Ok(row.into_inner())
}

/// Mark an action as successfully completed.
pub async fn complete_action_success(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
) -> Result<(), sqlx::Error> {
    let action_id: Uuid = action_id.into();
    let sql = format!(
        "UPDATE action_log SET status = 'success', completed_at = {} WHERE id = $1",
        db::sql::now()
    );
    sqlx::query(&sql)
        .bind(DbUuid::from(action_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark an action as failed with an error message.
pub async fn complete_action_failed(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    let action_id: Uuid = action_id.into();
    let sql = format!(
        "UPDATE action_log SET status = 'failed', error_message = $2, completed_at = {} WHERE id = $1",
        db::sql::now()
    );
    sqlx::query(&sql)
        .bind(DbUuid::from(action_id))
        .bind(error_message)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark an action as cancelled.
pub async fn complete_action_cancelled(
    pool: &DbPool,
    action_id: impl Into<Uuid>,
) -> Result<(), sqlx::Error> {
    let action_id: Uuid = action_id.into();
    let sql = format!(
        "UPDATE action_log SET status = 'cancelled', completed_at = {} WHERE id = $1",
        db::sql::now()
    );
    sqlx::query(&sql)
        .bind(DbUuid::from(action_id))
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Rate Limit (PostgreSQL HA mode)
// ============================================================================

/// PostgreSQL-backed rate limit check (UPSERT + window reset).
/// Returns the current count after increment.
pub async fn check_rate_limit_pg(
    pool: &DbPool,
    key: &str,
    window_secs: i32,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO rate_limit_counters (key, count, window_start)
        VALUES ($1, 1, now())
        ON CONFLICT (key) DO UPDATE SET
            count = CASE
                WHEN rate_limit_counters.window_start < now() - $2 * interval '1 second'
                THEN 1
                ELSE rate_limit_counters.count + 1
            END,
            window_start = CASE
                WHEN rate_limit_counters.window_start < now() - $2 * interval '1 second'
                THEN now()
                ELSE rate_limit_counters.window_start
            END
        RETURNING count
        "#,
    )
    .bind(key)
    .bind(window_secs)
    .fetch_one(pool)
    .await
}

/// Cleanup expired rate limit counters (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn cleanup_rate_limit_counters(pool: &DbPool) {
    let _ = sqlx::query(
        "DELETE FROM rate_limit_counters WHERE window_start < now() - interval '2 minutes'",
    )
    .execute(pool)
    .await;
}

/// Cleanup expired rate limit counters (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn cleanup_rate_limit_counters(pool: &DbPool) {
    let _ = sqlx::query(
        "DELETE FROM rate_limit_counters WHERE window_start < datetime('now', '-2 minutes')",
    )
    .execute(pool)
    .await;
}

// ============================================================================
// User Management
// ============================================================================

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub auth_provider: String,
    pub is_active: bool,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List users in an organization with optional filters.
#[cfg(feature = "postgres")]
pub async fn list_users(
    pool: &DbPool,
    org_id: impl Into<Uuid>,
    role: Option<&str>,
    is_active: Option<bool>,
    search: Option<&str>,
) -> Result<Vec<UserRow>, sqlx::Error> {
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users
           WHERE organization_id = $1
             AND ($2::text IS NULL OR role = $2)
             AND ($3::bool IS NULL OR is_active = $3)
             AND ($4::text IS NULL OR email ILIKE '%' || $4 || '%' OR display_name ILIKE '%' || $4 || '%')
           ORDER BY display_name"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(role)
    .bind(is_active)
    .bind(search)
    .fetch_all(pool)
    .await
}

/// List users in an organization with optional filters (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn list_users(
    pool: &DbPool,
    org_id: impl Into<Uuid>,
    role: Option<&str>,
    is_active: Option<bool>,
    search: Option<&str>,
) -> Result<Vec<UserRow>, sqlx::Error> {
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users
           WHERE organization_id = $1
             AND ($2 IS NULL OR role = $2)
             AND ($3 IS NULL OR is_active = $3)
             AND ($4 IS NULL OR email LIKE '%' || $4 || '%' OR display_name LIKE '%' || $4 || '%')
           ORDER BY display_name"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(role)
    .bind(is_active)
    .bind(search)
    .fetch_all(pool)
    .await
}

/// Get a single user by ID and org.
pub async fn get_user_by_id(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    org_id: impl Into<Uuid>,
) -> Result<Option<UserRow>, sqlx::Error> {
    let user_id: Uuid = user_id.into();
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users
           WHERE id = $1 AND organization_id = $2"#,
    )
    .bind(crate::db::bind_id(user_id))
    .bind(crate::db::bind_id(org_id))
    .fetch_optional(pool)
    .await
}

/// Create a new local user.
pub async fn create_user(
    pool: &DbPool,
    org_id: impl Into<Uuid>,
    external_id: &str,
    email: &str,
    display_name: &str,
    role: &str,
    password_hash: Option<&str>,
) -> Result<UserRow, sqlx::Error> {
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, UserRow>(
        r#"INSERT INTO users (organization_id, external_id, email, display_name, role, auth_provider, password_hash)
           VALUES ($1, $2, $3, $4, $5, 'local', $6)
           RETURNING id, organization_id, email, display_name, role, auth_provider,
                     is_active, last_login_at, created_at"#,
    )
    .bind(crate::db::bind_id(org_id))
    .bind(external_id)
    .bind(email)
    .bind(display_name)
    .bind(role)
    .bind(password_hash)
    .fetch_one(pool)
    .await
}

/// Update a user.
pub async fn update_user(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    org_id: impl Into<Uuid>,
    display_name: Option<&str>,
    role: Option<&str>,
    is_active: Option<bool>,
    password_hash: Option<&str>,
) -> Result<Option<UserRow>, sqlx::Error> {
    let user_id: Uuid = user_id.into();
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, UserRow>(
        r#"UPDATE users SET
               display_name = COALESCE($3, display_name),
               role = COALESCE($4, role),
               is_active = COALESCE($5, is_active),
               password_hash = COALESCE($6, password_hash)
           WHERE id = $1 AND organization_id = $2
           RETURNING id, organization_id, email, display_name, role, auth_provider,
                     is_active, last_login_at, created_at"#,
    )
    .bind(crate::db::bind_id(user_id))
    .bind(crate::db::bind_id(org_id))
    .bind(display_name)
    .bind(role)
    .bind(is_active)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}

/// Get a user by ID only (no org check).
pub async fn get_user_by_id_only(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
) -> Result<Option<UserRow>, sqlx::Error> {
    let user_id: Uuid = user_id.into();
    sqlx::query_as::<_, UserRow>(
        r#"SELECT id, organization_id, email, display_name, role, auth_provider,
                  is_active, last_login_at, created_at
           FROM users WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(user_id))
    .fetch_optional(pool)
    .await
}

/// Get platform_role for a user.
pub async fn get_user_platform_role(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
) -> Result<Option<Option<String>>, sqlx::Error> {
    let user_id: Uuid = user_id.into();
    sqlx::query_scalar("SELECT platform_role FROM users WHERE id = $1")
        .bind(crate::db::bind_id(user_id))
        .fetch_optional(pool)
        .await
}

/// Get auth_provider and password_hash for a user.
pub async fn get_user_auth_info(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
    let user_id: Uuid = user_id.into();
    sqlx::query_as("SELECT auth_provider, password_hash FROM users WHERE id = $1")
        .bind(crate::db::bind_id(user_id))
        .fetch_optional(pool)
        .await
}

/// Update a user's password hash.
pub async fn update_user_password(
    pool: &DbPool,
    user_id: impl Into<Uuid>,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    let user_id: Uuid = user_id.into();
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(crate::db::bind_id(user_id))
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Break-Glass
// ============================================================================

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct BreakGlassSessionRow {
    pub id: DbUuid,
    pub account_id: DbUuid,
    pub organization_id: DbUuid,
    pub activated_by_ip: String,
    pub reason: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub actions_taken: i32,
}

/// Create a break-glass account.
pub async fn create_break_glass_account(
    pool: &DbPool,
    id: Uuid,
    org_id: impl Into<Uuid>,
    username: &str,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    let org_id: Uuid = org_id.into();
    sqlx::query(
        "INSERT INTO break_glass_accounts (id, organization_id, username, password_hash) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(crate::db::bind_id(id))
    .bind(crate::db::bind_id(org_id))
    .bind(username)
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// List break-glass accounts for an org.
pub async fn list_break_glass_accounts(
    pool: &DbPool,
    org_id: impl Into<Uuid>,
) -> Result<Vec<(DbUuid, String, bool, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, (DbUuid, String, bool, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, username, is_active, last_rotated_at FROM break_glass_accounts \
         WHERE organization_id = $1 ORDER BY username",
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_all(pool)
    .await
}

/// Validate break-glass credentials.
#[cfg(feature = "postgres")]
pub async fn find_break_glass_account(
    pool: &DbPool,
    username: &str,
    password_hash: &str,
) -> Result<Option<(DbUuid, DbUuid)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT id, organization_id FROM break_glass_accounts \
         WHERE username = $1 AND password_hash = $2 AND is_active = true",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}

/// Validate break-glass credentials (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn find_break_glass_account(
    pool: &DbPool,
    username: &str,
    password_hash: &str,
) -> Result<Option<(DbUuid, DbUuid)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, DbUuid)>(
        "SELECT id, organization_id FROM break_glass_accounts \
         WHERE username = $1 AND password_hash = $2 AND is_active = 1",
    )
    .bind(username)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}

/// Create a break-glass session.
pub async fn create_break_glass_session(
    pool: &DbPool,
    session_id: Uuid,
    account_id: DbUuid,
    organization_id: DbUuid,
    ip: &str,
    reason: &str,
    duration_minutes: i32,
) -> Result<BreakGlassSessionRow, sqlx::Error> {
    sqlx::query_as::<_, BreakGlassSessionRow>(&format!(
        "INSERT INTO break_glass_sessions (
                id, account_id, organization_id, activated_by_ip, reason, expires_at
            ) VALUES ($1, $2, $3, $4, $5, {} + make_interval(mins => $6))
            RETURNING id, account_id, organization_id, activated_by_ip, reason,
                      started_at, expires_at, ended_at, actions_taken",
        crate::db::sql::now()
    ))
    .bind(session_id)
    .bind(account_id)
    .bind(organization_id)
    .bind(ip)
    .bind(reason)
    .bind(duration_minutes)
    .fetch_one(pool)
    .await
}

/// Log a break-glass activation event in action_log.
pub async fn log_break_glass_activation(
    pool: &DbPool,
    account_id: DbUuid,
    organization_id: DbUuid,
    details: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        &format!(
            "INSERT INTO action_log (id, user_id, action, resource_type, resource_id, details, created_at)
             VALUES ($1, $2, 'break_glass_activated', 'organization', $3, $4, {})",
            crate::db::sql::now()
        ),
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(organization_id)
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

/// List break-glass sessions for an org.
pub async fn list_break_glass_sessions(
    pool: &DbPool,
    org_id: impl Into<Uuid>,
) -> Result<Vec<BreakGlassSessionRow>, sqlx::Error> {
    let org_id: Uuid = org_id.into();
    sqlx::query_as::<_, BreakGlassSessionRow>(
        "SELECT id, account_id, organization_id, activated_by_ip, reason, \
         started_at, expires_at, ended_at, actions_taken \
         FROM break_glass_sessions WHERE organization_id = $1 \
         ORDER BY started_at DESC LIMIT 50",
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_all(pool)
    .await
}

/// End a break-glass session. Returns rows_affected.
pub async fn end_break_glass_session(
    pool: &DbPool,
    session_id: Uuid,
    org_id: impl Into<Uuid>,
) -> Result<u64, sqlx::Error> {
    let org_id: Uuid = org_id.into();
    let result = sqlx::query(&format!(
        "UPDATE break_glass_sessions SET ended_at = {} \
         WHERE id = $1 AND organization_id = $2 AND ended_at IS NULL",
        crate::db::sql::now()
    ))
    .bind(session_id)
    .bind(crate::db::bind_id(org_id))
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
