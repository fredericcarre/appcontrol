# CLAUDE.md - crates/common

## Purpose
Shared types, protocol definitions, FSM logic, and mTLS utilities used by all other crates (agent, gateway, backend, cli).

## Dependencies (Cargo.toml)
```toml
[package]
name = "appcontrol-common"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
strum = { version = "0.26", features = ["derive"] }
```

## Public API

### types.rs
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display)]
pub enum ComponentState {
    Unknown, Running, Degraded, Failed, Stopped, Starting, Stopping, Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PermissionLevel {
    None = 0, View = 1, Operate = 2, Edit = 3, Manage = 4, Owner = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticRecommendation {
    Healthy, Restart, AppRebuild, InfraRebuild, Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub component_id: Uuid,
    pub check_type: CheckType, // Health, Integrity, PostStart, Infrastructure
    pub exit_code: i32,
    pub stdout: Option<String>, // truncated to 4KB by agent
    pub duration_ms: u32,
    pub at: DateTime<Utc>,
}
```

### fsm.rs
```rust
/// Returns true if transitioning from `from` to `to` is valid.
pub fn is_valid_transition(from: ComponentState, to: ComponentState) -> bool;

/// Given a check result, determine the new state (if any transition needed).
pub fn next_state_from_check(current: ComponentState, exit_code: i32) -> Option<ComponentState>;
```

Valid transitions — implement EXACTLY these:
- Unknown → Running, Stopped, Failed (first check received)
- Stopped → Starting (start command)
- Starting → Running (check OK), Failed (timeout or check KO)
- Running → Degraded (exit 1), Failed (exit ≥ 2), Stopping (stop command)
- Degraded → Running (exit 0), Failed (exit ≥ 2), Stopping (stop command)
- Stopping → Stopped (stop confirmed)
- Failed → Starting (retry), Stopping (cleanup)
- Any → Unreachable (heartbeat timeout)
- Unreachable → previous state (agent reconnects)

### protocol.rs
```rust
// Agent → Backend
pub enum AgentMessage {
    Heartbeat { agent_id: Uuid, cpu: f32, memory: f32, at: DateTime<Utc> },
    CheckResult(CheckResult),
    CommandResult { request_id: Uuid, exit_code: i32, stdout: String, stderr: String, duration_ms: u32 },
    Register { agent_id: Uuid, hostname: String, ip_addresses: Vec<String>, labels: Value, version: String },
    // ip_addresses: detected non-loopback IPs (FQDN + IP support for Azure/cloud)
    // serde(default) ensures backward compat with older agents that don't send ip_addresses
}

// Backend → Agent
pub enum BackendMessage {
    ExecuteCommand { request_id: Uuid, component_id: Uuid, command: String, timeout_seconds: u32 },
    UpdateConfig { components: Vec<ComponentConfig> },
    Ack { request_id: Uuid },
}
```

## Tests to Implement
- All 14 valid FSM transitions return true
- At least 10 invalid transitions return false (e.g., Running→Starting, Stopped→Running)
- next_state_from_check: exit 0 from Starting → Running, exit 1 from Running → Degraded, exit 2 from Running → Failed
- Serialize/deserialize roundtrip for all AgentMessage and BackendMessage variants
- PermissionLevel ordering: None < View < Operate < Edit < Manage < Owner
