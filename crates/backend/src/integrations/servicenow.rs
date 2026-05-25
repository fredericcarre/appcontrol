//! ServiceNow incident pull connector.
//!
//! Pulls incidents from a ServiceNow instance via the Table API
//! (`/api/now/table/incident`) and feeds them into the standard
//! `itsm::ingest` pipeline.
//!
//! Configuration (per call):
//!   * `instance_url` — e.g. `https://acme.service-now.com`
//!   * `auth_token_env_var` — name of the env var holding a bearer or
//!     basic-auth token (`user:password` base64-encoded for basic)
//!   * `query` — ServiceNow encoded query, e.g. `priority<=2^opened_at>=javascript:gs.daysAgoStart(7)`
//!   * `limit` — max rows to pull (default 100)
//!
//! Identity columns:
//!   * `sys_id` → `external_id`
//!   * `short_description` → `title`
//!   * `priority` → `severity`
//!   * `state` → `status`
//!   * `opened_at` → `opened_at`
//!   * `closed_at`/`resolved_at` → `resolved_at`
//!   * `close_notes` or `cmdb_ci` resolution notes → `root_cause`
//!   * `cmdb_ci` (CI display name list) → `impacted_component_names`

use serde::Deserialize;

use crate::error::ApiError;
use crate::integrations::itsm;

#[derive(Debug, Deserialize)]
pub struct ServiceNowPullRequest {
    pub instance_url: String,
    pub auth_token_env_var: String,
    pub query: Option<String>,
    pub limit: Option<u32>,
    #[serde(default = "default_auth_scheme")]
    pub auth_scheme: String, // "bearer" | "basic"
}

fn default_auth_scheme() -> String {
    "basic".to_string()
}

#[derive(Debug, Deserialize)]
struct SnTableResponse {
    result: Vec<SnIncident>,
}

#[derive(Debug, Deserialize)]
struct SnIncident {
    sys_id: String,
    #[serde(default)]
    short_description: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    opened_at: Option<String>,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    resolved_at: Option<String>,
    #[serde(default)]
    close_notes: Option<String>,
    #[serde(default)]
    cmdb_ci: Option<serde_json::Value>,
}

pub async fn pull(
    req: &ServiceNowPullRequest,
) -> Result<Vec<itsm::ItsmIncident>, ApiError> {
    let token = std::env::var(&req.auth_token_env_var).map_err(|_| {
        ApiError::Validation(format!(
            "env var {} is not set",
            req.auth_token_env_var
        ))
    })?;

    let url = format!(
        "{}/api/now/table/incident",
        req.instance_url.trim_end_matches('/')
    );
    let limit = req.limit.unwrap_or(100).min(2000);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut request = client
        .get(&url)
        .query(&[
            ("sysparm_limit", limit.to_string()),
            (
                "sysparm_fields",
                "sys_id,short_description,description,priority,state,opened_at,closed_at,resolved_at,close_notes,cmdb_ci".to_string(),
            ),
        ])
        .header("Accept", "application/json")
        .header("User-Agent", "appcontrol-backend");

    if let Some(q) = &req.query {
        request = request.query(&[("sysparm_query", q.as_str())]);
    }

    request = match req.auth_scheme.as_str() {
        "bearer" => request.bearer_auth(&token),
        _ => request.header("Authorization", format!("Basic {}", token)),
    };

    let resp = request.send().await.map_err(|e| ApiError::Internal(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!(
            "ServiceNow returned HTTP {}: {}",
            status, body
        )));
    }

    let parsed: SnTableResponse = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("invalid ServiceNow JSON: {}", e)))?;

    Ok(parsed.result.into_iter().map(to_incident).collect())
}

fn parse_sn_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    // ServiceNow returns 'YYYY-MM-DD HH:MM:SS' in UTC.
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .unwrap_or_else(|_| chrono::Utc::now().naive_utc());
    chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc)
}

fn extract_ci_names(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(v) = value else { return Vec::new() };
    match v {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|i| i.get("display_value").and_then(|d| d.as_str()).map(String::from))
            .collect(),
        serde_json::Value::Object(o) => o
            .get("display_value")
            .and_then(|d| d.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default(),
        serde_json::Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn to_incident(sn: SnIncident) -> itsm::ItsmIncident {
    let opened_at = sn
        .opened_at
        .as_deref()
        .map(parse_sn_datetime)
        .unwrap_or_else(chrono::Utc::now);
    let resolved_at = sn
        .resolved_at
        .as_deref()
        .or(sn.closed_at.as_deref())
        .map(parse_sn_datetime);

    itsm::ItsmIncident {
        external_id: sn.sys_id,
        title: sn.short_description.unwrap_or_else(|| "(no title)".to_string()),
        description: sn.description,
        severity: sn.priority,
        status: sn.state,
        opened_at,
        resolved_at,
        root_cause: sn.close_notes,
        impacted_component_names: extract_ci_names(sn.cmdb_ci.as_ref()),
        metadata: serde_json::json!({"source_system": "servicenow"}),
    }
}
