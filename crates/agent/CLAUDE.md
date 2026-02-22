# CLAUDE.md - crates/agent

## Purpose
Single binary (~8MB static) deployed on managed machines. Executes health checks, integrity checks, and commands. Sends deltas only. Survives disconnection (offline buffer). **CRITICAL: processes started by the agent MUST survive agent crash via double-fork + setsid.**

## Dependencies (Cargo.toml)
```toml
[package]
name = "appcontrol-agent"
version = "0.1.0"
edition = "2021"

[dependencies]
appcontrol-common = { path = "../common" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tokio-tungstenite = { version = "0.21", features = ["rustls-tls-native-roots"] }
sysinfo = "0.30"
nix = { version = "0.28", features = ["process", "signal"] }
sled = "0.34"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
sha2 = "0.10"
clap = { version = "4", features = ["derive"] }
```

## Architecture
```
agent/src/
├── main.rs            # CLI, config load, start runtime
├── config.rs          # YAML config: gateway_url, tls, labels, agent_id
├── connection.rs      # WebSocket client → gateway (or direct → backend)
│                      # Auto-reconnect with exponential backoff (1s, 2s, 4s, max 60s)
├── executor.rs        # CRITICAL: process execution with detachment
├── scheduler.rs       # Local check scheduler (tokio::time::interval per component)
├── buffer.rs          # Offline sled DB buffer (~100MB FIFO), replay on reconnect
├── platform.rs        # gethostname() + get_ip_addresses() (FQDN + IP detection)
└── native_commands.rs # Built-in: disk_space, memory, cpu, process, tcp_port, http, load_average
```

## Agent Registration
On connect, the agent sends a Register message with:
- `agent_id`: UUID (auto-generated or configured)
- `hostname`: FQDN from gethostname()
- `ip_addresses`: all non-loopback IPs detected via sysinfo (IPv4 + IPv6)
- `labels`: key-value metadata (role, env, zone)
- `version`: agent binary version

The `ip_addresses` field supports cloud scenarios (Azure, AWS) where FQDN
may be auto-generated and unhelpful. Operators can identify agents by IP.

## CRITICAL: Process Detachment (executor.rs)

```rust
// MUST implement double-fork + setsid for ASYNC commands (start, stop, rebuild)
// SYNC commands (checks, diagnostics) run normally with timeout
//
// Algorithm:
// 1. fork() → child
// 2. In child: setsid() → new session
// 3. fork() again → grandchild
// 4. Intermediate child exits immediately
// 5. Grandchild: close ALL file descriptors, redirect to /dev/null or log
// 6. Grandchild: exec() the command
//
// Result: grandchild is reparented to init/PID 1
// Agent crash does NOT affect the grandchild

pub enum CommandMode {
    Sync { timeout: Duration },   // checks, diagnostics — agent waits for result
    Async,                        // start, stop, rebuild — agent returns job_id immediately
}
```

## Semi-Autonomous Mode
1. Agent receives **Snapshot** from backend: list of components, check commands, intervals
2. Agent schedules checks locally (no polling the server)
3. Agent sends **deltas only**: when exit_code changes, send immediately
4. Every 60s, agent sends a batch of metrics even if nothing changed (heartbeat)
5. Check deduplication: if multiple components share the same check_cmd, execute ONCE and share result (key: SHA-256 of command string)

## Offline Buffer (buffer.rs)
- Storage: sled embedded key-value DB
- Key: timestamp (nanosecond precision for ordering)
- Value: serialized AgentMessage
- Max size: 100MB. When full, FIFO (oldest entries evicted)
- On reconnect: replay all buffered messages in chronological order, then switch to real-time

## Configuration (agent.yaml)
```yaml
agent:
  id: auto  # or specific UUID
gateway:
  url: wss://gateway.company.com:443
  reconnect_interval_secs: 10
tls:
  enabled: true
  cert_file: /etc/appcontrol/certs/agent.crt
  key_file: /etc/appcontrol/certs/agent.key
  ca_file: /etc/appcontrol/certs/ca.crt
labels:
  role: database
  env: production
  zone: PRD
```

## Tests to Implement
- **Process detachment:** start a long-running process (sleep 3600), kill the agent, verify process still runs
- **Check scheduler:** configure 2 components with different intervals, verify checks execute at correct frequency
- **Offline buffer:** disconnect, execute 100 checks, reconnect, verify all 100 are replayed in order
- **Check dedup:** 3 components with same check_cmd, verify command executes only once
- **Native commands:** disk_space, memory, cpu return valid JSON with expected fields
- **Reconnection:** disconnect gateway, verify exponential backoff, reconnect, verify normal operation resumes
