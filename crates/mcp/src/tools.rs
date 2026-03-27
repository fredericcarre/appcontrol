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
            // Log access tools
            "list_log_sources" => {
                let app = get_arg_str(args, "app_name")?;
                let component = get_arg_str(args, "component_name")?;
                self.list_log_sources(&app, &component).await
            }
            "get_component_logs" => {
                let app = get_arg_str(args, "app_name")?;
                let component = get_arg_str(args, "component_name")?;
                let source = args.get("source").and_then(|v| v.as_str());
                let lines = args.get("lines").and_then(|v| v.as_u64()).map(|v| v as i32);
                let filter = args.get("filter").and_then(|v| v.as_str());
                let since = args.get("since").and_then(|v| v.as_str());
                self.get_component_logs(&app, &component, source, lines, filter, since)
                    .await
            }
            "run_diagnostic_command" => {
                let app = get_arg_str(args, "app_name")?;
                let component = get_arg_str(args, "component_name")?;
                let command_name = get_arg_str(args, "command_name")?;
                self.run_diagnostic_command(&app, &component, &command_name)
                    .await
            }
            "search_logs" => {
                let app = get_arg_str(args, "app_name")?;
                let pattern = get_arg_str(args, "pattern")?;
                let level = args.get("level").and_then(|v| v.as_str());
                let since = args.get("since").and_then(|v| v.as_str()).unwrap_or("1h");
                self.search_logs(&app, &pattern, level, since).await
            }
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
    // Log access tools
    // -----------------------------------------------------------------------

    async fn list_log_sources(&self, app_name: &str, component_name: &str) -> Result<String> {
        let component_id = self.resolve_component(app_name, component_name).await?;
        let resp = self
            .get(&format!("/api/v1/components/{}/log-sources", component_id))
            .await?;

        let mut output = format!("## Log Sources for {} / {}\n\n", app_name, component_name);

        // Always available: process stdout/stderr
        output.push_str("### Process Output (always available)\n");
        output.push_str("- **process**: Console stdout/stderr captured by AppControl\n\n");

        if let Some(sources) = resp.as_array() {
            if !sources.is_empty() {
                output.push_str("### Declared Log Sources\n\n");
                for source in sources {
                    let id = source.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = source.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let source_type = source
                        .get("source_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let description = source
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let is_sensitive = source
                        .get("is_sensitive")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let type_icon = match source_type {
                        "file" => "[FILE]",
                        "event_log" => "[EVENTLOG]",
                        "command" => "[CMD]",
                        _ => "[?]",
                    };

                    let sensitive_tag = if is_sensitive { " [SENSITIVE]" } else { "" };
                    output.push_str(&format!(
                        "- **{}**: {} `{}`{}\n",
                        name, type_icon, id, sensitive_tag
                    ));
                    if !description.is_empty() {
                        output.push_str(&format!("  _{}_\n", description));
                    }

                    // Show additional details based on type
                    match source_type {
                        "file" => {
                            if let Some(path) = source.get("file_path").and_then(|v| v.as_str()) {
                                output.push_str(&format!("  Path: `{}`\n", path));
                            }
                        }
                        "event_log" => {
                            if let Some(log) = source.get("event_log_name").and_then(|v| v.as_str())
                            {
                                output.push_str(&format!("  Log: {}\n", log));
                            }
                            if let Some(src) =
                                source.get("event_log_source").and_then(|v| v.as_str())
                            {
                                output.push_str(&format!("  Source: {}\n", src));
                            }
                        }
                        "command" => {
                            if let Some(cmd) = source.get("command").and_then(|v| v.as_str()) {
                                output.push_str(&format!("  Command: `{}`\n", cmd));
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                output.push_str("_No additional log sources declared._\n");
            }
        }

        Ok(output)
    }

    async fn get_component_logs(
        &self,
        app_name: &str,
        component_name: &str,
        source: Option<&str>,
        lines: Option<i32>,
        filter: Option<&str>,
        since: Option<&str>,
    ) -> Result<String> {
        let component_id = self.resolve_component(app_name, component_name).await?;

        // Build query parameters
        let mut params = Vec::new();
        if let Some(s) = source {
            params.push(format!("source={}", s));
        }
        if let Some(l) = lines {
            params.push(format!("lines={}", l.min(1000))); // Cap at 1000
        }
        if let Some(f) = filter {
            params.push(format!("filter={}", urlencoding::encode(f)));
        }
        if let Some(s) = since {
            params.push(format!("since={}", s));
        }

        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };

        let resp = self
            .get(&format!(
                "/api/v1/components/{}/logs{}",
                component_id, query
            ))
            .await?;

        let source_type = resp
            .get("source_type")
            .and_then(|v| v.as_str())
            .unwrap_or("process");
        let source_name = resp
            .get("source_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Console output");
        let total_lines = resp
            .get("total_lines")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let truncated = resp
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut output = format!(
            "## Logs: {} / {} - {}\n\n",
            app_name, component_name, source_name
        );

        output.push_str(&format!(
            "_Source: {} | Lines: {}{}_\n\n",
            source_type,
            total_lines,
            if truncated { " (truncated)" } else { "" }
        ));

        output.push_str("```\n");
        if let Some(entries) = resp.get("entries").and_then(|v| v.as_array()) {
            for entry in entries {
                let timestamp = entry
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let level = entry.get("level").and_then(|v| v.as_str());
                let content = entry.get("content").and_then(|v| v.as_str()).unwrap_or("");

                if let Some(lvl) = level {
                    output.push_str(&format!("{} [{}] {}\n", timestamp, lvl, content));
                } else if !timestamp.is_empty() {
                    output.push_str(&format!("{} {}\n", timestamp, content));
                } else {
                    output.push_str(&format!("{}\n", content));
                }
            }
        }
        output.push_str("```\n");

        Ok(output)
    }

    async fn run_diagnostic_command(
        &self,
        app_name: &str,
        component_name: &str,
        command_name: &str,
    ) -> Result<String> {
        let component_id = self.resolve_component(app_name, component_name).await?;
        let resp = self
            .post(
                &format!(
                    "/api/v1/components/{}/logs/command/{}",
                    component_id,
                    urlencoding::encode(command_name)
                ),
                &serde_json::json!({}),
            )
            .await?;

        let exit_code = resp.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
        let duration_ms = resp
            .get("duration_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let stdout = resp.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
        let stderr = resp.get("stderr").and_then(|v| v.as_str()).unwrap_or("");

        let status_icon = if exit_code == 0 { "[OK]" } else { "[FAIL]" };

        let mut output = format!(
            "## Diagnostic Command: {} {} ({}ms)\n\n",
            command_name, status_icon, duration_ms
        );

        output.push_str(&format!("_Exit code: {}_\n\n", exit_code));

        if !stdout.is_empty() {
            output.push_str("### Output\n```\n");
            output.push_str(stdout);
            if !stdout.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n");
        }

        if !stderr.is_empty() {
            output.push_str("\n### Errors\n```\n");
            output.push_str(stderr);
            if !stderr.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n");
        }

        Ok(output)
    }

    async fn search_logs(
        &self,
        app_name: &str,
        pattern: &str,
        level: Option<&str>,
        since: &str,
    ) -> Result<String> {
        let app_id = self.resolve_app(app_name).await?;

        // Get all components for the app
        let status = self
            .get(&format!("/api/v1/orchestration/apps/{}/status", app_id))
            .await?;

        let mut output = format!("## Log Search: '{}' in {}\n\n", pattern, app_name);
        output.push_str(&format!(
            "_Filter: level={}, since={}_\n\n",
            level.unwrap_or("ALL"),
            since
        ));

        let mut total_matches = 0;

        if let Some(components) = status.get("components").and_then(|c| c.as_array()) {
            for comp in components {
                let comp_id = match comp.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };
                let comp_name = comp
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                // Build query with filter=pattern
                let mut params = vec![
                    format!("filter={}", urlencoding::encode(pattern)),
                    format!("since={}", since),
                    "lines=50".to_string(),
                ];
                if let Some(lvl) = level {
                    params.push(format!("filter={}", lvl));
                }

                let query = format!("?{}", params.join("&"));
                let logs_result = self
                    .get(&format!("/api/v1/components/{}/logs{}", comp_id, query))
                    .await;

                if let Ok(logs) = logs_result {
                    if let Some(entries) = logs.get("entries").and_then(|v| v.as_array()) {
                        if !entries.is_empty() {
                            total_matches += entries.len();
                            output.push_str(&format!(
                                "### {} ({} matches)\n```\n",
                                comp_name,
                                entries.len()
                            ));
                            for entry in entries.iter().take(10) {
                                // Limit per component
                                let ts = entry
                                    .get("timestamp")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let content =
                                    entry.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                output.push_str(&format!("{} {}\n", ts, content));
                            }
                            if entries.len() > 10 {
                                output.push_str(&format!("... and {} more\n", entries.len() - 10));
                            }
                            output.push_str("```\n\n");
                        }
                    }
                }
            }
        }

        if total_matches == 0 {
            output.push_str("_No matches found._\n");
        } else {
            output.push_str(&format!("\n**Total: {} matches**\n", total_matches));
        }

        Ok(output)
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

    /// Resolve a component name to its UUID.
    /// Requires the app name/id to look up the component within that app.
    async fn resolve_component(&self, app_name: &str, component_name: &str) -> Result<String> {
        // If component_name looks like a UUID, use it directly
        if uuid::Uuid::parse_str(component_name).is_ok() {
            return Ok(component_name.to_string());
        }

        // First resolve the app
        let app_id = self.resolve_app(app_name).await?;

        // Get app status which includes components
        let status = self
            .get(&format!("/api/v1/orchestration/apps/{}/status", app_id))
            .await?;

        if let Some(components) = status.get("components").and_then(|c| c.as_array()) {
            for comp in components {
                let comp_name = comp.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if comp_name.eq_ignore_ascii_case(component_name) {
                    if let Some(id) = comp.get("id").and_then(|i| i.as_str()) {
                        return Ok(id.to_string());
                    }
                }
            }
        }

        anyhow::bail!(
            "Component '{}' not found in application '{}'",
            component_name,
            app_name
        )
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
