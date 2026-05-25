//! Git remote synchronisation for application maps.
//!
//! GitOps story (methodology §3.5 / §4.5): each application's map is
//! the source of truth, versioned like code. This module pushes the
//! current map JSON to a configured Git remote so the team can review,
//! revert, or diff via standard Git tooling.
//!
//! Initial implementation targets the **GitHub Contents API** (works
//! against github.com and GitHub Enterprise via `base_url`). The
//! provider abstraction is in place so GitLab and self-hosted Gitea
//! can be added without changing callers — `provider="gitlab"` simply
//! routes to a different impl.
//!
//! Credentials: we never store the actual token in the DB. The
//! `git_remotes` row carries a `token_env_var` field naming an
//! environment variable; the backend reads the token at push time.
//! This keeps secrets out of backups and audit dumps and lets
//! operators rotate via standard envvar deploys.

use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("token environment variable {0} is not set")]
    TokenNotSet(String),
    #[error("unsupported provider: {0}")]
    UnsupportedProvider(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("provider rejected the push: {status} — {body}")]
    Provider { status: u16, body: String },
}

#[derive(Debug, Clone)]
pub struct GitRemoteConfig {
    pub provider: String,
    pub base_url: String,
    pub repo: String,
    pub branch: String,
    pub token_env_var: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushResult {
    pub commit_sha: String,
    pub content_sha: String,
    pub path: String,
    pub branch: String,
}

/// Push a single file to the configured remote. The file content
/// is base64-encoded as required by the GitHub Contents API; other
/// providers may need a different shape, hence the provider switch.
pub async fn push_file(
    config: &GitRemoteConfig,
    path: &str,
    content: &[u8],
    commit_message: &str,
) -> Result<PushResult, GitError> {
    let token = std::env::var(&config.token_env_var)
        .map_err(|_| GitError::TokenNotSet(config.token_env_var.clone()))?;

    match config.provider.as_str() {
        "github" => push_via_github(config, &token, path, content, commit_message).await,
        other => Err(GitError::UnsupportedProvider(other.to_string())),
    }
}

async fn push_via_github(
    config: &GitRemoteConfig,
    token: &str,
    path: &str,
    content: &[u8],
    commit_message: &str,
) -> Result<PushResult, GitError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| GitError::Http(e.to_string()))?;

    let api_url = format!(
        "{}/repos/{}/contents/{}",
        config.base_url.trim_end_matches('/'),
        config.repo,
        path
    );

    // GitHub requires the file's current SHA when updating. Probe first.
    let existing_sha = fetch_existing_sha(&client, token, &api_url, &config.branch).await?;

    let body = serde_json::json!({
        "message": commit_message,
        "content": base64::engine::general_purpose::STANDARD.encode(content),
        "branch": config.branch,
        "sha": existing_sha,
    });

    let resp = client
        .put(&api_url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "appcontrol-backend")
        .json(&body)
        .send()
        .await
        .map_err(|e| GitError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(GitError::Provider {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: serde_json::Value = resp.json().await.map_err(|e| GitError::Http(e.to_string()))?;
    let commit_sha = parsed["commit"]["sha"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let content_sha = parsed["content"]["sha"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    Ok(PushResult {
        commit_sha,
        content_sha,
        path: path.to_string(),
        branch: config.branch.clone(),
    })
}

async fn fetch_existing_sha(
    client: &reqwest::Client,
    token: &str,
    api_url: &str,
    branch: &str,
) -> Result<Option<String>, GitError> {
    let resp = client
        .get(api_url)
        .query(&[("ref", branch)])
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "appcontrol-backend")
        .send()
        .await
        .map_err(|e| GitError::Http(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(GitError::Provider { status, body });
    }

    let parsed: serde_json::Value = resp.json().await.map_err(|e| GitError::Http(e.to_string()))?;
    Ok(parsed["sha"].as_str().map(|s| s.to_string()))
}

/// Substitute `{app_id}` and `{app_name}` placeholders in a path template.
pub fn render_path(template: &str, app_id: uuid::Uuid, app_name: &str) -> String {
    template
        .replace("{app_id}", &app_id.to_string())
        .replace("{app_name}", &slug(app_name))
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_path_substitutes_placeholders() {
        let id = uuid::uuid!("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            render_path("apps/{app_id}/map.json", id, "Billing Core"),
            format!("apps/{}/map.json", id)
        );
        assert_eq!(
            render_path("{app_name}/map.json", id, "Billing Core"),
            "billing-core/map.json"
        );
    }

    #[test]
    fn slug_lowercases_and_replaces_unsafe_chars() {
        assert_eq!(slug("Billing Core"), "billing-core");
        assert_eq!(slug("AppControl_v4!"), "appcontrol_v4");
        assert_eq!(slug("---multi---hyphen---"), "multi---hyphen");
    }
}
