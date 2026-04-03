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

// ============================================================================
// History (timeline) queries
// ============================================================================

/// Component id+name pair for history.
#[derive(Debug, Clone)]
pub struct HistoryComponentRow {
    pub id: Uuid,
    pub name: String,
}

/// State transition row for history timeline.
#[derive(Debug, Clone)]
pub struct HistoryTransitionRow {
    pub component_id: Uuid,
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Fetch components for an app (id + name only).
pub async fn history_list_components(
    pool: &DbPool,
    app_id: Uuid,
) -> Result<Vec<HistoryComponentRow>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        #[derive(sqlx::FromRow)]
        struct Row { id: Uuid, name: String }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
        )
        .bind(app_id)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|r| HistoryComponentRow { id: r.id, name: r.name }).collect())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        #[derive(sqlx::FromRow)]
        struct Row { id: DbUuid, name: String }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, name FROM components WHERE application_id = $1 ORDER BY name",
        )
        .bind(DbUuid::from(app_id))
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|r| HistoryComponentRow { id: r.id.into_inner(), name: r.name }).collect())
    }
}

/// Get initial state of components at a given time.
pub async fn history_initial_states(
    pool: &DbPool,
    component_ids: &[Uuid],
    at: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT c.id, COALESCE(
                (SELECT st.to_state
                 FROM state_transitions st
                 WHERE st.component_id = c.id AND st.created_at < $2
                 ORDER BY st.created_at DESC
                 LIMIT 1),
                c.current_state
            ) as state
            FROM components c
            WHERE c.id = ANY($1)
            "#,
        )
        .bind(component_ids)
        .bind(at)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let placeholders: Vec<String> = (2..=1 + component_ids.len())
            .map(|i| format!("${}", i))
            .collect();
        let query = format!(
            r#"
            SELECT c.id, COALESCE(
                (SELECT st.to_state
                 FROM state_transitions st
                 WHERE st.component_id = c.id AND st.created_at < $1
                 ORDER BY st.created_at DESC
                 LIMIT 1),
                c.current_state
            ) as state
            FROM components c
            WHERE c.id IN ({})
            "#,
            placeholders.join(", ")
        );
        let mut q = sqlx::query_as::<_, (String, String)>(&query).bind(at.to_rfc3339());
        for id in component_ids {
            q = q.bind(id.to_string());
        }
        let rows: Vec<(String, String)> = q.fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id_str, state)| {
                Uuid::parse_str(&id_str).ok().map(|id| (id, state))
            })
            .collect())
    }
}

/// Get state transition rows for components in a time range.
pub async fn history_transition_rows(
    pool: &DbPool,
    component_ids: &[Uuid],
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<HistoryTransitionRow>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "postgres")]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
            component_id: Uuid,
            from_state: String,
            to_state: String,
            trigger: String,
            created_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT component_id, from_state, to_state, trigger, created_at
            FROM state_transitions
            WHERE component_id = ANY($1) AND created_at >= $2 AND created_at <= $3
            ORDER BY created_at ASC
            "#,
        )
        .bind(component_ids)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|r| HistoryTransitionRow {
            component_id: r.component_id,
            from_state: r.from_state,
            to_state: r.to_state,
            trigger: r.trigger,
            created_at: r.created_at,
        }).collect())
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let placeholders: Vec<String> = (3..=2 + component_ids.len())
            .map(|i| format!("${}", i))
            .collect();
        let query = format!(
            r#"
            SELECT component_id, from_state, to_state, trigger, created_at
            FROM state_transitions
            WHERE component_id IN ({}) AND created_at >= $1 AND created_at <= $2
            ORDER BY created_at ASC
            "#,
            placeholders.join(", ")
        );
        #[derive(sqlx::FromRow)]
        struct SqliteRow {
            component_id: String,
            from_state: String,
            to_state: String,
            trigger: String,
            created_at: String,
        }
        let mut q = sqlx::query_as::<_, SqliteRow>(&query)
            .bind(from.to_rfc3339())
            .bind(to.to_rfc3339());
        for id in component_ids {
            q = q.bind(id.to_string());
        }
        let rows = q.fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let id = Uuid::parse_str(&r.component_id).ok()?;
                let at = chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc);
                Some(HistoryTransitionRow {
                    component_id: id,
                    from_state: r.from_state,
                    to_state: r.to_state,
                    trigger: r.trigger,
                    created_at: at,
                })
            })
            .collect())
    }
}

/// Fetch state transitions with limit for event building.
pub async fn history_state_transitions(
    pool: &DbPool,
    component_ids: &[Uuid],
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Vec<(Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<_, (Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
            r#"
            SELECT component_id, from_state, to_state, trigger, created_at
            FROM state_transitions
            WHERE component_id = ANY($1) AND created_at >= $2 AND created_at <= $3
            ORDER BY created_at ASC
            LIMIT $4
            "#,
        )
        .bind(component_ids)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let placeholders: Vec<String> = (4..=3 + component_ids.len())
            .map(|i| format!("${}", i))
            .collect();
        let query = format!(
            r#"
            SELECT component_id, from_state, to_state, trigger, created_at
            FROM state_transitions
            WHERE component_id IN ({}) AND created_at >= $1 AND created_at <= $2
            ORDER BY created_at ASC
            LIMIT $3
            "#,
            placeholders.join(", ")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String, String)>(&query)
            .bind(from.to_rfc3339())
            .bind(to.to_rfc3339())
            .bind(limit);
        for id in component_ids {
            q = q.bind(id.to_string());
        }
        let rows: Vec<(String, String, String, String, String)> = q.fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(comp_id, from_state, to_state, trigger, created_at)| {
                let id = Uuid::parse_str(&comp_id).ok()?;
                let at = chrono::DateTime::parse_from_rfc3339(&created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc);
                Some((id, from_state, to_state, trigger, at))
            })
            .collect())
    }
}

/// Fetch app-level actions for history.
pub async fn history_app_actions(
    pool: &DbPool,
    app_id: Uuid,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Vec<(String, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<_, (String, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>(
            r#"
            SELECT COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action, al.details, al.created_at,
                   al.status, al.error_message
            FROM action_log al
            LEFT JOIN users u ON u.id = al.user_id
            WHERE al.resource_id = $1 AND al.created_at >= $2 AND al.created_at <= $3
            ORDER BY al.created_at ASC
            LIMIT $4
            "#,
        )
        .bind(app_id)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        #[derive(sqlx::FromRow)]
        struct Row {
            user_email: String,
            action: String,
            details: String,
            created_at: String,
            status: Option<String>,
            error_message: Option<String>,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT COALESCE(u.email, CAST(al.user_id AS TEXT)) as user_email, al.action,
                   al.details, al.created_at, al.status, al.error_message
            FROM action_log al
            LEFT JOIN users u ON u.id = al.user_id
            WHERE al.resource_id = $1 AND al.created_at >= $2 AND al.created_at <= $3
            ORDER BY al.created_at ASC
            LIMIT $4
            "#,
        )
        .bind(DbUuid::from(app_id))
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339())
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let at = chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc);
                let details: Value = serde_json::from_str(&r.details).unwrap_or(Value::Null);
                Some((r.user_email, r.action, details, at, r.status, r.error_message))
            })
            .collect())
    }
}

/// Fetch component-level actions for history.
pub async fn history_component_actions(
    pool: &DbPool,
    component_ids: &[Uuid],
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Vec<(String, String, Uuid, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<_, (String, String, Uuid, String, Value, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>(
            r#"
            SELECT COALESCE(u.email, al.user_id::text), al.action, al.resource_id,
                   COALESCE(c.name, al.resource_id::text), al.details, al.created_at,
                   al.status, al.error_message
            FROM action_log al
            LEFT JOIN users u ON u.id = al.user_id
            LEFT JOIN components c ON c.id = al.resource_id
            WHERE al.resource_type = 'component'
              AND al.resource_id = ANY($1)
              AND al.created_at >= $2 AND al.created_at <= $3
            ORDER BY al.created_at ASC
            LIMIT $4
            "#,
        )
        .bind(component_ids)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let placeholders: Vec<String> = (4..=3 + component_ids.len())
            .map(|i| format!("${}", i))
            .collect();
        let query = format!(
            r#"
            SELECT COALESCE(u.email, CAST(al.user_id AS TEXT)), al.action, al.resource_id,
                   COALESCE(c.name, CAST(al.resource_id AS TEXT)), al.details, al.created_at,
                   al.status, al.error_message
            FROM action_log al
            LEFT JOIN users u ON u.id = al.user_id
            LEFT JOIN components c ON c.id = al.resource_id
            WHERE al.resource_type = 'component'
              AND al.resource_id IN ({})
              AND al.created_at >= $1 AND al.created_at <= $2
            ORDER BY al.created_at ASC
            LIMIT $3
            "#,
            placeholders.join(", ")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String, String, String, Option<String>, Option<String>)>(&query)
            .bind(from.to_rfc3339())
            .bind(to.to_rfc3339())
            .bind(limit);
        for id in component_ids {
            q = q.bind(id.to_string());
        }
        let rows = q.fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(user, action, resource_id, comp_name, details, created_at, status, error)| {
                let id = Uuid::parse_str(&resource_id).ok()?;
                let at = chrono::DateTime::parse_from_rfc3339(&created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc);
                let details_val: Value = serde_json::from_str(&details).unwrap_or(Value::Null);
                Some((user, action, id, comp_name, details_val, at, status, error))
            })
            .collect())
    }
}

/// Fetch command execution events for history.
pub async fn history_command_executions(
    pool: &DbPool,
    component_ids: &[Uuid],
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Vec<(Uuid, Uuid, String, Option<i16>, Option<i32>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)>, sqlx::Error> {
    if component_ids.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "postgres")]
    {
        let rows = sqlx::query_as::<_, (Uuid, Uuid, String, Option<i16>, Option<i32>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)>(
            r#"
            SELECT ce.request_id, ce.component_id, ce.command_type,
                   ce.exit_code, ce.duration_ms, ce.dispatched_at, ce.completed_at
            FROM command_executions ce
            WHERE ce.component_id = ANY($1) AND ce.dispatched_at >= $2 AND ce.dispatched_at <= $3
            ORDER BY ce.dispatched_at ASC
            LIMIT $4
            "#,
        )
        .bind(component_ids)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        let placeholders: Vec<String> = (4..=3 + component_ids.len())
            .map(|i| format!("${}", i))
            .collect();
        let query = format!(
            r#"
            SELECT ce.request_id, ce.component_id, ce.command_type,
                   ce.exit_code, ce.duration_ms, ce.dispatched_at, ce.completed_at
            FROM command_executions ce
            WHERE ce.component_id IN ({}) AND ce.dispatched_at >= $1 AND ce.dispatched_at <= $2
            ORDER BY ce.dispatched_at ASC
            LIMIT $3
            "#,
            placeholders.join(", ")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, Option<i16>, Option<i32>, String, Option<String>)>(&query)
            .bind(from.to_rfc3339())
            .bind(to.to_rfc3339())
            .bind(limit);
        for id in component_ids {
            q = q.bind(id.to_string());
        }
        let rows = q.fetch_all(pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(request_id, comp_id, cmd_type, exit_code, duration_ms, dispatched_at, completed_at)| {
                let req_id = Uuid::parse_str(&request_id).ok()?;
                let cid = Uuid::parse_str(&comp_id).ok()?;
                let dispatched = chrono::DateTime::parse_from_rfc3339(&dispatched_at)
                    .ok()?
                    .with_timezone(&chrono::Utc);
                let completed = completed_at
                    .and_then(|c| chrono::DateTime::parse_from_rfc3339(&c).ok())
                    .map(|c| c.with_timezone(&chrono::Utc));
                Some((req_id, cid, cmd_type, exit_code, duration_ms, dispatched, completed))
            })
            .collect())
    }
}

// ============================================================================
// Approval queries (api/approvals.rs)
// ============================================================================

/// Row type for approval requests.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ApprovalRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub operation_type: String,
    pub resource_type: String,
    pub resource_id: DbUuid,
    pub risk_level: String,
    pub requested_by: DbUuid,
    pub request_payload: Value,
    pub status: String,
    pub required_approvals: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Check if an approval policy is enabled for an operation type.
pub async fn check_approval_policy(
    pool: &DbPool,
    organization_id: DbUuid,
    operation_type: &str,
) -> Result<Option<(bool,)>, sqlx::Error> {
    sqlx::query_as::<_, (bool,)>(
        "SELECT enabled FROM approval_policies WHERE organization_id = $1 AND operation_type = $2",
    )
    .bind(organization_id)
    .bind(operation_type)
    .fetch_optional(pool)
    .await
}

/// Insert a new approval request and return it.
pub async fn insert_approval_request(
    pool: &DbPool,
    request_id: Uuid,
    organization_id: DbUuid,
    operation_type: &str,
    resource_type: &str,
    resource_id: Uuid,
    risk_level: &str,
    requested_by: DbUuid,
    request_payload: &Value,
    required_approvals: i32,
    timeout_minutes: i32,
) -> Result<ApprovalRow, sqlx::Error> {
    sqlx::query_as::<_, ApprovalRow>(&format!(
        "INSERT INTO approval_requests (
                id, organization_id, operation_type, resource_type, resource_id,
                risk_level, requested_by, request_payload, required_approvals, expires_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, {} + make_interval(mins => $10))
            RETURNING id, organization_id, operation_type, resource_type, resource_id,
                      risk_level, requested_by, request_payload, status, required_approvals,
                      created_at, expires_at, resolved_at",
        db::sql::now()
    ))
    .bind(request_id)
    .bind(crate::db::bind_id(organization_id))
    .bind(operation_type)
    .bind(resource_type)
    .bind(resource_id)
    .bind(risk_level)
    .bind(crate::db::bind_id(requested_by))
    .bind(request_payload)
    .bind(required_approvals)
    .bind(timeout_minutes)
    .fetch_one(pool)
    .await
}

/// List approval requests for an organization.
pub async fn list_approval_requests(
    pool: &DbPool,
    organization_id: DbUuid,
) -> Result<Vec<ApprovalRow>, sqlx::Error> {
    sqlx::query_as::<_, ApprovalRow>(
        r#"
        SELECT id, organization_id, operation_type, resource_type, resource_id,
               risk_level, requested_by, request_payload, status, required_approvals,
               created_at, expires_at, resolved_at
        FROM approval_requests
        WHERE organization_id = $1
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .bind(crate::db::bind_id(organization_id))
    .fetch_all(pool)
    .await
}

/// Get a single approval request by id and org.
pub async fn get_approval_request(
    pool: &DbPool,
    request_id: Uuid,
    organization_id: DbUuid,
) -> Result<Option<ApprovalRow>, sqlx::Error> {
    sqlx::query_as::<_, ApprovalRow>(
        r#"
        SELECT id, organization_id, operation_type, resource_type, resource_id,
               risk_level, requested_by, request_payload, status, required_approvals,
               created_at, expires_at, resolved_at
        FROM approval_requests
        WHERE id = $1 AND organization_id = $2
        "#,
    )
    .bind(request_id)
    .bind(crate::db::bind_id(organization_id))
    .fetch_optional(pool)
    .await
}

/// Expire an approval request.
pub async fn expire_approval_request(
    pool: &DbPool,
    request_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE approval_requests SET status = 'expired', resolved_at = {} WHERE id = $1",
        db::sql::now()
    ))
    .bind(request_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert an approval decision.
pub async fn insert_approval_decision(
    pool: &DbPool,
    decision_id: Uuid,
    request_id: Uuid,
    decided_by: DbUuid,
    decision: &str,
    reason: &Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO approval_decisions (id, request_id, decided_by, decision, reason) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(decision_id)
    .bind(request_id)
    .bind(crate::db::bind_id(decided_by))
    .bind(decision)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

/// Count approvals for a request.
pub async fn count_approvals(
    pool: &DbPool,
    request_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM approval_decisions WHERE request_id = $1 AND decision = 'approved'",
    )
    .bind(request_id)
    .fetch_one(pool)
    .await
}

/// Update approval request status.
pub async fn update_approval_status(
    pool: &DbPool,
    request_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE approval_requests SET status = $2, resolved_at = {} WHERE id = $1",
        db::sql::now()
    ))
    .bind(request_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

/// List approval policies for an organization.
pub async fn list_approval_policies(
    pool: &DbPool,
    organization_id: DbUuid,
) -> Result<Vec<(DbUuid, String, String, i32, i32, bool)>, sqlx::Error> {
    sqlx::query_as::<_, (DbUuid, String, String, i32, i32, bool)>(
        "SELECT id, operation_type, risk_level, required_approvals, timeout_minutes, enabled \
         FROM approval_policies WHERE organization_id = $1 ORDER BY operation_type",
    )
    .bind(crate::db::bind_id(organization_id))
    .fetch_all(pool)
    .await
}

/// Upsert an approval policy.
pub async fn upsert_approval_policy(
    pool: &DbPool,
    organization_id: DbUuid,
    operation_type: &str,
    risk_level: &str,
    required_approvals: i32,
    timeout_minutes: i32,
    enabled: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO approval_policies (organization_id, operation_type, risk_level, required_approvals, timeout_minutes, enabled)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (organization_id, operation_type)
        DO UPDATE SET risk_level = $3, required_approvals = $4, timeout_minutes = $5, enabled = $6
        "#,
    )
    .bind(crate::db::bind_id(organization_id))
    .bind(operation_type)
    .bind(risk_level)
    .bind(required_approvals)
    .bind(timeout_minutes)
    .bind(enabled)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// PKI queries (api/pki_export.rs)
// ============================================================================

/// Get the first CA cert PEM for unauthenticated retrieval.
pub async fn get_first_ca_public(
    pool: &DbPool,
) -> Result<Option<(Option<String>, String)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT ca_cert_pem, slug FROM organizations
           WHERE ca_cert_pem IS NOT NULL
           ORDER BY created_at ASC
           LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
}

/// Get CA cert/key for an organization.
pub async fn get_org_ca_cert_key(
    pool: &DbPool,
    org_id: DbUuid,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT ca_cert_pem, ca_key_pem FROM organizations WHERE id = $1")
        .bind(crate::db::bind_id(org_id))
        .fetch_optional(pool)
        .await
}

/// Log a certificate event (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn log_certificate_event_with_days(
    pool: &DbPool,
    event_type: &str,
    fingerprint: &str,
    cn: &str,
    validity_days: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
             VALUES ($1, $2, $3, {now}, {now} + $4 * interval '1 day')",
        now = db::sql::now()
    ))
    .bind(event_type)
    .bind(fingerprint)
    .bind(cn)
    .bind(validity_days)
    .execute(pool)
    .await?;
    Ok(())
}

/// Log a certificate event (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn log_certificate_event_with_days(
    pool: &DbPool,
    event_type: &str,
    fingerprint: &str,
    cn: &str,
    _validity_days: i32,
) -> Result<(), sqlx::Error> {
    let expires_at =
        (chrono::Utc::now() + chrono::Duration::days(_validity_days as i64)).to_rfc3339();
    sqlx::query(&format!(
        "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
             VALUES ($1, $2, $3, {now}, $4)",
        now = db::sql::now()
    ))
    .bind(event_type)
    .bind(fingerprint)
    .bind(cn)
    .bind(&expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Log a certificate event with fixed interval (PostgreSQL).
#[cfg(feature = "postgres")]
pub async fn log_certificate_event_fixed_interval(
    pool: &DbPool,
    fingerprint: &str,
    cn: &str,
    interval_expr: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
             VALUES ('issued', $1, $2, {now}, {now} + {interval})",
        now = db::sql::now(),
        interval = interval_expr
    ))
    .bind(fingerprint)
    .bind(cn)
    .execute(pool)
    .await?;
    Ok(())
}

/// Log a certificate event with fixed interval (SQLite).
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub async fn log_certificate_event_fixed_interval(
    pool: &DbPool,
    fingerprint: &str,
    cn: &str,
    expires_at_str: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "INSERT INTO certificate_events (event_type, fingerprint, cn, issued_at, expires_at) \
             VALUES ('issued', $1, $2, {now}, $3)",
        now = db::sql::now()
    ))
    .bind(fingerprint)
    .bind(cn)
    .bind(expires_at_str)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get CA status with rotation info.
pub async fn get_org_ca_status(
    pool: &DbPool,
    org_id: DbUuid,
) -> Result<
    Option<(
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    )>,
    sqlx::Error,
> {
    sqlx::query_as(
        r#"SELECT ca_cert_pem, pending_ca_cert_pem, rotation_started_at
           FROM organizations WHERE id = $1"#,
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_optional(pool)
    .await
}

/// Count enrolled agents with certificates.
pub async fn count_enrolled_agents(
    pool: &DbPool,
    org_id: DbUuid,
) -> Result<(i64,), sqlx::Error> {
    sqlx::query_as(
        "SELECT COUNT(*) FROM agents WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL",
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_one(pool)
    .await
}

/// Count enrolled gateways with certificates.
pub async fn count_enrolled_gateways(
    pool: &DbPool,
    org_id: DbUuid,
) -> Result<(i64,), sqlx::Error> {
    sqlx::query_as(
        "SELECT COUNT(*) FROM gateways WHERE organization_id = $1 AND certificate_fingerprint IS NOT NULL",
    )
    .bind(crate::db::bind_id(org_id))
    .fetch_one(pool)
    .await
}

/// Get first org with CA for auto-export.
pub async fn get_first_org_with_ca(
    pool: &DbPool,
) -> Result<Option<(Uuid, Option<String>, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, ca_cert_pem, ca_key_pem FROM organizations WHERE ca_cert_pem IS NOT NULL LIMIT 1",
    )
    .fetch_optional(pool)
    .await
}

// ============================================================================
// Log source queries (api/logs.rs)
// ============================================================================

/// Row type for log sources.
#[derive(Debug, sqlx::FromRow)]
pub struct LogSourceRow {
    pub id: DbUuid,
    pub component_id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub source_type: String,
    pub description: Option<String>,
    pub file_path: Option<String>,
    pub event_log_name: Option<String>,
    pub event_log_source: Option<String>,
    pub event_log_level: Option<String>,
    pub command: Option<String>,
    pub command_timeout_seconds: i32,
    pub max_lines: i32,
    pub max_age_hours: i32,
    pub is_sensitive: bool,
    pub display_order: i32,
    pub created_by: Option<DbUuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Component row for permission checks in logs.
#[derive(Debug, sqlx::FromRow)]
pub struct LogComponentRow {
    pub id: DbUuid,
    pub application_id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub agent_id: Option<DbUuid>,
}

/// Get component by ID for permission checks.
pub async fn get_component_for_logs(
    pool: &DbPool,
    component_id: DbUuid,
) -> Result<Option<LogComponentRow>, sqlx::Error> {
    sqlx::query_as::<_, LogComponentRow>(
        "SELECT id, application_id, organization_id, name, agent_id FROM components WHERE id = $1",
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
}

/// List log sources for a component.
pub async fn list_log_sources(
    pool: &DbPool,
    component_id: Uuid,
) -> Result<Vec<LogSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, LogSourceRow>(
        r#"
        SELECT id, component_id, organization_id, name, source_type, description,
               file_path, event_log_name, event_log_source, event_log_level,
               command, command_timeout_seconds,
               max_lines, max_age_hours, is_sensitive, display_order, created_by, created_at, updated_at
        FROM component_log_sources
        WHERE component_id = $1
        ORDER BY display_order, name
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .fetch_all(pool)
    .await
}

/// Create a log source.
#[allow(clippy::too_many_arguments)]
pub async fn create_log_source(
    pool: &DbPool,
    id: Uuid,
    component_id: Uuid,
    organization_id: DbUuid,
    name: &str,
    source_type: &str,
    description: &Option<String>,
    file_path: &Option<String>,
    event_log_name: &Option<String>,
    event_log_source: &Option<String>,
    event_log_level: &Option<String>,
    command: &Option<String>,
    command_timeout_seconds: i32,
    max_lines: i32,
    max_age_hours: i32,
    is_sensitive: bool,
    display_order: i32,
    created_by: DbUuid,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO component_log_sources (
            id, component_id, organization_id, name, source_type, description,
            file_path, event_log_name, event_log_source, event_log_level,
            command, command_timeout_seconds,
            max_lines, max_age_hours, is_sensitive, display_order,
            created_by, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $18)
        "#,
    )
    .bind(crate::db::bind_id(id))
    .bind(crate::db::bind_id(component_id))
    .bind(organization_id)
    .bind(name)
    .bind(source_type)
    .bind(description)
    .bind(file_path)
    .bind(event_log_name)
    .bind(event_log_source)
    .bind(event_log_level)
    .bind(command)
    .bind(command_timeout_seconds)
    .bind(max_lines)
    .bind(max_age_hours)
    .bind(is_sensitive)
    .bind(display_order)
    .bind(crate::db::bind_id(created_by))
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a log source by ID.
pub async fn get_log_source_by_id(
    pool: &DbPool,
    source_id: Uuid,
) -> Result<Option<LogSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, LogSourceRow>(
        "SELECT id, component_id, organization_id, name, source_type, description, \
         file_path, event_log_name, event_log_source, event_log_level, \
         command, command_timeout_seconds, max_lines, max_age_hours, is_sensitive, display_order, \
         created_by, created_at, updated_at \
         FROM component_log_sources WHERE id = $1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
}

/// Update a log source.
#[allow(clippy::too_many_arguments)]
pub async fn update_log_source(
    pool: &DbPool,
    source_id: Uuid,
    name: &str,
    description: &Option<String>,
    file_path: &Option<String>,
    event_log_name: &Option<String>,
    event_log_source: &Option<String>,
    event_log_level: &Option<String>,
    command: &Option<String>,
    command_timeout_seconds: i32,
    max_lines: i32,
    max_age_hours: i32,
    is_sensitive: bool,
    display_order: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE component_log_sources SET
                name = $2, description = $3, file_path = $4,
                event_log_name = $5, event_log_source = $6, event_log_level = $7,
                command = $8, command_timeout_seconds = $9,
                max_lines = $10, max_age_hours = $11, is_sensitive = $12, display_order = $13,
                updated_at = {}
            WHERE id = $1",
        db::sql::now()
    ))
    .bind(source_id)
    .bind(name)
    .bind(description)
    .bind(file_path)
    .bind(event_log_name)
    .bind(event_log_source)
    .bind(event_log_level)
    .bind(command)
    .bind(command_timeout_seconds)
    .bind(max_lines)
    .bind(max_age_hours)
    .bind(is_sensitive)
    .bind(display_order)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a log source.
pub async fn delete_log_source(pool: &DbPool, source_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM component_log_sources WHERE id = $1")
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get a log source by ID and component_id.
pub async fn get_log_source_by_id_and_component(
    pool: &DbPool,
    source_id: Uuid,
    component_id: Uuid,
) -> Result<Option<LogSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, LogSourceRow>(
        "SELECT id, component_id, organization_id, name, source_type, description, \
         file_path, event_log_name, event_log_source, event_log_level, \
         command, command_timeout_seconds, max_lines, max_age_hours, is_sensitive, display_order, \
         created_by, created_at, updated_at \
         FROM component_log_sources WHERE id = $1 AND component_id = $2",
    )
    .bind(source_id)
    .bind(crate::db::bind_id(component_id))
    .fetch_optional(pool)
    .await
}

/// Get a log source by component_id, type, and name.
pub async fn get_log_source_by_component_type_name(
    pool: &DbPool,
    component_id: Uuid,
    name: &str,
) -> Result<Option<LogSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, LogSourceRow>(
        r#"
        SELECT id, component_id, organization_id, name, source_type, description,
               file_path, event_log_name, event_log_source, event_log_level,
               command, command_timeout_seconds, max_lines, max_age_hours, is_sensitive, display_order,
               created_by, created_at, updated_at
        FROM component_log_sources
        WHERE component_id = $1 AND source_type = 'command' AND name = $2
        "#,
    )
    .bind(crate::db::bind_id(component_id))
    .bind(name)
    .fetch_optional(pool)
    .await
}

/// Get organization_id for a component.
pub async fn get_component_org_id(
    pool: &DbPool,
    component_id: DbUuid,
) -> Result<DbUuid, sqlx::Error> {
    sqlx::query_scalar::<_, DbUuid>("SELECT organization_id FROM components WHERE id = $1")
        .bind(crate::db::bind_id(component_id))
        .fetch_one(pool)
        .await
}

/// Insert a log access audit record.
pub async fn insert_log_access_audit(
    pool: &DbPool,
    id: Uuid,
    organization_id: DbUuid,
    user_id: DbUuid,
    component_id: DbUuid,
    log_source_id: Option<DbUuid>,
    source_type: &str,
    source_name: &str,
    lines_requested: Option<i32>,
    filter_applied: &Option<String>,
    time_range_hours: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO log_access_audit (
            id, organization_id, user_id, component_id, log_source_id,
            source_type, source_name, lines_requested, filter_applied, time_range_hours
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(id)
    .bind(organization_id)
    .bind(crate::db::bind_id(user_id))
    .bind(crate::db::bind_id(component_id))
    .bind(log_source_id)
    .bind(source_type)
    .bind(source_name)
    .bind(lines_requested)
    .bind(filter_applied)
    .bind(time_range_hours)
    .execute(pool)
    .await?;
    Ok(())
}
