//! MCP tool implementations — each tool wraps a REST API call to the AppControl backend.

use anyhow::{Context, Result};
use serde_json::Value;

/// HTTP client for the AppControl backend API.
pub struct McpClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl McpClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        })
    }

    /// Dispatch a tool call to the appropriate handler.
    pub async fn call_tool(&self, name: &str, args: &Value) -> Result<String> {
        match name {
            "list_apps" => self.list_apps().await,
            "get_app_status" => {
                let app = get_arg_str(args, "app_name")?;
                self.get_app_status(&app).await
            }
            "start_app" => {
                let app = get_arg_str(args, "app_name")?;
                let dry_run = args
                    .get("dry_run")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.start_app(&app, dry_run).await
            }
            "stop_app" => {
                let app = get_arg_str(args, "app_name")?;
                self.stop_app(&app).await
            }
            "diagnose_app" => {
                let app = get_arg_str(args, "app_name")?;
                self.diagnose_app(&app).await
            }
            "get_incidents" => {
                let app = get_arg_str(args, "app_name")?;
                let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(7);
                self.get_incidents(&app, days).await
            }
            "get_topology" => {
                let app = get_arg_str(args, "app_name")?;
                self.get_topology(&app).await
            }
            "estimate_time" => {
                let app = get_arg_str(args, "app_name")?;
                let op = args
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("start");
                self.estimate_time(&app, op).await
            }
            "get_activity" => {
                let app = get_arg_str(args, "app_name")?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
                self.get_activity(&app, limit).await
            }
            "list_agents" => self.list_agents().await,
            _ => anyhow::bail!("Unknown tool: {}", name),
        }
    }

    // -----------------------------------------------------------------------
    // Tool implementations
    // -----------------------------------------------------------------------

    async fn list_apps(&self) -> Result<String> {
        let resp = self.get("/api/v1/apps").await?;
        format_json_response("Applications", &resp)
    }

    async fn get_app_status(&self, app_name: &str) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let status = self
            .get(&format!("/api/v1/orchestration/apps/{}/status", app_id))
            .await?;
        let health = self
            .get(&format!("/api/v1/apps/{}/health-summary", app_id))
            .await?;

        let mut output = format!("## Application Status: {}\n\n", app_name);
        if let Some(components) = status.get("components").and_then(|c| c.as_array()) {
            for comp in components {
                let name = comp.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let state = comp.get("state").and_then(|s| s.as_str()).unwrap_or("?");
                let icon = state_icon(state);
                output.push_str(&format!("{} **{}**: {}\n", icon, name, state));
            }
        }
        if let Some(summary) = health.get("summary") {
            output.push_str(&format!("\n**Health Summary**: {}\n", summary));
        }
        Ok(output)
    }

    async fn start_app(&self, app_name: &str, dry_run: bool) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let body = serde_json::json!({ "dry_run": dry_run });
        let resp = self
            .post(
                &format!("/api/v1/orchestration/apps/{}/start", app_id),
                &body,
            )
            .await?;

        if dry_run {
            let mut output = format!("## Dry Run Plan for {}\n\n", app_name);
            if let Some(plan) = resp.get("plan") {
                if let Some(levels) = plan.get("levels").and_then(|l| l.as_array()) {
                    for (i, level) in levels.iter().enumerate() {
                        if let Some(comps) = level.as_array() {
                            let names: Vec<&str> = comps
                                .iter()
                                .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                                .collect();
                            output.push_str(&format!("Level {}: {}\n", i, names.join(", ")));
                        }
                    }
                }
            }
            Ok(output)
        } else {
            Ok(format!(
                "Starting application **{}**. Components will start in DAG order.",
                app_name
            ))
        }
    }

    async fn stop_app(&self, app_name: &str) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let _resp = self
            .post(
                &format!("/api/v1/orchestration/apps/{}/stop", app_id),
                &serde_json::json!({}),
            )
            .await?;
        Ok(format!(
            "Stopping application **{}**. Components will stop in reverse DAG order.",
            app_name
        ))
    }

    async fn diagnose_app(&self, app_name: &str) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let resp = self
            .post(
                &format!("/api/v1/apps/{}/diagnose", app_id),
                &serde_json::json!({}),
            )
            .await?;
        format_json_response(&format!("Diagnostic for {}", app_name), &resp)
    }

    async fn get_incidents(&self, app_name: &str, days: u64) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let resp = self
            .get(&format!(
                "/api/v1/apps/{}/reports/incidents?days={}",
                app_id, days
            ))
            .await?;
        format_json_response(
            &format!("Incidents for {} (last {} days)", app_name, days),
            &resp,
        )
    }

    async fn get_topology(&self, app_name: &str) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let resp = self
            .get(&format!("/api/v1/apps/{}/topology", app_id))
            .await?;
        format_json_response(&format!("Topology for {}", app_name), &resp)
    }

    async fn estimate_time(&self, app_name: &str, operation: &str) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let resp = self
            .get(&format!(
                "/api/v1/apps/{}/estimates?operation={}",
                app_id, operation
            ))
            .await?;

        let mut output = format!("## Time Estimate: {} {}\n\n", operation, app_name);
        if let Some(estimate) = resp.get("estimate") {
            let typical = estimate
                .get("typical_human")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let worst = estimate
                .get("worst_case_human")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            output.push_str(&format!("**Typical (P50)**: {}\n", typical));
            output.push_str(&format!("**Worst case (P95)**: {}\n", worst));
        }
        if let Some(confidence) = resp.get("confidence").and_then(|c| c.as_str()) {
            output.push_str(&format!("**Confidence**: {}\n", confidence));
        }
        Ok(output)
    }

    async fn get_activity(&self, app_name: &str, limit: u64) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;
        let resp = self
            .get(&format!("/api/v1/apps/{}/activity?limit={}", app_id, limit))
            .await?;
        format_json_response(&format!("Activity for {}", app_name), &resp)
    }

    async fn list_agents(&self) -> Result<String> {
        let resp = self.get("/api/v1/agents").await?;
        format_json_response("Registered Agents", &resp)
    }

    // -----------------------------------------------------------------------
    // HTTP helpers
    // -----------------------------------------------------------------------

    async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .context("HTTP request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("API returned {}: {}", status, body);
        }

        resp.json().await.context("Failed to parse JSON response")
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(body)
            .send()
            .await
            .context("HTTP request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("API returned {}: {}", status, body);
        }

        resp.json().await.context("Failed to parse JSON response")
    }

    /// Resolve an app name to its UUID.
    /// Accepts either a UUID string or a name (searches apps by name).
    async fn resolve_app(&self, name_or_id: &str) -> Result<String> {
        // If it looks like a UUID, use it directly
        if uuid::Uuid::parse_str(name_or_id).is_ok() {
            return Ok(name_or_id.to_string());
        }

        // Search by name
        let apps = self.get("/api/v1/apps").await?;
        if let Some(app_list) = apps.get("applications").and_then(|a| a.as_array()) {
            for app in app_list {
                let app_name = app.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if app_name.eq_ignore_ascii_case(name_or_id) {
                    if let Some(id) = app.get("id").and_then(|i| i.as_str()) {
                        return Ok(id.to_string());
                    }
                }
            }
        }

        anyhow::bail!("Application '{}' not found", name_or_id)
    }
}

fn get_arg_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: {}", key))
}

fn state_icon(state: &str) -> &str {
    match state {
        "RUNNING" => "[OK]",
        "FAILED" => "[FAIL]",
        "STOPPED" => "[STOP]",
        "DEGRADED" => "[WARN]",
        "STARTING" | "STOPPING" => "[...]",
        "UNREACHABLE" => "[X]",
        _ => "[?]",
    }
}

fn format_json_response(title: &str, data: &Value) -> Result<String> {
    Ok(format!(
        "## {}\n\n```json\n{}\n```",
        title,
        serde_json::to_string_pretty(data)?
    ))
}
