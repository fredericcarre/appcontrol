# Agent Installation Guide

Deploy AppControl agents on Linux, macOS, and Windows servers. This guide covers
binary installation, mTLS enrollment with automatic certificate provisioning, and
running the agent as a system service.

## Architecture overview

```
                   +-----------+
                   |  Backend  |   (API + PKI authority)
                   +-----+-----+
                         |
                   +-----+-----+
                   |  Gateway   |   (WebSocket relay + enrollment proxy)
                   +-----+-----+
                    /    |    \
               +---+  +---+  +---+
               | A1|  | A2|  | A3|   Agents (mTLS WebSocket)
               +---+  +---+  +---+
```

Agents connect to the gateway over **mTLS WebSocket**. The gateway relays
commands from the backend and forwards agent health reports.

**Enrollment** provisions mTLS certificates automatically: the agent sends a
one-time token to the gateway, the backend validates it, generates a
certificate signed by the organization's CA, and returns it. Zero PKI
expertise required.

---

## 1. Prerequisites

| Requirement | Details |
|-------------|---------|
| AppControl backend | Running, with at least one organization created |
| AppControl gateway | Running, reachable from the agent host |
| Network | Agent must reach the gateway on port **4443** (TCP, outbound) |
| OS | Linux (x86_64, aarch64), macOS (x86_64, arm64), Windows (x86_64, arm64) |

---

## 2. Download the agent binary

### From a GitHub release

```bash
# Linux (amd64)
gh release download --repo fredericcarre/appcontrol --pattern 'appcontrol-agent-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appcontrol-agent-linux-amd64
mv /usr/local/bin/appcontrol-agent-linux-amd64 /usr/local/bin/appcontrol-agent

# Linux (arm64)
gh release download --repo fredericcarre/appcontrol --pattern 'appcontrol-agent-linux-arm64' --dir /usr/local/bin
chmod +x /usr/local/bin/appcontrol-agent-linux-arm64
mv /usr/local/bin/appcontrol-agent-linux-arm64 /usr/local/bin/appcontrol-agent

# macOS (Apple Silicon)
gh release download --repo fredericcarre/appcontrol --pattern 'appcontrol-agent-darwin-arm64' --dir /usr/local/bin
chmod +x /usr/local/bin/appcontrol-agent-darwin-arm64
mv /usr/local/bin/appcontrol-agent-darwin-arm64 /usr/local/bin/appcontrol-agent
```

### Windows (PowerShell, run as Administrator)

```powershell
# Create install directory
New-Item -ItemType Directory -Force -Path "$env:ProgramFiles\AppControl"

# Download
gh release download --repo fredericcarre/appcontrol --pattern 'appcontrol-agent-windows-amd64.exe' --dir "$env:ProgramFiles\AppControl"
Rename-Item "$env:ProgramFiles\AppControl\appcontrol-agent-windows-amd64.exe" "appcontrol-agent.exe"

# Add to PATH (optional)
[Environment]::SetEnvironmentVariable("PATH", "$env:PATH;$env:ProgramFiles\AppControl", "Machine")
```

### Docker (Linux only)

```bash
docker pull ghcr.io/fredericcarre/appcontrol-agent:latest
```

---

## 3. Enrollment (automatic certificate provisioning)

Enrollment is a **one-command** process: the agent contacts the gateway with a
token, receives its mTLS certificate, writes everything to disk, and is ready
to start.

### 3.1 Create an enrollment token

Tokens are created by an administrator. Three options:

**Option A: Web UI**

1. Go to **Settings > Enrollment** in the AppControl web UI.
2. Click **Create Token**.
3. Set a name, scope (`agent` or `gateway`), optional max uses and expiry.
4. Copy the token (shown once, starts with `ac_enroll_`).

**Option B: CLI**

```bash
appctl pki create-token --name "deploy-prod" --max-uses 50 --scope agent
# Output: ac_enroll_7f3a2b...
```

**Option C: API**

```bash
curl -X POST https://backend:3000/api/v1/enrollment/tokens \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{"name": "deploy-prod", "max_uses": 50, "scope": "agent"}'
```

### 3.2 Enroll the agent

#### Linux / macOS

```bash
sudo appcontrol-agent --enroll https://gateway:4443 --token ac_enroll_7f3a2b...
```

This writes:
```
/etc/appcontrol/
  tls/
    agent.crt         # Agent certificate (signed by org CA)
    agent.key         # Private key (mode 0600)
    ca.crt            # Organization CA certificate
  agent.yaml          # Auto-generated config
```

#### Windows (PowerShell, run as Administrator)

```powershell
& "$env:ProgramFiles\AppControl\appcontrol-agent.exe" --enroll https://gateway:4443 --token ac_enroll_7f3a2b...
```

This writes:
```
C:\ProgramData\AppControl\config\
  tls\
    agent.crt         # Agent certificate (signed by org CA)
    agent.key         # Private key (read-only)
    ca.crt            # Organization CA certificate
  agent.yaml          # Auto-generated config
```

#### Custom install directory

```bash
appcontrol-agent --enroll https://gateway:4443 --token ac_enroll_7f3a2b... --enroll-dir /opt/appcontrol
```

### 3.3 Verify enrollment

After enrollment, the output shows:

```
  Agent Enrollment Successful
  ===========================

  Agent ID:    a3f1b7c8-...
  Hostname:    server01.prod
  Fingerprint: 7a2b3c4d...

  cert:   /etc/appcontrol/tls/agent.crt
  key:    /etc/appcontrol/tls/agent.key
  ca:     /etc/appcontrol/tls/ca.crt
  config: /etc/appcontrol/agent.yaml
```

---

## 4. Start the agent

### 4.1 Foreground (testing)

```bash
# Linux/macOS
appcontrol-agent --config /etc/appcontrol/agent.yaml

# Windows
appcontrol-agent.exe --config "C:\ProgramData\AppControl\config\agent.yaml"
```

### 4.2 Systemd service (Linux)

Create `/etc/systemd/system/appcontrol-agent.service`:

```ini
[Unit]
Description=AppControl Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/appcontrol-agent --config /etc/appcontrol/agent.yaml
Restart=always
RestartSec=10
User=root
LimitNOFILE=65536

# Security hardening
ProtectSystem=strict
ReadWritePaths=/var/lib/appcontrol /etc/appcontrol
PrivateTmp=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

Then enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable appcontrol-agent
sudo systemctl start appcontrol-agent

# Check status
sudo systemctl status appcontrol-agent
sudo journalctl -u appcontrol-agent -f
```

**Configuration reload** (without restart):
```bash
sudo systemctl reload appcontrol-agent   # Sends SIGHUP
```

### 4.3 Windows service

Install and start the service (run as **Administrator**):

```powershell
# Install
appcontrol-agent.exe service install --config "C:\ProgramData\AppControl\config\agent.yaml"

# Start
sc.exe start AppControlAgent

# Check status
sc.exe query AppControlAgent

# View logs (Event Viewer > Windows Logs > Application)
Get-EventLog -LogName Application -Source AppControlAgent -Newest 20
```

Manage the service:
```powershell
# Stop
sc.exe stop AppControlAgent

# Restart
Restart-Service AppControlAgent

# Set to auto-start (already the default)
sc.exe config AppControlAgent start=auto

# Uninstall
sc.exe stop AppControlAgent
appcontrol-agent.exe service uninstall
```

The service runs as **LocalSystem** by default. To use a dedicated service account:
```powershell
sc.exe config AppControlAgent obj="DOMAIN\svc_appcontrol" password="..."
```

### 4.4 Docker

```bash
docker run -d \
  --name appcontrol-agent \
  --restart unless-stopped \
  -v /etc/appcontrol:/etc/appcontrol:ro \
  -v /var/lib/appcontrol:/var/lib/appcontrol \
  ghcr.io/fredericcarre/appcontrol-agent:latest \
  --config /etc/appcontrol/agent.yaml
```

---

## 5. Configuration reference

The agent configuration file (`agent.yaml`) is auto-generated during
enrollment. You can customize it afterward.

```yaml
# Agent identity
agent:
  id: "auto"                    # UUID or "auto" (deterministic from hostname)

# Gateway connection
gateway:
  url: "wss://gateway:4443/ws"  # Single gateway
  # urls:                       # Multiple gateways (failover)
  #   - "wss://gw1:4443/ws"
  #   - "wss://gw2:4443/ws"
  failover_strategy: "ordered"  # "ordered" or "round-robin"
  primary_retry_secs: 300       # How often to try primary gateway
  reconnect_interval_secs: 10   # Reconnect delay

# mTLS certificates
tls:
  enabled: true
  cert_file: "/etc/appcontrol/tls/agent.crt"
  key_file: "/etc/appcontrol/tls/agent.key"
  ca_file: "/etc/appcontrol/tls/ca.crt"

# Labels for agent grouping and filtering
labels:
  environment: "production"
  datacenter: "eu-west-1"
  role: "webserver"

# Log level
log_level: "appcontrol_agent=info"
```

### Gateway failover

For high availability, configure multiple gateway URLs. The agent automatically
fails over to the next gateway if the current one becomes unreachable.

```yaml
gateway:
  urls:
    - "wss://gw-primary.prod:4443/ws"
    - "wss://gw-secondary.prod:4443/ws"
  failover_strategy: "ordered"   # "ordered" or "round-robin"
  primary_retry_secs: 300        # Retry primary every 5 minutes
  reconnect_interval_secs: 10    # Delay between reconnection attempts
```

| Strategy | Behavior |
|----------|----------|
| `ordered` | Try URLs in sequence, always prefer the first. If the primary fails, connect to the next. Periodically retry the primary every `primary_retry_secs`. |
| `round-robin` | Distribute connections across all gateways. Useful for load balancing when multiple gateways serve the same zone. |

Gateways are **stateless WebSocket relays** — they hold no persistent state
about agents. Any gateway can serve any agent, and agents can switch gateways
transparently. Administrators can suspend a gateway for maintenance by setting
`is_active = false` in the UI or API; connected agents will failover
automatically.

### Environment variable overrides

| Variable | Overrides | Example |
|----------|-----------|---------|
| `AGENT_ID` | `agent.id` | `AGENT_ID=auto` |
| `GATEWAY_URL` | `gateway.url` | `GATEWAY_URL=wss://gw:4443/ws` |
| `GATEWAY_URLS` | `gateway.urls` | `GATEWAY_URLS=wss://gw1:4443/ws,wss://gw2:4443/ws` |
| `GATEWAY_RECONNECT_SECS` | `gateway.reconnect_interval_secs` | `GATEWAY_RECONNECT_SECS=30` |
| `TLS_ENABLED` | `tls.enabled` | `TLS_ENABLED=true` |
| `TLS_CERT_FILE` | `tls.cert_file` | `TLS_CERT_FILE=/path/to/agent.crt` |
| `TLS_KEY_FILE` | `tls.key_file` | `TLS_KEY_FILE=/path/to/agent.key` |
| `TLS_CA_FILE` | `tls.ca_file` | `TLS_CA_FILE=/path/to/ca.crt` |

---

## 6. Gateway enrollment

Gateways can also be enrolled with mTLS certificates. The process is similar
but uses the `gateway` scope which generates a **server certificate** with
Subject Alternative Names (SANs).

### 6.1 Create a gateway enrollment token

```bash
appctl pki create-token --name "gateway-prod" --max-uses 1 --scope gateway
```

### 6.2 Enroll the gateway

The gateway enrollment is done via the API (gateway doesn't have an `--enroll`
CLI flag since it proxies to itself):

```bash
curl -k -X POST https://gateway:4443/enroll \
  -H "Content-Type: application/json" \
  -d '{
    "token": "ac_enroll_...",
    "hostname": "gateway.prod.example.com",
    "san_dns": ["gw.prod.example.com", "gateway.internal"],
    "san_ips": ["10.0.1.5", "127.0.0.1"]
  }'
```

The response includes `cert_pem`, `key_pem`, and `ca_pem`. Save them to the
gateway's TLS configuration directory.

### 6.3 Local certificate issuance (offline)

For air-gapped environments, use the CLI to issue certificates locally:

```bash
# Initialize CA (if not already done)
appctl pki init --org-name "My Corp"

# Issue gateway certificate with SANs
appctl pki issue-gateway \
  --cn "gateway.prod.example.com" \
  --san-dns "gw.prod.example.com,localhost" \
  --san-ips "10.0.1.5,127.0.0.1" \
  --ca-cert /path/to/ca.crt \
  --ca-key /path/to/ca.key \
  --out-dir /etc/appcontrol-gateway/tls

# Issue agent certificate
appctl pki issue-agent \
  --hostname "server01.prod" \
  --ca-cert /path/to/ca.crt \
  --ca-key /path/to/ca.key \
  --out-dir /etc/appcontrol/tls
```

---

## 7. PKI overview

AppControl uses a **per-organization CA** for mTLS:

```
  Organization CA (self-signed, stored in database)
      |
      +-- Gateway cert (server, with SANs)
      |
      +-- Agent cert (client, CN=hostname)
      +-- Agent cert ...
      +-- Agent cert ...
```

### Initialize PKI

Before any enrollment, the organization's CA must be initialized (once):

```bash
# Via CLI
appctl pki init --org-name "My Company" --validity-days 3650

# Via API
curl -X POST https://backend:3000/api/v1/pki/init \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{"org_name": "My Company", "validity_days": 3650}'

# Via web UI: Settings > Enrollment > Initialize PKI
```

### Token security

- Tokens start with `ac_enroll_` for easy identification.
- The backend stores only the **SHA-256 hash** of the token. The plaintext
  is returned once at creation and never stored.
- Tokens can have **max uses** (e.g., 50) and **expiry** (e.g., 48 hours).
- Tokens can be **revoked** at any time.
- All enrollment attempts (success and failure) are logged in the
  **enrollment audit trail**.

### Certificate details

| Property | Agent cert | Gateway cert |
|----------|-----------|--------------|
| Type | Client (mTLS) | Server (TLS) |
| CN | hostname | gateway FQDN |
| SANs | none | DNS + IP SANs |
| Validity | 365 days | 365 days |
| Key | RSA 2048 / ECDSA P-256 | RSA 2048 / ECDSA P-256 |

---

## 8. Troubleshooting

### Agent won't connect

```bash
# Check the agent is running
systemctl status appcontrol-agent     # Linux
sc.exe query AppControlAgent          # Windows

# Check logs
journalctl -u appcontrol-agent -f     # Linux
Get-EventLog -LogName Application -Source AppControlAgent -Newest 50  # Windows

# Test network connectivity
curl -v https://gateway:4443/healthz
```

### Enrollment fails

| Error | Cause | Fix |
|-------|-------|-----|
| `HTTP 401` | Token invalid or revoked | Create a new token |
| `HTTP 409` | Token max uses exhausted | Create a new token with higher max_uses |
| `HTTP 500` | PKI not initialized | Run `appctl pki init` first |
| `Connection refused` | Gateway unreachable | Check firewall, port 4443 |
| `Certificate verify failed` | Wrong CA or expired cert | Re-enroll the agent |

### Certificate renewal

Certificates expire after 365 days (default). To renew:

1. Create a new enrollment token.
2. Stop the agent.
3. Re-enroll: `appcontrol-agent --enroll https://gateway:4443 --token <new-token>`
4. Restart the agent.

---

## 9. Platform-specific notes

### Linux

| Item | Path |
|------|------|
| Config directory | `/etc/appcontrol/` |
| Data directory | `/var/lib/appcontrol/` |
| TLS certificates | `/etc/appcontrol/tls/` |
| Config file | `/etc/appcontrol/agent.yaml` |
| Service | `systemctl {start,stop,status,reload} appcontrol-agent` |
| Reload config | `kill -HUP $(pidof appcontrol-agent)` |

### Windows

| Item | Path |
|------|------|
| Install directory | `C:\Program Files\AppControl\` |
| Config directory | `C:\ProgramData\AppControl\config\` |
| Data directory | `C:\ProgramData\AppControl\` |
| TLS certificates | `C:\ProgramData\AppControl\config\tls\` |
| Config file | `C:\ProgramData\AppControl\config\agent.yaml` |
| Service | `sc.exe {start,stop,query} AppControlAgent` |
| Service install | `appcontrol-agent.exe service install --config "..."` |
| Service uninstall | `appcontrol-agent.exe service uninstall` |

### macOS

Same as Linux, but without systemd. Use launchd instead:

```bash
# Create /Library/LaunchDaemons/com.appcontrol.agent.plist
sudo launchctl load /Library/LaunchDaemons/com.appcontrol.agent.plist
sudo launchctl start com.appcontrol.agent
```

---

## 10. Agent identity & resilience

### How the agent ID works

By default, the agent generates a **deterministic UUID** from the hostname:

```
UUID = Uuid::new_v5(NAMESPACE_DNS, hostname)
```

The same hostname always produces the same UUID across restarts and
re-enrollments. This means:

- Restarting the agent preserves its identity without any configuration.
- Re-enrolling on the same host upserts the existing agent record in the
  backend database (no duplicate).
- The agent ID is predictable and reproducible.

The ID can be overridden (highest priority first):

| Method | Example |
|--------|---------|
| CLI flag | `appcontrol-agent --agent-id 550e8400-...` |
| Environment variable | `AGENT_ID=550e8400-...` |
| Config file | `agent: { id: "550e8400-..." }` in `agent.yaml` |
| Auto (default) | `agent: { id: "auto" }` — deterministic from hostname |

### Multi-agent on the same host

Running multiple agents on the same host is **not supported with the default
`id: "auto"` setting** — both agents would produce the same UUID and conflict
in the backend.

To run multiple agents on one host, assign each agent an **explicit unique
UUID** via `--agent-id`, `AGENT_ID`, or the config file. Each agent must also
use a separate config directory and TLS certificate set.

In **container environments**, each container has its own hostname, so
`id: "auto"` works without collision. No special configuration is needed.

### Resilience model

Agent-level high availability (active/passive on the same host) is
**unnecessary by design**:

1. **Process detachment.** Processes started by the agent use double-fork +
   setsid. They survive agent crashes — a dead agent does not kill monitored
   processes.
2. **Automatic restart.** systemd restarts the agent within ~10 seconds
   (`Restart=always`, `RestartSec=10`). The agent is typically back before
   the heartbeat timeout (default 180 seconds) triggers.
3. **Seamless reconnection.** On restart, the agent re-registers with the same
   UUID and mTLS certificate. The backend updates its metadata (version, IPs,
   heartbeat) — no re-enrollment needed.
4. **No state loss.** The agent uses an on-disk offline buffer. Messages
   queued during disconnection are replayed on reconnection.

The backend marks an agent `UNREACHABLE` only after `heartbeat_timeout_seconds`
(default 180s) of silence. With systemd auto-restart, the agent recovers well
before this threshold.

### Reconnection behavior

When an agent reconnects (after restart, upgrade, or network recovery):

1. The agent opens a WebSocket connection to the gateway (with mTLS).
2. The agent sends a `Register` message containing: `agent_id`, `hostname`,
   `ip_addresses`, `labels`, `version`, `cert_fingerprint`.
3. The gateway forwards to the backend.
4. The backend verifies the certificate fingerprint against the stored value
   in `agents.certificate_fingerprint` (anti-spoofing).
5. The backend updates: `version`, `ip_addresses`, `last_heartbeat_at`,
   `is_active = true`.
6. The backend pushes the current component configuration to the agent.

IP addresses are auto-detected: all non-loopback interfaces (IPv4 + IPv6).
This is useful in cloud environments where hostnames may be ephemeral but
IP addresses are stable.

---

## 11. Version management

### Version string format

Agent and gateway binaries embed a version string at build time:

```
<cargo_version> (<git_hash> <build_time>)
```

Example: `0.2.0 (a3f1b7c 2026-02-28T14:32:15Z)`

| Component | Source |
|-----------|--------|
| `cargo_version` | `version` field in `Cargo.toml` (workspace-level) |
| `git_hash` | Short commit hash, injected by `build.rs` at compile time |
| `build_time` | UTC timestamp, injected by `build.rs` at compile time |

Display the version of a running binary:

```bash
appcontrol-agent --version
# → appcontrol-agent 0.2.0 (a3f1b7c 2026-02-28T14:32:15Z)
```

### Version reporting

Agents and gateways report their version in the `Register` message sent on
every connection. The backend stores it:

- `agents.version` — updated on every agent connection
- `gateways.version` — updated on every gateway connection

Query agent versions:

```bash
# Via API
curl https://backend:3000/api/v1/agents \
  -H "Authorization: Bearer $JWT"

# Via CLI
appctl agents list
```

### Release cycle

Releases follow GitHub tags. The version is **automatically injected from the
tag** — no manual version bump in `Cargo.toml` is needed:

1. Create and push a git tag: `git tag v0.3.0 && git push origin v0.3.0`
2. GitHub Actions `release.yaml` triggers automatically:
   - **Injects the version** from the tag into the workspace `Cargo.toml`
     (e.g., `v0.3.0` → `version = "0.3.0"`). All crates inherit this version
     via `version.workspace = true`.
   - Builds multi-platform binaries (Linux amd64/arm64, macOS x86_64/arm64,
     Windows amd64/arm64) — each binary reports the correct version
   - Publishes Docker images tagged with the version — the binary inside the
     image also reports the correct version
   - Packages the Helm chart with the matching version
   - Creates a GitHub Release with binary artifacts and SHA-256 checksums

This ensures that `appcontrol-agent --version` always matches the release tag
and the Docker image tag. The backend can compare agent-reported versions
against the latest available version to detect outdated agents.

Pin a Docker Compose deployment to a specific version:

```bash
APPCONTROL_VERSION=0.3.0 docker compose -f docker/docker-compose.release.yaml up -d
```

---

## 12. Upgrading agents

Three upgrade paths are available, from most to least recommended.

### 12.1 Managed update via API (recommended)

The backend can push binary updates to connected agents, with integrity
verification, progress tracking, and automatic rollback. This works even in
air-gapped networks since the binary is transferred over the existing
WebSocket connection.

**Step 1: Upload the new binary to the backend**

```bash
curl -X POST https://backend:3000/api/v1/admin/agent-binaries \
  -H "Authorization: Bearer $JWT" \
  -F "binary=@appcontrol-agent-linux-amd64" \
  -F "version=0.3.0" \
  -F "platform=linux-amd64"
```

**Step 2: Push the update to agents**

```bash
# Single agent
curl -X POST https://backend:3000/api/v1/admin/agents/<agent-id>/update \
  -H "Authorization: Bearer $JWT"

# Batch update
curl -X POST https://backend:3000/api/v1/admin/agents/update-batch \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{"agent_ids": ["<id1>", "<id2>"], "version": "0.3.0"}'
```

**How it works:**

1. The backend splits the binary into **256 KB chunks** and sends them via
   WebSocket (`UpdateBinaryChunk` messages).
2. The agent accumulates chunks in memory and sends `UpdateProgress` messages
   back (status: `Downloading` → `Verifying` → `Applying` → `Complete`).
3. Once all chunks are received, the agent writes the assembled binary to a
   temp file and verifies its **SHA-256 checksum**.
4. **Atomic swap**: the current binary is renamed to `.old`, the new binary
   takes its place.
5. **Re-exec**: on Unix the agent calls `exec()` to replace itself in-place,
   preserving CLI arguments. On Windows a new process is spawned.
6. **Automatic rollback**: if the swap fails, the `.old` binary is restored.

Update progress is tracked in the `agent_update_tasks` table (status:
`pending` → `in_progress` → `complete` / `failed`).

### 12.2 Direct download

For agents with internet access, the backend can instruct the agent to
download a binary from a URL:

- The agent downloads the binary, verifies the SHA-256 checksum, and performs
  the same atomic swap + re-exec.
- Same rollback behavior as the managed update.

### 12.3 Manual binary replacement

Replace the binary on disk and restart the service:

```bash
# Linux
sudo systemctl stop appcontrol-agent
sudo cp appcontrol-agent-0.3.0 /usr/local/bin/appcontrol-agent
sudo chmod +x /usr/local/bin/appcontrol-agent
sudo systemctl start appcontrol-agent
```

```powershell
# Windows
sc.exe stop AppControlAgent
Copy-Item appcontrol-agent-0.3.0.exe "$env:ProgramFiles\AppControl\appcontrol-agent.exe"
sc.exe start AppControlAgent
```

The agent's **identity is preserved** — certificates and config remain on disk.
On reconnection the backend updates the `version` field automatically.

**Trade-offs**: no integrity verification, no automatic rollback, no
centralized progress tracking. Use this only when the managed update path is
not available.

### Recommended upgrade strategy

1. **Upgrade the backend first** (Helm rolling update), then frontend, then
   gateways, then agents.
2. **Test on a subset**: upgrade 2-3 agents, verify health checks resume and
   the correct version appears in the Agent Management UI.
3. **Batch the rest**: use the batch update API to upgrade remaining agents.
4. **Monitor**: check `agent_update_tasks` status and agent versions via the
   API or UI.

---

## 13. Quick reference

### End-to-end: from zero to monitoring in 3 commands

```bash
# 1. Initialize PKI (once, by admin)
appctl pki init --org-name "My Company"

# 2. Create an enrollment token (by admin)
appctl pki create-token --name "deploy-prod" --max-uses 100 --scope agent

# 3. Enroll the agent (on each server)
appcontrol-agent --enroll https://gateway:4443 --token ac_enroll_...
```

The agent is now enrolled with a valid mTLS certificate. Start it:

```bash
# Linux
sudo systemctl start appcontrol-agent

# Windows
sc.exe start AppControlAgent
```
