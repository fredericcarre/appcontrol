//! Git remote sync API.
//!
//! Routes (wired in api/mod.rs):
//!
//!   GET    /api/v1/git/remotes                     list org remotes
//!   POST   /api/v1/git/remotes                     create a remote (admin)
//!   PUT    /api/v1/git/remotes/:id                 update (admin)
//!   DELETE /api/v1/git/remotes/:id                 delete (admin)
//!   GET    /api/v1/apps/:id/git                    fetch app sync settings
//!   PUT    /api/v1/apps/:id/git                    set/clear app sync settings
//!   POST   /api/v1/apps/:id/git/push               push current map (manual)

use axum::{
    extract::{Extension, Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use appcontrol_common::PermissionLevel;

use crate::auth::AuthUser;
use crate::core::permissions::effective_permission;
use crate::db::{DbPool, DbUuid};
use crate::error::ApiError;
use crate::integrations::git::{self, GitRemoteConfig};
use crate::middleware::audit::{complete_action_failed, complete_action_success, log_action};
use crate::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GitRemoteRow {
    pub id: DbUuid,
    pub organization_id: DbUuid,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub repo: String,
    pub branch: String,
    pub token_env_var: String,
    pub default_path_template: String,
    pub is_enabled: bool,
    pub last_push_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_push_sha: Option<String>,
    pub last_push_status: Option<String>,
    pub last_push_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRemoteRequest {
    pub name: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub repo: String,
    pub branch: Option<String>,
    pub token_env_var: String,
    pub default_path_template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRemoteRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub token_env_var: Option<String>,
    pub default_path_template: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SetAppGitRequest {
    pub git_remote_id: Option<Uuid>,
    pub path_override: Option<String>,
    pub auto_push_on_change: Option<bool>,
}

fn require_admin(user: &AuthUser) -> Result<(), ApiError> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// GET /api/v1/git/remotes
pub async fn list_remotes(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, ApiError> {
    let org_id: Uuid = *user.organization_id;

    #[cfg(feature = "postgres")]
    let rows: Vec<GitRemoteRow> = sqlx::query_as(
        "SELECT id, organization_id, name, provider, base_url, repo, branch, \
         token_env_var, default_path_template, is_enabled, \
         last_push_at, last_push_sha, last_push_status, last_push_error, \
         created_at, updated_at \
         FROM git_remotes WHERE organization_id = $1 ORDER BY name",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let rows: Vec<GitRemoteRow> = sqlx::query_as(
        "SELECT id, organization_id, name, provider, base_url, repo, branch, \
         token_env_var, default_path_template, is_enabled, \
         last_push_at, last_push_sha, last_push_status, last_push_error, \
         created_at, updated_at \
         FROM git_remotes WHERE organization_id = ? ORDER BY name",
    )
    .bind(DbUuid::from(org_id))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "remotes": rows })))
}

/// POST /api/v1/git/remotes
pub async fn create_remote(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<CreateRemoteRequest>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&user)?;

    let id = Uuid::new_v4();
    let org_id: Uuid = *user.organization_id;
    let provider = body.provider.trim().to_lowercase();
    if !["github", "gitlab", "gitea", "shell"].contains(&provider.as_str()) {
        return Err(ApiError::Validation(format!(
            "unsupported provider: {}",
            provider
        )));
    }
    let base_url = body
        .base_url
        .unwrap_or_else(|| match provider.as_str() {
            "github" => "https://api.github.com".to_string(),
            "gitlab" => "https://gitlab.com".to_string(),
            _ => "".to_string(),
        });
    let branch = body.branch.unwrap_or_else(|| "main".to_string());
    let path_template = body
        .default_path_template
        .unwrap_or_else(|| "apps/{app_id}/map.json".to_string());

    let action_id = log_action(
        &state.db,
        user.user_id,
        "git.remote.create",
        "git_remote",
        id,
        json!({"name": body.name, "provider": provider, "repo": body.repo}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO git_remotes
            (id, organization_id, name, provider, base_url, repo, branch,
             token_env_var, default_path_template)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(org_id)
    .bind(&body.name)
    .bind(&provider)
    .bind(&base_url)
    .bind(&body.repo)
    .bind(&branch)
    .bind(&body.token_env_var)
    .bind(&path_template)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO git_remotes
            (id, organization_id, name, provider, base_url, repo, branch,
             token_env_var, default_path_template)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(DbUuid::from(id))
    .bind(DbUuid::from(org_id))
    .bind(&body.name)
    .bind(&provider)
    .bind(&base_url)
    .bind(&body.repo)
    .bind(&branch)
    .bind(&body.token_env_var)
    .bind(&path_template)
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "created", "id": id })))
}

/// PUT /api/v1/git/remotes/:id
pub async fn update_remote(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRemoteRequest>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&user)?;
    let _ = fetch_remote(&state.db, *user.organization_id, id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "git.remote.update",
        "git_remote",
        id,
        json!({"id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE git_remotes SET
            name = COALESCE($1, name),
            provider = COALESCE($2, provider),
            base_url = COALESCE($3, base_url),
            repo = COALESCE($4, repo),
            branch = COALESCE($5, branch),
            token_env_var = COALESCE($6, token_env_var),
            default_path_template = COALESCE($7, default_path_template),
            is_enabled = COALESCE($8, is_enabled),
            updated_at = NOW()
          WHERE id = $9",
    )
    .bind(&body.name)
    .bind(&body.provider)
    .bind(&body.base_url)
    .bind(&body.repo)
    .bind(&body.branch)
    .bind(&body.token_env_var)
    .bind(&body.default_path_template)
    .bind(body.is_enabled)
    .bind(id)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE git_remotes SET
            name = COALESCE(?, name),
            provider = COALESCE(?, provider),
            base_url = COALESCE(?, base_url),
            repo = COALESCE(?, repo),
            branch = COALESCE(?, branch),
            token_env_var = COALESCE(?, token_env_var),
            default_path_template = COALESCE(?, default_path_template),
            is_enabled = COALESCE(?, is_enabled),
            updated_at = CURRENT_TIMESTAMP
          WHERE id = ?",
    )
    .bind(&body.name)
    .bind(&body.provider)
    .bind(&body.base_url)
    .bind(&body.repo)
    .bind(&body.branch)
    .bind(&body.token_env_var)
    .bind(&body.default_path_template)
    .bind(body.is_enabled)
    .bind(DbUuid::from(id))
    .execute(&state.db)
    .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "updated", "id": id })))
}

/// DELETE /api/v1/git/remotes/:id
pub async fn delete_remote(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&user)?;
    let _ = fetch_remote(&state.db, *user.organization_id, id).await?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "git.remote.delete",
        "git_remote",
        id,
        json!({"id": id}),
    )
    .await?;

    #[cfg(feature = "postgres")]
    sqlx::query("DELETE FROM git_remotes WHERE id = $1").bind(id).execute(&state.db).await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query("DELETE FROM git_remotes WHERE id = ?")
        .bind(DbUuid::from(id))
        .execute(&state.db)
        .await?;

    let _ = complete_action_success(&state.db, action_id).await;
    Ok(Json(json!({ "status": "deleted", "id": id })))
}

/// GET /api/v1/apps/:id/git
pub async fn get_app_git(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::View {
        return Err(ApiError::Forbidden);
    }

    type AppGitSettingsRow = (
        DbUuid,
        DbUuid,
        Option<String>,
        bool,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    );

    #[cfg(feature = "postgres")]
    let row: Option<AppGitSettingsRow> = sqlx::query_as(
        "SELECT application_id, git_remote_id, path_override, auto_push_on_change, \
         last_push_at, last_push_sha \
         FROM application_git_settings WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_optional(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<AppGitSettingsRow> = sqlx::query_as(
        "SELECT application_id, git_remote_id, path_override, auto_push_on_change, \
         last_push_at, last_push_sha \
         FROM application_git_settings WHERE application_id = ?",
    )
    .bind(DbUuid::from(app_id))
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((_, remote_id, path, auto, last_at, last_sha)) => Ok(Json(json!({
            "application_id": app_id,
            "git_remote_id": remote_id,
            "path_override": path,
            "auto_push_on_change": auto,
            "last_push_at": last_at,
            "last_push_sha": last_sha,
        }))),
        None => Ok(Json(json!({ "application_id": app_id, "git_remote_id": null }))),
    }
}

/// PUT /api/v1/apps/:id/git
pub async fn set_app_git(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
    Json(body): Json<SetAppGitRequest>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // If clearing the remote binding, delete the row.
    if body.git_remote_id.is_none() {
        #[cfg(feature = "postgres")]
        sqlx::query("DELETE FROM application_git_settings WHERE application_id = $1")
            .bind(app_id)
            .execute(&state.db)
            .await?;
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        sqlx::query("DELETE FROM application_git_settings WHERE application_id = ?")
            .bind(DbUuid::from(app_id))
            .execute(&state.db)
            .await?;
        return Ok(Json(json!({ "status": "cleared" })));
    }

    let remote_id = body.git_remote_id.unwrap();
    fetch_remote(&state.db, *user.organization_id, remote_id).await?;

    let auto = body.auto_push_on_change.unwrap_or(false);

    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO application_git_settings
            (application_id, git_remote_id, path_override, auto_push_on_change)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (application_id) DO UPDATE
            SET git_remote_id = EXCLUDED.git_remote_id,
                path_override = EXCLUDED.path_override,
                auto_push_on_change = EXCLUDED.auto_push_on_change,
                updated_at = NOW()",
    )
    .bind(app_id)
    .bind(remote_id)
    .bind(&body.path_override)
    .bind(auto)
    .execute(&state.db)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "INSERT INTO application_git_settings
            (application_id, git_remote_id, path_override, auto_push_on_change)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(application_id) DO UPDATE SET
            git_remote_id = excluded.git_remote_id,
            path_override = excluded.path_override,
            auto_push_on_change = excluded.auto_push_on_change,
            updated_at = CURRENT_TIMESTAMP",
    )
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(remote_id))
    .bind(&body.path_override)
    .bind(auto)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "status": "set", "git_remote_id": remote_id })))
}

/// POST /api/v1/apps/:id/git/push
pub async fn push_app(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(app_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let perm = effective_permission(&state.db, user.user_id, app_id, user.is_admin()).await;
    if perm < PermissionLevel::Manage {
        return Err(ApiError::Forbidden);
    }

    // Resolve the binding + remote.
    let (remote_config, path) =
        resolve_app_target(&state.db, *user.organization_id, app_id).await?;

    // Build the map JSON we're going to push.
    let map_json = build_app_map_json(&state, app_id).await?;
    let pretty = serde_json::to_vec_pretty(&map_json)
        .map_err(|e| ApiError::Internal(format!("serialise map: {}", e)))?;

    let action_id = log_action(
        &state.db,
        user.user_id,
        "git.push",
        "application",
        app_id,
        json!({"path": path, "branch": remote_config.branch, "provider": remote_config.provider}),
    )
    .await?;

    let commit_message = format!(
        "appcontrol: sync map for application {} ({})",
        app_id,
        chrono::Utc::now().to_rfc3339()
    );
    match git::push_file(&remote_config, &path, &pretty, &commit_message).await {
        Ok(result) => {
            record_push_success(&state.db, app_id, &result.commit_sha).await?;
            let _ = complete_action_success(&state.db, action_id).await;
            Ok(Json(json!({ "status": "pushed", "result": result })))
        }
        Err(e) => {
            record_push_failure(&state.db, app_id, &e.to_string()).await?;
            let _ = complete_action_failed(&state.db, action_id, &e.to_string()).await;
            Err(ApiError::Internal(format!("git push failed: {}", e)))
        }
    }
}

type AppGitRow = (
    DbUuid,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

async fn resolve_app_target(
    pool: &DbPool,
    org_id: Uuid,
    app_id: Uuid,
) -> Result<(GitRemoteConfig, String), ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<AppGitRow> = sqlx::query_as(
        "SELECT s.git_remote_id, s.path_override, \
                r.name, r.provider, r.base_url, r.repo, r.branch, r.token_env_var, \
                r.default_path_template, COALESCE(a.name, '') \
         FROM application_git_settings s \
         INNER JOIN git_remotes r ON r.id = s.git_remote_id \
         LEFT JOIN applications a ON a.id = s.application_id \
         WHERE s.application_id = $1 AND r.organization_id = $2 AND r.is_enabled = TRUE",
    )
    .bind(app_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<AppGitRow> = sqlx::query_as(
        "SELECT s.git_remote_id, s.path_override, \
                r.name, r.provider, r.base_url, r.repo, r.branch, r.token_env_var, \
                r.default_path_template, COALESCE(a.name, '') \
         FROM application_git_settings s \
         INNER JOIN git_remotes r ON r.id = s.git_remote_id \
         LEFT JOIN applications a ON a.id = s.application_id \
         WHERE s.application_id = ? AND r.organization_id = ? AND r.is_enabled = 1",
    )
    .bind(DbUuid::from(app_id))
    .bind(DbUuid::from(org_id))
    .fetch_optional(pool)
    .await?;

    let (_, path_override, _name, provider, base_url, repo, branch, token_env_var, default_path_template, app_name) =
        row.ok_or_else(|| {
            ApiError::Validation(
                "application has no Git remote configured (or remote is disabled)".to_string(),
            )
        })?;

    let template = path_override.unwrap_or(default_path_template);
    let path = git::render_path(&template, app_id, &app_name);

    Ok((
        GitRemoteConfig {
            provider,
            base_url,
            repo,
            branch,
            token_env_var,
        },
        path,
    ))
}

async fn build_app_map_json(
    state: &Arc<AppState>,
    app_id: Uuid,
) -> Result<Value, ApiError> {
    let app_name = state
        .app_repo
        .get_app_name(app_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let components = state.app_repo.get_components_with_agents(app_id).await?;
    let dependencies = state.app_repo.get_app_dependencies(app_id).await?;

    // Project components into a Git-friendly JSON shape. We deliberately
    // do NOT dump the full struct (state, position, agent metadata) since
    // the goal is a stable, reviewable map definition — not a runtime
    // snapshot. Runtime data is queryable via the regular API.
    let components_json: Vec<Value> = components
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "display_name": c.display_name,
                "description": c.description,
                "component_type": c.component_type,
                "host": c.host,
                "agent_id": c.agent_id,
                "check_cmd": c.check_cmd,
                "start_cmd": c.start_cmd,
                "stop_cmd": c.stop_cmd,
                "check_interval_seconds": c.check_interval_seconds,
                "start_timeout_seconds": c.start_timeout_seconds,
                "stop_timeout_seconds": c.stop_timeout_seconds,
                "is_optional": c.is_optional,
                "cluster_size": c.cluster_size,
                "cluster_mode": c.cluster_mode,
                "referenced_app_id": c.referenced_app_id,
            })
        })
        .collect();
    let dependencies_json: Vec<Value> = dependencies
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "from_component_id": d.from_component_id,
                "to_component_id": d.to_component_id,
            })
        })
        .collect();

    Ok(json!({
        "schema_version": 1,
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "application": {
            "id": app_id,
            "name": app_name,
        },
        "components": components_json,
        "dependencies": dependencies_json,
    }))
}

async fn record_push_success(
    pool: &DbPool,
    app_id: Uuid,
    commit_sha: &str,
) -> Result<(), ApiError> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE application_git_settings SET last_push_at = NOW(), last_push_sha = $1 WHERE application_id = $2",
    )
    .bind(commit_sha)
    .bind(app_id)
    .execute(pool)
    .await?;
    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE git_remotes SET last_push_at = NOW(), last_push_sha = $1, last_push_status = 'ok', last_push_error = NULL \
         WHERE id = (SELECT git_remote_id FROM application_git_settings WHERE application_id = $2)",
    )
    .bind(commit_sha)
    .bind(app_id)
    .execute(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    {
        sqlx::query(
            "UPDATE application_git_settings SET last_push_at = CURRENT_TIMESTAMP, last_push_sha = ? WHERE application_id = ?",
        )
        .bind(commit_sha)
        .bind(DbUuid::from(app_id))
        .execute(pool)
        .await?;
        sqlx::query(
            "UPDATE git_remotes SET last_push_at = CURRENT_TIMESTAMP, last_push_sha = ?, last_push_status = 'ok', last_push_error = NULL \
             WHERE id = (SELECT git_remote_id FROM application_git_settings WHERE application_id = ?)",
        )
        .bind(commit_sha)
        .bind(DbUuid::from(app_id))
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn record_push_failure(
    pool: &DbPool,
    app_id: Uuid,
    err: &str,
) -> Result<(), ApiError> {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE git_remotes SET last_push_at = NOW(), last_push_status = 'error', last_push_error = $1 \
         WHERE id = (SELECT git_remote_id FROM application_git_settings WHERE application_id = $2)",
    )
    .bind(err)
    .bind(app_id)
    .execute(pool)
    .await?;
    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    sqlx::query(
        "UPDATE git_remotes SET last_push_at = CURRENT_TIMESTAMP, last_push_status = 'error', last_push_error = ? \
         WHERE id = (SELECT git_remote_id FROM application_git_settings WHERE application_id = ?)",
    )
    .bind(err)
    .bind(DbUuid::from(app_id))
    .execute(pool)
    .await?;
    Ok(())
}

async fn fetch_remote(
    pool: &DbPool,
    org_id: Uuid,
    id: Uuid,
) -> Result<GitRemoteRow, ApiError> {
    #[cfg(feature = "postgres")]
    let row: Option<GitRemoteRow> = sqlx::query_as(
        "SELECT id, organization_id, name, provider, base_url, repo, branch, \
         token_env_var, default_path_template, is_enabled, \
         last_push_at, last_push_sha, last_push_status, last_push_error, \
         created_at, updated_at \
         FROM git_remotes WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    let row: Option<GitRemoteRow> = sqlx::query_as(
        "SELECT id, organization_id, name, provider, base_url, repo, branch, \
         token_env_var, default_path_template, is_enabled, \
         last_push_at, last_push_sha, last_push_status, last_push_error, \
         created_at, updated_at \
         FROM git_remotes WHERE id = ? AND organization_id = ?",
    )
    .bind(DbUuid::from(id))
    .bind(DbUuid::from(org_id))
    .fetch_optional(pool)
    .await?;

    row.ok_or(ApiError::NotFound)
}
