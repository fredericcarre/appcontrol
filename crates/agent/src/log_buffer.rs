//! Log buffer for capturing and storing process output.
//!
//! This module provides:
//! - Ring buffer for process stdout/stderr capture (per-component)
//! - File log reading with tail-like functionality
//! - Windows Event Log reading (Windows only)
//!
//! Process output is captured from commands started by AppControl and stored
//! in a per-component ring buffer. This allows Claude/MCP to retrieve recent
//! logs without requiring direct access to the remote machine.

// Allow dead code for ring buffer implementation - these will be used when
// executor integration is complete to capture process output
#![allow(dead_code)]

use appcontrol_common::protocol::ComponentLogEntry;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Default ring buffer size per component (number of lines)
const DEFAULT_BUFFER_LINES: usize = 10_000;

/// Maximum line length before truncation
const MAX_LINE_LENGTH: usize = 4096;

/// A line in the log buffer
#[derive(Debug, Clone)]
struct BufferedLine {
    timestamp: DateTime<Utc>,
    level: Option<String>,
    content: String,
    is_stderr: bool,
}

/// Per-component log ring buffer
struct ComponentBuffer {
    lines: VecDeque<BufferedLine>,
    max_lines: usize,
}

impl ComponentBuffer {
    fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines.min(1000)), // Pre-allocate reasonably
            max_lines,
        }
    }

    fn push(&mut self, line: BufferedLine) {
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn get_lines(
        &self,
        count: usize,
        filter: Option<&str>,
        since: Option<DateTime<Utc>>,
    ) -> Vec<ComponentLogEntry> {
        self.lines
            .iter()
            .filter(|l| {
                // Time filter
                if let Some(since_time) = since {
                    if l.timestamp < since_time {
                        return false;
                    }
                }
                // Text filter
                if let Some(pattern) = filter {
                    let pattern_upper = pattern.to_uppercase();
                    // Check if it's a level filter
                    if ["ERROR", "WARN", "INFO", "DEBUG"].contains(&pattern_upper.as_str()) {
                        if let Some(ref level) = l.level {
                            return level.to_uppercase() == pattern_upper;
                        }
                        return false;
                    }
                    // Otherwise, text search
                    if !l.content.to_lowercase().contains(&pattern.to_lowercase()) {
                        return false;
                    }
                }
                true
            })
            .rev() // Most recent first
            .take(count)
            .map(|l| ComponentLogEntry {
                timestamp: Some(l.timestamp),
                level: l.level.clone(),
                content: l.content.clone(),
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev() // Back to chronological order
            .collect()
    }
}

/// Global log buffer manager
pub struct LogBufferManager {
    buffers: Arc<RwLock<HashMap<Uuid, ComponentBuffer>>>,
    default_max_lines: usize,
}

impl LogBufferManager {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
            default_max_lines: DEFAULT_BUFFER_LINES,
        }
    }

    /// Add a line to a component's buffer
    pub async fn push_line(&self, component_id: Uuid, content: String, is_stderr: bool) {
        let mut content = content;
        if content.len() > MAX_LINE_LENGTH {
            content.truncate(MAX_LINE_LENGTH);
            content.push_str("...[truncated]");
        }

        // Try to detect log level from content
        let level = detect_log_level(&content);

        let line = BufferedLine {
            timestamp: Utc::now(),
            level,
            content,
            is_stderr,
        };

        let mut buffers = self.buffers.write().await;
        let buffer = buffers
            .entry(component_id)
            .or_insert_with(|| ComponentBuffer::new(self.default_max_lines));
        buffer.push(line);
    }

    /// Add multiple lines at once
    pub async fn push_output(&self, component_id: Uuid, stdout: &str, stderr: &str) {
        for line in stdout.lines() {
            if !line.is_empty() {
                self.push_line(component_id, line.to_string(), false).await;
            }
        }
        for line in stderr.lines() {
            if !line.is_empty() {
                self.push_line(component_id, line.to_string(), true).await;
            }
        }
    }

    /// Get logs from a component's buffer
    pub async fn get_logs(
        &self,
        component_id: Uuid,
        lines: Option<i32>,
        filter: Option<&str>,
        since: Option<&str>,
    ) -> Vec<ComponentLogEntry> {
        let lines = lines.unwrap_or(100).min(1000) as usize;
        let since_time = parse_since(since);

        let buffers = self.buffers.read().await;
        if let Some(buffer) = buffers.get(&component_id) {
            buffer.get_lines(lines, filter, since_time)
        } else {
            Vec::new()
        }
    }

    /// Configure buffer size for a component
    pub async fn set_buffer_size(&self, component_id: Uuid, max_lines: usize) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&component_id) {
            buffer.max_lines = max_lines;
            // Trim if needed
            while buffer.lines.len() > max_lines {
                buffer.lines.pop_front();
            }
        }
    }

    /// Clear buffer for a component
    pub async fn clear(&self, component_id: Uuid) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(&component_id) {
            buffer.lines.clear();
        }
    }
}

impl Default for LogBufferManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect log level from line content
fn detect_log_level(content: &str) -> Option<String> {
    let content_upper = content.to_uppercase();

    // Common log patterns
    if content_upper.contains("[ERROR]")
        || content_upper.contains("ERROR:")
        || content_upper.contains(" ERROR ")
        || content_upper.starts_with("ERROR")
    {
        return Some("ERROR".to_string());
    }
    if content_upper.contains("[WARN]")
        || content_upper.contains("WARN:")
        || content_upper.contains("WARNING:")
        || content_upper.contains(" WARN ")
        || content_upper.contains(" WARNING ")
    {
        return Some("WARN".to_string());
    }
    if content_upper.contains("[INFO]")
        || content_upper.contains("INFO:")
        || content_upper.contains(" INFO ")
    {
        return Some("INFO".to_string());
    }
    if content_upper.contains("[DEBUG]")
        || content_upper.contains("DEBUG:")
        || content_upper.contains(" DEBUG ")
    {
        return Some("DEBUG".to_string());
    }

    None
}

/// Parse "since" time range string
fn parse_since(since: Option<&str>) -> Option<DateTime<Utc>> {
    let since = since?;
    let now = Utc::now();

    // Parse formats like "1h", "6h", "24h", "7d"
    if let Some(stripped) = since.strip_suffix('h') {
        if let Ok(hours) = stripped.parse::<i64>() {
            return Some(now - chrono::Duration::hours(hours));
        }
    }
    if let Some(stripped) = since.strip_suffix('d') {
        if let Ok(days) = stripped.parse::<i64>() {
            return Some(now - chrono::Duration::days(days));
        }
    }
    if let Some(stripped) = since.strip_suffix('m') {
        if let Ok(minutes) = stripped.parse::<i64>() {
            return Some(now - chrono::Duration::minutes(minutes));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// File log reading
// ---------------------------------------------------------------------------

/// Read the last N lines from a file
pub async fn read_file_tail(
    file_path: &str,
    lines: Option<i32>,
    filter: Option<&str>,
    since: Option<&str>,
) -> Result<(Vec<ComponentLogEntry>, bool), String> {
    use tokio::fs::File;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let max_lines = lines.unwrap_or(100).min(1000) as usize;
    let since_time = parse_since(since);

    let file = File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let reader = BufReader::new(file);
    let mut all_lines: VecDeque<String> = VecDeque::new();
    let mut lines_stream = reader.lines();

    // Read all lines (inefficient for large files, but simple)
    // TODO: For large files, implement reverse reading from end
    while let Some(line) = lines_stream
        .next_line()
        .await
        .map_err(|e| format!("Read error: {}", e))?
    {
        all_lines.push_back(line);
        // Keep only last N*2 lines in memory to handle filtering
        if all_lines.len() > max_lines * 2 {
            all_lines.pop_front();
        }
    }

    let mut result: Vec<ComponentLogEntry> = Vec::new();
    let filter_lower = filter.map(|f| f.to_lowercase());

    for line in all_lines.iter().rev() {
        // Apply filter
        if let Some(ref f) = filter_lower {
            let f_upper = f.to_uppercase();
            // Level filter
            if ["error", "warn", "info", "debug"].contains(&f.as_str()) {
                let level = detect_log_level(line);
                if level.as_ref().map(|l| l.to_uppercase()) != Some(f_upper) {
                    continue;
                }
            } else if !line.to_lowercase().contains(f) {
                continue;
            }
        }

        let level = detect_log_level(line);
        // Try to parse timestamp from line (various formats)
        let timestamp = parse_log_timestamp(line);

        // Apply since filter if we got a timestamp
        if let (Some(since_t), Some(line_t)) = (since_time, timestamp) {
            if line_t < since_t {
                continue;
            }
        }

        result.push(ComponentLogEntry {
            timestamp,
            level,
            content: line.clone(),
        });

        if result.len() >= max_lines {
            break;
        }
    }

    let truncated = result.len() >= max_lines;
    result.reverse(); // Back to chronological order

    Ok((result, truncated))
}

/// Try to parse a timestamp from a log line
fn parse_log_timestamp(line: &str) -> Option<DateTime<Utc>> {
    // Common timestamp patterns at the start of log lines
    // ISO 8601: 2024-01-15T10:30:45.123Z
    // Common: 2024-01-15 10:30:45
    // Syslog: Jan 15 10:30:45

    // Try ISO 8601 first
    if line.len() >= 24 {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&line[..24]) {
            return Some(dt.with_timezone(&Utc));
        }
    }

    // Try common format: YYYY-MM-DD HH:MM:SS
    if line.len() >= 19 {
        let potential = &line[..19];
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(potential, "%Y-%m-%d %H:%M:%S") {
            return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Windows Event Log reading (Windows only)
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub async fn read_event_log(
    log_name: &str,
    source: Option<&str>,
    level: Option<&str>,
    lines: Option<i32>,
    since: Option<&str>,
) -> Result<(Vec<ComponentLogEntry>, bool), String> {
    use std::process::Command;

    let max_lines = lines.unwrap_or(100).min(500) as usize;
    let since_hours = match since {
        Some(s) if s.ends_with('h') => s[..s.len() - 1].parse::<i64>().unwrap_or(24),
        Some(s) if s.ends_with('d') => s[..s.len() - 1].parse::<i64>().unwrap_or(1) * 24,
        _ => 24,
    };

    // Build PowerShell command to query event log
    let mut ps_cmd = format!(
        "Get-WinEvent -LogName '{}' -MaxEvents {} | Where-Object {{ $_.TimeCreated -gt (Get-Date).AddHours(-{}) }}",
        log_name, max_lines * 2, since_hours
    );

    if let Some(src) = source {
        ps_cmd.push_str(&format!(
            " | Where-Object {{ $_.ProviderName -eq '{}' }}",
            src
        ));
    }

    if let Some(lvl) = level {
        let level_id = match lvl.to_uppercase().as_str() {
            "ERROR" => "2",
            "WARNING" | "WARN" => "3",
            "INFORMATION" | "INFO" => "4",
            _ => "",
        };
        if !level_id.is_empty() {
            ps_cmd.push_str(&format!(" | Where-Object {{ $_.Level -eq {} }}", level_id));
        }
    }

    ps_cmd.push_str(" | Select-Object TimeCreated, LevelDisplayName, Message | ConvertTo-Json");

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_cmd])
        .output()
        .map_err(|e| format!("Failed to run PowerShell: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PowerShell error: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let entries: Vec<ComponentLogEntry> = if stdout.trim().is_empty() {
        Vec::new()
    } else {
        let json: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| format!("JSON parse error: {}", e))?;

        let events = if json.is_array() {
            json.as_array().unwrap().clone()
        } else {
            vec![json]
        };

        events
            .into_iter()
            .take(max_lines)
            .map(|e| {
                let timestamp = e
                    .get("TimeCreated")
                    .and_then(|t| t.as_str())
                    .and_then(|s| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%:z").ok())
                    .map(|dt| dt.with_timezone(&Utc));
                let level = e.get("LevelDisplayName").and_then(|l| l.as_str()).map(|s| {
                    match s {
                        "Error" => "ERROR",
                        "Warning" => "WARN",
                        "Information" => "INFO",
                        _ => s,
                    }
                    .to_string()
                });
                let content = e
                    .get("Message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();

                ComponentLogEntry {
                    timestamp,
                    level,
                    content,
                }
            })
            .collect()
    };

    let truncated = entries.len() >= max_lines;
    Ok((entries, truncated))
}

#[cfg(not(windows))]
pub async fn read_event_log(
    _log_name: &str,
    _source: Option<&str>,
    _level: Option<&str>,
    _lines: Option<i32>,
    _since: Option<&str>,
) -> Result<(Vec<ComponentLogEntry>, bool), String> {
    Err("Windows Event Log is only available on Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_push_and_get() {
        let manager = LogBufferManager::new();
        let component_id = Uuid::new_v4();

        manager
            .push_line(component_id, "Line 1".to_string(), false)
            .await;
        manager
            .push_line(component_id, "Line 2".to_string(), false)
            .await;
        manager
            .push_line(component_id, "[ERROR] Something failed".to_string(), true)
            .await;

        let logs = manager.get_logs(component_id, Some(10), None, None).await;
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[2].level, Some("ERROR".to_string()));
    }

    #[tokio::test]
    async fn test_buffer_filter() {
        let manager = LogBufferManager::new();
        let component_id = Uuid::new_v4();

        manager
            .push_line(component_id, "[INFO] Starting".to_string(), false)
            .await;
        manager
            .push_line(component_id, "[ERROR] Failed".to_string(), true)
            .await;
        manager
            .push_line(component_id, "[INFO] Completed".to_string(), false)
            .await;

        let logs = manager
            .get_logs(component_id, Some(10), Some("ERROR"), None)
            .await;
        assert_eq!(logs.len(), 1);
        assert!(logs[0].content.contains("Failed"));
    }

    #[test]
    fn test_detect_log_level() {
        assert_eq!(
            detect_log_level("[ERROR] failed"),
            Some("ERROR".to_string())
        );
        assert_eq!(
            detect_log_level("2024-01-15 WARN: warning"),
            Some("WARN".to_string())
        );
        assert_eq!(detect_log_level("INFO: started"), Some("INFO".to_string()));
        assert_eq!(detect_log_level("just a line"), None);
    }

    #[test]
    fn test_parse_since() {
        let now = Utc::now();

        let since_1h = parse_since(Some("1h")).unwrap();
        assert!((now - since_1h).num_minutes() >= 59);

        let since_24h = parse_since(Some("24h")).unwrap();
        assert!((now - since_24h).num_hours() >= 23);

        let since_7d = parse_since(Some("7d")).unwrap();
        assert!((now - since_7d).num_days() >= 6);
    }
}
