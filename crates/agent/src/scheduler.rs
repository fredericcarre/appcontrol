use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use appcontrol_common::{AgentMessage, CheckResult, CheckType, ComponentConfig};

/// Staleness timeout: force-send a check result even if exit code hasn't changed
/// after this many seconds. Ensures the backend always has a recent data point
/// for each component, even if the health status never flips.
const STALENESS_TIMEOUT_SECS: i64 = 300; // 5 minutes

/// Minimum check tick resolution. The scheduler wakes up on this interval
/// and evaluates which components are due for a health check.
const TICK_INTERVAL_SECS: u64 = 5;

/// Number of ticks between heartbeats (60s / 5s = 12 ticks).
const HEARTBEAT_EVERY_N_TICKS: u64 = 60 / TICK_INTERVAL_SECS;

/// Per-component tracking for individual check intervals and delta dedup.
struct ComponentCheckState {
    /// Last time this component's check was *started* (not completed).
    /// Using start time prevents drift: if a check takes 5s and interval is 30s,
    /// the next check fires at 30s from start, not 35s.
    last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last known exit code for delta comparison.
    last_exit_code: Option<i32>,
    /// Last time a CheckResult was actually sent to the backend.
    last_sent_at: Option<chrono::DateTime<chrono::Utc>>,
    /// True while a check is in flight. Prevents piling: if a check takes longer
    /// than its interval, the scheduler skips the next evaluation instead of
    /// queuing a second concurrent check.
    in_flight: bool,
}

/// Manages periodic health check scheduling for all assigned components.
///
/// Architecture decisions:
/// - **Agent-driven checks**: The agent runs checks locally on its own timer.
///   The backend only configures *what* to check via UpdateConfig messages.
///   This keeps network traffic minimal (no polling from backend).
/// - **Per-component intervals**: Each component's `check_interval_seconds` is
///   respected individually. A 5-second tick evaluates which are due.
/// - **Command hash dedup**: If multiple components share the same `check_cmd`
///   (e.g., `pgrep nginx` across 3 maps), the command executes only ONCE per
///   cycle. The result is shared across all components with the same command.
/// - **Delta-only sync**: CheckResult is sent only when the exit code changes
///   from the last known value, dramatically reducing network traffic.
/// - **Staleness protection**: Even if exit code never changes, a force-send
///   occurs every 5 minutes so the backend knows the component is still monitored.
pub struct CheckScheduler {
    agent_id: Uuid,
    msg_tx: mpsc::UnboundedSender<AgentMessage>,
    components: tokio::sync::RwLock<HashMap<Uuid, ComponentConfig>>,
    /// Per-component state: tracks last check time, last exit code, last send time.
    check_state: tokio::sync::RwLock<HashMap<Uuid, ComponentCheckState>>,
}

impl CheckScheduler {
    pub fn new(agent_id: Uuid, msg_tx: mpsc::UnboundedSender<AgentMessage>) -> Self {
        Self {
            agent_id,
            msg_tx,
            components: tokio::sync::RwLock::new(HashMap::new()),
            check_state: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Trigger immediate health checks for all components by resetting their
    /// last_checked_at timestamps. This forces them to be "due" on the next scheduler tick.
    ///
    /// Used when agent reconnects to quickly recover component states from UNREACHABLE.
    pub async fn run_all_checks_now(&self) {
        let mut check_state = self.check_state.write().await;
        for state in check_state.values_mut() {
            // Reset last_checked_at to force the component to be "due" immediately
            state.last_checked_at = None;
            // Also reset last_sent_at to ensure we send the result regardless of staleness
            state.last_sent_at = None;
        }
        tracing::info!(
            "Triggered immediate checks for {} components",
            check_state.len()
        );
        drop(check_state);

        // Run due checks immediately instead of waiting for the next tick
        self.run_due_checks().await;
    }

    /// Replace the current component list with a new configuration from the backend.
    /// Cleans up check state for removed components.
    pub async fn update_components(&self, configs: Vec<ComponentConfig>) {
        let mut components = self.components.write().await;
        let mut check_state = self.check_state.write().await;

        // Collect new component IDs
        let new_ids: std::collections::HashSet<Uuid> =
            configs.iter().map(|c| c.component_id).collect();

        // Remove stale check state for components no longer in the config
        check_state.retain(|id, _| new_ids.contains(id));

        // Replace component configs
        let with_check_cmd = configs.iter().filter(|c| c.check_cmd.is_some()).count();
        components.clear();
        for config in configs {
            components.insert(config.component_id, config);
        }
        tracing::info!(
            "Scheduler updated: {} total components, {} with check_cmd",
            components.len(),
            with_check_cmd
        );
    }

    /// Main scheduler loop. Sends heartbeats every 60s and evaluates
    /// per-component check intervals on a 5-second tick.
    pub async fn run(self: Arc<Self>) {
        let mut tick_interval = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
        let mut heartbeat_counter: u64 = 0;

        loop {
            tick_interval.tick().await;
            heartbeat_counter += 1;

            // Send heartbeat every ~60 seconds
            if heartbeat_counter.is_multiple_of(HEARTBEAT_EVERY_N_TICKS) {
                self.send_heartbeat();
            }

            // Evaluate and run due health checks
            self.run_due_checks().await;
        }
    }

    fn send_heartbeat(&self) {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.refresh_cpu_usage();
        let cpu = sysinfo::System::load_average().one as f32;
        let memory = if sys.total_memory() > 0 {
            sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0
        } else {
            0.0
        };

        // Calculate disk usage percentage for root filesystem
        let disk = {
            let disks = sysinfo::Disks::new_with_refreshed_list();
            // Find the root filesystem (or largest disk as fallback)
            disks
                .iter()
                .find(|d| {
                    d.mount_point().to_str() == Some("/")
                        || d.mount_point().to_str() == Some("C:\\")
                })
                .or_else(|| disks.iter().max_by_key(|d| d.total_space()))
                .map(|d| {
                    let total = d.total_space() as f64;
                    let available = d.available_space() as f64;
                    if total > 0.0 {
                        ((total - available) / total * 100.0) as f32
                    } else {
                        0.0
                    }
                })
        };

        let _ = self.msg_tx.send(AgentMessage::Heartbeat {
            agent_id: self.agent_id,
            cpu,
            memory,
            disk,
            at: chrono::Utc::now(),
        });
    }

    /// Evaluate which components are due for a check and run them.
    ///
    /// Anti-drift and anti-piling guarantees:
    /// - `last_checked_at` is set at check *start* (not completion), so a check
    ///   that takes 5s with a 30s interval fires next at t=30s, not t=35s.
    /// - `in_flight` flag prevents double-execution: if a check takes longer
    ///   than its interval, the scheduler skips that component until it finishes.
    /// - Command hash dedup: identical commands execute only once per cycle.
    async fn run_due_checks(&self) {
        let now = chrono::Utc::now();
        let components = self.components.read().await;

        if components.is_empty() {
            return;
        }

        // Determine which components are due for a check
        let mut due_components: Vec<(Uuid, &ComponentConfig)> = Vec::new();

        {
            let check_state = self.check_state.read().await;
            for (comp_id, config) in components.iter() {
                if config.check_cmd.is_none() {
                    continue;
                }

                let interval_secs = if config.check_interval_seconds > 0 {
                    config.check_interval_seconds as i64
                } else {
                    30 // default to 30s per spec
                };

                let is_due = match check_state.get(comp_id) {
                    Some(state) => {
                        // Skip if already in flight (prevents piling)
                        if state.in_flight {
                            continue;
                        }
                        match state.last_checked_at {
                            Some(last) => (now - last).num_seconds() >= interval_secs,
                            None => true, // never checked
                        }
                    }
                    None => true, // no state yet
                };

                if is_due {
                    due_components.push((*comp_id, config));
                }
            }
        } // drop read lock

        if due_components.is_empty() {
            return;
        }

        tracing::debug!("Running checks for {} due components", due_components.len());

        // Mark all due components as in_flight and set last_checked_at to NOW
        // (start time, not completion time — prevents drift)
        {
            let mut state = self.check_state.write().await;
            for (comp_id, _) in &due_components {
                let cs = state
                    .entry(*comp_id)
                    .or_insert_with(|| ComponentCheckState {
                        last_checked_at: None,
                        last_exit_code: None,
                        last_sent_at: None,
                        in_flight: false,
                    });
                cs.in_flight = true;
                cs.last_checked_at = Some(now);
            }
        }

        // Execute checks with command hash deduplication.
        // If multiple components share the same check_cmd, execute once and share the result.
        let mut executed_cmds: HashMap<String, (i32, String, u32)> = HashMap::new();

        for (comp_id, config) in &due_components {
            let check_cmd = config.check_cmd.as_ref().unwrap();
            let cmd_hash = hash_command(check_cmd);

            let (exit_code, stdout, duration_ms) = if let Some(cached) =
                executed_cmds.get(&cmd_hash)
            {
                cached.clone()
            } else {
                let timeout = Duration::from_secs(30);
                let start = std::time::Instant::now();
                match crate::executor::execute_sync(check_cmd, timeout).await {
                    Ok(result) => {
                        let entry = (result.exit_code, result.stdout.clone(), result.duration_ms);
                        executed_cmds.insert(cmd_hash, entry.clone());
                        entry
                    }
                    Err(_) => {
                        let duration = start.elapsed().as_millis() as u32;
                        let entry = (-1i32, String::new(), duration);
                        executed_cmds.insert(cmd_hash, entry.clone());
                        entry
                    }
                }
            };

            // Update check state, clear in_flight, and determine if we should send
            let mut state = self.check_state.write().await;
            let cs = state
                .entry(*comp_id)
                .or_insert_with(|| ComponentCheckState {
                    last_checked_at: None,
                    last_exit_code: None,
                    last_sent_at: None,
                    in_flight: false,
                });

            cs.in_flight = false;

            // Delta check: send if exit code changed OR staleness timeout exceeded
            let exit_code_changed = cs.last_exit_code != Some(exit_code);
            let stale = cs
                .last_sent_at
                .is_none_or(|last| (now - last).num_seconds() >= STALENESS_TIMEOUT_SECS);

            if exit_code_changed || stale {
                cs.last_exit_code = Some(exit_code);
                cs.last_sent_at = Some(now);

                tracing::debug!(
                    component_id = %comp_id,
                    exit_code = exit_code,
                    reason = if exit_code_changed { "changed" } else { "stale" },
                    "Sending CheckResult"
                );

                // Extract generic metrics from stdout (any valid JSON)
                let metrics = extract_metrics_from_stdout(&stdout);

                let _ = self.msg_tx.send(AgentMessage::CheckResult(CheckResult {
                    component_id: *comp_id,
                    check_type: CheckType::Health,
                    exit_code,
                    stdout: Some(stdout.clone()),
                    duration_ms,
                    at: now,
                    metrics,
                }));
            }
        }
    }
}

fn hash_command(cmd: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cmd.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract generic metrics from check command stdout.
///
/// Supports multiple formats (tried in order):
/// 1. Entire stdout is valid JSON: `{"users": 12, "queue": 150}`
/// 2. JSON wrapped in `<appcontrol>...</appcontrol>` tags (legacy format)
/// 3. JSON after a `---METRICS---` marker
/// 4. Auto-detect: last JSON object found in output (for mixed logs + JSON)
///
/// Returns `None` if no valid JSON is found (which is fine - most checks
/// don't return metrics).
fn extract_metrics_from_stdout(stdout: &str) -> Option<serde_json::Value> {
    let trimmed = stdout.trim();

    // 1. Try parsing entire stdout as JSON (pure JSON output)
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.is_object() {
            return Some(value);
        }
    }

    // 2. Legacy format: <appcontrol>JSON</appcontrol>
    if let Some(start) = trimmed.find("<appcontrol>") {
        if let Some(end) = trimmed.find("</appcontrol>") {
            let json_part = &trimmed[start + 12..end];
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_part.trim()) {
                if value.is_object() {
                    return Some(value);
                }
            }
        }
    }

    // 3. Marker format: ---METRICS---\nJSON
    if let Some(marker_pos) = trimmed.find("---METRICS---") {
        let json_part = &trimmed[marker_pos + 13..];
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_part.trim()) {
            if value.is_object() {
                return Some(value);
            }
        }
    }

    // 4. Auto-detect: find last line that is a valid JSON object
    // This allows scripts to output logs and JSON mixed together
    for line in stdout.lines().rev() {
        let line_trimmed = line.trim();
        if line_trimmed.starts_with('{') && line_trimmed.ends_with('}') {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line_trimmed) {
                if value.is_object() {
                    return Some(value);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(id: Uuid, check_cmd: &str, interval: u32) -> ComponentConfig {
        ComponentConfig {
            component_id: id,
            name: "test".to_string(),
            check_cmd: Some(check_cmd.to_string()),
            start_cmd: None,
            stop_cmd: None,
            integrity_check_cmd: None,
            post_start_check_cmd: None,
            infra_check_cmd: None,
            rebuild_cmd: None,
            rebuild_infra_cmd: None,
            check_interval_seconds: interval,
            start_timeout_seconds: 120,
            stop_timeout_seconds: 60,
            env_vars: serde_json::json!({}),
        }
    }

    #[test]
    fn test_hash_command_deterministic() {
        let h1 = hash_command("pgrep nginx");
        let h2 = hash_command("pgrep nginx");
        let h3 = hash_command("pgrep apache");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_hash_command_different_for_different_cmds() {
        let h1 = hash_command("echo 1");
        let h2 = hash_command("echo 2");
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn test_update_components_replaces_config() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let scheduler = CheckScheduler::new(Uuid::new_v4(), tx);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        scheduler
            .update_components(vec![
                make_config(id1, "echo ok", 30),
                make_config(id2, "echo ok", 30),
            ])
            .await;

        {
            let components = scheduler.components.read().await;
            assert_eq!(components.len(), 2);
            assert!(components.contains_key(&id1));
            assert!(components.contains_key(&id2));
        }

        // Replace with a single component
        let id3 = Uuid::new_v4();
        scheduler
            .update_components(vec![make_config(id3, "echo ok", 30)])
            .await;

        {
            let components = scheduler.components.read().await;
            assert_eq!(components.len(), 1);
            assert!(components.contains_key(&id3));
            assert!(!components.contains_key(&id1));
        }
    }

    #[tokio::test]
    async fn test_update_components_cleans_stale_check_state() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let scheduler = CheckScheduler::new(Uuid::new_v4(), tx);

        let id1 = Uuid::new_v4();

        // Seed some check state
        {
            let mut state = scheduler.check_state.write().await;
            state.insert(
                id1,
                ComponentCheckState {
                    last_checked_at: Some(chrono::Utc::now()),
                    last_exit_code: Some(0),
                    last_sent_at: Some(chrono::Utc::now()),
                    in_flight: false,
                },
            );
        }

        // Update with a different component — id1 should be cleaned up
        let id2 = Uuid::new_v4();
        scheduler
            .update_components(vec![make_config(id2, "echo ok", 30)])
            .await;

        let state = scheduler.check_state.read().await;
        assert!(!state.contains_key(&id1));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_run_due_checks_executes_and_sends_first_result() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        let id1 = Uuid::new_v4();
        scheduler
            .update_components(vec![make_config(id1, "echo hello", 30)])
            .await;

        // First run should execute the check and send a result
        scheduler.run_due_checks().await;

        // Drain messages, find CheckResult
        let mut found = false;
        while let Ok(msg) = rx.try_recv() {
            if let AgentMessage::CheckResult(cr) = msg {
                assert_eq!(cr.component_id, id1);
                assert_eq!(cr.exit_code, 0);
                assert!(cr.stdout.as_ref().unwrap().contains("hello"));
                assert!(cr.duration_ms < 5000); // should be fast
                found = true;
            }
        }
        assert!(found, "Expected CheckResult to be sent on first check");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_delta_dedup_suppresses_identical_results() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        let id1 = Uuid::new_v4();
        scheduler
            .update_components(vec![make_config(id1, "echo ok", 1)]) // 1s interval
            .await;

        // First check: should send
        scheduler.run_due_checks().await;
        let mut count = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, AgentMessage::CheckResult(_)) {
                count += 1;
            }
        }
        assert_eq!(count, 1, "First check should produce one result");

        // Wait to make it due again (generous buffer for slow CI runners)
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Second check with same exit code: should NOT send (delta dedup)
        scheduler.run_due_checks().await;
        let mut count2 = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, AgentMessage::CheckResult(_)) {
                count2 += 1;
            }
        }
        assert_eq!(
            count2, 0,
            "Same exit code should be suppressed by delta dedup"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_command_hash_dedup_shares_results() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        // Same check_cmd for both: command should execute only once
        scheduler
            .update_components(vec![
                make_config(id1, "echo shared_cmd", 1),
                make_config(id2, "echo shared_cmd", 1),
            ])
            .await;

        scheduler.run_due_checks().await;

        let mut results = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let AgentMessage::CheckResult(cr) = msg {
                results.push(cr);
            }
        }
        // Both components should get a result
        assert_eq!(results.len(), 2);
        // Both should have the same exit code and stdout
        assert_eq!(results[0].exit_code, results[1].exit_code);
        assert_eq!(results[0].stdout, results[1].stdout);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_staleness_forces_resend() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        let id1 = Uuid::new_v4();
        scheduler
            .update_components(vec![make_config(id1, "echo ok", 1)])
            .await;

        // First check
        scheduler.run_due_checks().await;
        while rx.try_recv().is_ok() {} // drain

        // Manually set last_sent_at to 6 minutes ago to simulate staleness
        {
            let mut state = scheduler.check_state.write().await;
            if let Some(cs) = state.get_mut(&id1) {
                cs.last_sent_at = Some(
                    chrono::Utc::now() - chrono::Duration::seconds(STALENESS_TIMEOUT_SECS + 1),
                );
                cs.last_checked_at = Some(chrono::Utc::now() - chrono::Duration::seconds(10));
            }
        }

        // Run again — should force-send despite same exit code
        scheduler.run_due_checks().await;
        let mut found = false;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, AgentMessage::CheckResult(_)) {
                found = true;
            }
        }
        assert!(
            found,
            "Staleness should force a resend even with same exit code"
        );
    }

    #[tokio::test]
    async fn test_no_checks_when_no_components() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        scheduler.run_due_checks().await;

        // No messages should be sent
        assert!(rx.try_recv().is_err(), "No components = no check results");
    }

    #[tokio::test]
    async fn test_components_without_check_cmd_skipped() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        let id1 = Uuid::new_v4();
        let mut config = make_config(id1, "echo ok", 30);
        config.check_cmd = None;

        scheduler.update_components(vec![config]).await;
        scheduler.run_due_checks().await;

        assert!(
            rx.try_recv().is_err(),
            "Components without check_cmd should be skipped"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_interval_respected_not_due_yet() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let scheduler = Arc::new(CheckScheduler::new(Uuid::new_v4(), tx));

        let id1 = Uuid::new_v4();
        // 60s interval
        scheduler
            .update_components(vec![make_config(id1, "echo ok", 60)])
            .await;

        // First check (always due)
        scheduler.run_due_checks().await;
        while rx.try_recv().is_ok() {} // drain

        // Immediately run again — should NOT be due (60s interval)
        scheduler.run_due_checks().await;
        let mut count = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, AgentMessage::CheckResult(_)) {
                count += 1;
            }
        }
        assert_eq!(count, 0, "Component should not be due yet (60s interval)");
    }

    // =========================================================================
    // Metrics extraction tests
    // =========================================================================

    #[test]
    fn test_extract_metrics_plain_json() {
        let stdout = r#"{"active_users": 12, "queue_depth": 150}"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["active_users"], 12);
        assert_eq!(m["queue_depth"], 150);
    }

    #[test]
    fn test_extract_metrics_json_with_whitespace() {
        let stdout = r#"
        {
            "users": ["Alice", "Bob", "Charlie"],
            "count": 3
        }
        "#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["count"], 3);
        assert!(m["users"].is_array());
    }

    #[test]
    fn test_extract_metrics_appcontrol_tags() {
        let stdout = r#"Some log output here
<appcontrol>{"state": "Started", "connections": 45}</appcontrol>
More output"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["state"], "Started");
        assert_eq!(m["connections"], 45);
    }

    #[test]
    fn test_extract_metrics_marker_format() {
        let stdout = r#"Process is running OK
Memory usage: 2.1GB
---METRICS---
{"memory_mb": 2150, "cpu_pct": 45.5, "status": "healthy"}"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["memory_mb"], 2150);
        assert_eq!(m["status"], "healthy");
    }

    #[test]
    fn test_extract_metrics_no_json() {
        let stdout = "Process nginx is running with PID 1234";
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_none());
    }

    #[test]
    fn test_extract_metrics_empty_stdout() {
        let metrics = extract_metrics_from_stdout("");
        assert!(metrics.is_none());
    }

    #[test]
    fn test_extract_metrics_nested_json() {
        let stdout = r#"{"cluster": {"nodes": 3, "master": "node-1"}, "shards": {"active": 15}}"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["cluster"]["nodes"], 3);
        assert_eq!(m["cluster"]["master"], "node-1");
    }

    #[test]
    fn test_extract_metrics_auto_detect_mixed_output() {
        // Simulates a script that outputs logs and JSON mixed together
        let stdout = r#"[2024-03-09 14:30:15] Checking service status...
[2024-03-09 14:30:15] Found 12 active connections
[2024-03-09 14:30:16] Memory usage: 2.1GB
{"widgets": [{"type": "number", "label": "Users", "value": 12}]}
[2024-03-09 14:30:16] Check completed successfully"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert!(m["widgets"].is_array());
        assert_eq!(m["widgets"][0]["value"], 12);
    }

    #[test]
    fn test_extract_metrics_auto_detect_json_at_end() {
        let stdout = r#"Service is running
CPU: 45%
Memory: 2.1GB
{"cpu": 45, "memory_gb": 2.1, "status": "healthy"}"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["cpu"], 45);
        assert_eq!(m["status"], "healthy");
    }

    #[test]
    fn test_extract_metrics_auto_detect_multiple_json_lines() {
        // If multiple JSON lines exist, take the last one
        let stdout = r#"{"intermediate": true}
Some log line
{"final": "result", "count": 42}"#;
        let metrics = extract_metrics_from_stdout(stdout);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert_eq!(m["final"], "result");
        assert_eq!(m["count"], 42);
    }
}
