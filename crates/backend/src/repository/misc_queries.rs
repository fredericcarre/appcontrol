//! Query functions for misc domain (links, variables, groups, etc).

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid, DbJson};
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
