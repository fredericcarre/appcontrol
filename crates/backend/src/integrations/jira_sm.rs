//! Jira Service Management incident pull connector.
//!
//! Pulls issues matching a JQL query from the Jira REST API
//! (`/rest/api/3/search`) and feeds them into the standard
//! `itsm::ingest` pipeline.
//!
//! Configuration (per call):
//!   * `instance_url` — e.g. `https://acme.atlassian.net`
//!   * `auth_token_env_var` — name of the env var holding
//!     `user@example.com:api_token` base64-encoded (Atlassian REST auth)
//!   * `jql` — JQL query, e.g. `project = INC AND priority in (Highest, High) AND created >= -7d`
//!   * `limit` — max issues (default 100)

use serde::Deserialize;

use crate::error::ApiError;
use crate::integrations::itsm;

#[derive(Debug, Deserialize)]
pub struct JiraPullRequest {
    pub instance_url: String,
    pub auth_token_env_var: String,
    pub jql: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    #[serde(default)]
    issues: Vec<JiraIssue>,
}

#[derive(Debug, Deserialize)]
struct JiraIssue {
    key: String,
    fields: JiraFields,
}

#[derive(Debug, Deserialize)]
struct JiraFields {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<serde_json::Value>,
    #[serde(default)]
    priority: Option<JiraNamedField>,
    #[serde(default)]
    status: Option<JiraNamedField>,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    resolutiondate: Option<String>,
    #[serde(default)]
    components: Vec<JiraNamedField>,
}

#[derive(Debug, Deserialize)]
struct JiraNamedField {
    name: String,
}

pub async fn pull(req: &JiraPullRequest) -> Result<Vec<itsm::ItsmIncident>, ApiError> {
    let token = std::env::var(&req.auth_token_env_var).map_err(|_| {
        ApiError::Validation(format!(
            "env var {} is not set",
            req.auth_token_env_var
        ))
    })?;

    let url = format!(
        "{}/rest/api/3/search",
        req.instance_url.trim_end_matches('/')
    );
    let limit = req.limit.unwrap_or(100).min(1000);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let resp = client
        .get(&url)
        .query(&[
            ("jql", req.jql.as_str()),
            ("maxResults", &limit.to_string()),
            (
                "fields",
                "summary,description,priority,status,created,resolutiondate,components",
            ),
        ])
        .header("Accept", "application/json")
        .header("Authorization", format!("Basic {}", token))
        .header("User-Agent", "appcontrol-backend")
        .send()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!(
            "Jira returned HTTP {}: {}",
            status, body
        )));
    }

    let parsed: JiraSearchResponse = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("invalid Jira JSON: {}", e)))?;

    Ok(parsed.issues.into_iter().map(to_incident).collect())
}

fn parse_jira_dt(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

fn doc_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(_) => value
            .get("content")
            .and_then(|c| c.as_array())
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| n.get("content").and_then(|c| c.as_array()))
                    .flat_map(|arr| arr.iter().filter_map(|x| x.get("text").and_then(|t| t.as_str())))
                    .collect::<Vec<&str>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn to_incident(j: JiraIssue) -> itsm::ItsmIncident {
    let opened_at = j
        .fields
        .created
        .as_deref()
        .map(parse_jira_dt)
        .unwrap_or_else(chrono::Utc::now);
    let resolved_at = j.fields.resolutiondate.as_deref().map(parse_jira_dt);

    itsm::ItsmIncident {
        external_id: j.key,
        title: j.fields.summary.unwrap_or_else(|| "(no title)".to_string()),
        description: j.fields.description.as_ref().map(doc_to_string).filter(|s| !s.is_empty()),
        severity: j.fields.priority.map(|p| p.name),
        status: j.fields.status.map(|s| s.name),
        opened_at,
        resolved_at,
        root_cause: None,
        impacted_component_names: j.fields.components.into_iter().map(|c| c.name).collect(),
        metadata: serde_json::json!({"source_system": "jira-sm"}),
    }
}
