# AppControl Architecture

## Network Topology

```
                        +------------------+
                        |    Browser /     |
                        |    appctl CLI    |
                        +--------+---------+
                                 |
                                 | HTTPS :443 (nginx)
                                 | or HTTP :3000 (direct API)
                                 v
                   +----------------------------+
                   |     nginx (TLS termination)|
                   |        :80/:443            |
                   |                            |
                   |  /api/*  --> backend:3000  |
                   |  /ws     --> backend:3000  |
                   |  /*      --> frontend:8080 |
                   +-------------+--------------+
                                 |
                    HTTP/WS (internal network)
                                 |
         +-----------------------+-----------------------+
         |                                               |
         v                                               v
+------------------+                          +-------------------+
|    Backend API   |                          |    PostgreSQL 16  |
|    :3000         |                          |    :5432          |
|                  +------- sqlx pool ------->+                   |
|  REST API        |                          +-------------------+
|  WebSocket /ws   |
|  (JWT auth)      |
+--------+---------+
         |
         | WebSocket /ws/gateway
         | (internal, gateway connects here)
         |
+--------+---------+       +------------------+
|    Gateway PRD   |       |    Gateway DR    |
|    :4443         |       |    :4443         |
|                  |       |                  |
| Directly exposed |       | Directly exposed |
| to agents (mTLS) |       | to agents (mTLS) |
+---+-----------+--+       +---+-----------+--+
    |           |              |           |
    | WSS :4443 | WSS :4443    | WSS :4443 | WSS :4443
    |  (mTLS)   |  (mTLS)      |  (mTLS)   |  (mTLS)
    v           v              v           v
+--------+ +--------+      +--------+ +--------+
| Agent  | | Agent  |      | Agent  | | Agent  |
| srv-01 | | srv-02 |      | dr-01  | | dr-02  |
+--------+ +--------+      +--------+ +--------+

Note: Agents connect DIRECTLY to gateways, NOT through nginx.
      Each gateway is exposed on its own IP/DNS.
```

## Component Roles

| Component | Role | Inbound Port | Config Source |
|-----------|------|-------------|---------------|
| **nginx** | TLS termination for web UI | `:80/:443` | `nginx.conf` |
| **Frontend** | React SPA | `:8080` (internal) | Build-time env |
| **Backend** | REST API + WebSocket hub + FSM engine | `:3000` (internal) | Env vars only |
| **Gateway** | Agent connection multiplexer (one per zone) | `:4443` (exposed directly) | YAML + env var overrides |
| **Agent** | Local process manager + health checker | None (client only) | YAML + env var overrides |
| **CLI** | CLI client for automation/schedulers | None (client only) | Env vars + CLI args |
| **PostgreSQL** | Persistent storage | `:5432` | Standard |

**Important:** Gateways are exposed directly on their own IP/port (e.g., `gateway-prd.company.com:4443`).
They do NOT go through nginx. Each zone (PRD, DR, DMZ) has its own gateway.

## Communication Protocols

```
Browser ---HTTPS---> nginx:443 ---HTTP---> backend:3000
                                     (proxy_pass /api/* and /ws)
                               ---HTTP---> frontend:8080
                                     (proxy_pass /* for SPA)

appctl  ---HTTP(S)---> backend:3000
                       (direct API calls, no nginx needed)

Gateway ---WebSocket---> backend:3000/ws/gateway
          (internal, persistent connection, reconnect on failure)

Agent ---WSS (mTLS)---> gateway:4443/ws
         (DIRECT connection, not through nginx)
         (outbound only, reconnect with exponential backoff 1s..60s)
```

## Connection Details

### Agent --> Gateway (WSS, mTLS)
- Agent initiates outbound WebSocket connection to gateway
- No inbound port needed on the agent (firewall-friendly)
- mTLS: agent presents its certificate, gateway verifies it
- WSS on port 443/4443 passes through most corporate firewalls
  (indistinguishable from HTTPS traffic)
- Automatic reconnect with exponential backoff (1s, 2s, 4s... max 60s)
- Offline buffer: agent stores messages locally if gateway is unreachable

### Gateway --> Backend (WebSocket)
- Gateway connects to backend WebSocket endpoint
- Routes messages bidirectionally: agents <--> backend
- Reconnect on disconnect with configurable interval
- In-memory message buffer during backend unavailability

### Frontend --> Backend (via nginx)
- nginx proxies `/api/*` to `backend:3000/api/`
- nginx proxies `/ws` to `backend:3000/ws` (WebSocket upgrade)
- All other routes serve the React SPA (client-side routing)

### CLI --> Backend (HTTP)
- Direct HTTP calls to backend REST API
- No nginx/frontend needed
- Configured via `APPCONTROL_URL` env var or `--url` CLI arg
- Exit codes designed for scheduler integration (0=OK, 1=Fail, 2=Timeout)

## Environment Variables

### Backend
| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3000` | HTTP listen port |
| `DATABASE_URL` | `postgresql://appcontrol:appcontrol@localhost:5432/appcontrol` | PostgreSQL connection string |
| `JWT_SECRET` | `dev-secret-change-in-production` | JWT signing secret (**required** in production) |
| `JWT_ISSUER` | `appcontrol` | JWT issuer claim |
| `APP_ENV` | `development` | Environment: `development`, `staging`, `production` |
| `CORS_ORIGINS` | _(none)_ | Comma-separated allowed origins |
| `LOG_FORMAT` | `text` | `text` or `json` (structured logging) |
| `DB_POOL_SIZE` | `20` | Database connection pool max connections |
| `DB_IDLE_TIMEOUT_SECS` | `600` | Idle connection timeout (seconds) |
| `DB_CONNECT_TIMEOUT_SECS` | `30` | Connection acquisition timeout (seconds) |
| `SHUTDOWN_TIMEOUT_SECS` | `30` | Graceful shutdown timeout (seconds) |
| `RATE_LIMIT_AUTH` | `10` | Auth endpoint rate limit (per IP per minute) |
| `RATE_LIMIT_OPERATIONS` | `5` | Operation endpoint rate limit (per user per minute) |
| `RATE_LIMIT_READS` | `200` | Read endpoint rate limit (per user per minute) |
| `RETENTION_ACTION_LOG_DAYS` | `0` | Days to keep action_log entries (0 = unlimited) |
| `RETENTION_CHECK_EVENTS_DAYS` | `0` | Days to keep check_events partitions (0 = unlimited) |

### Gateway
| Variable | Default | Description |
|----------|---------|-------------|
| `LISTEN_ADDR` | `0.0.0.0` | Bind address |
| `LISTEN_PORT` | `4443` | Bind port for agent connections |
| `BACKEND_URL` | `ws://localhost:3000/ws` | Backend WebSocket URL |
| `GATEWAY_ID` | `gateway-01` | Gateway identifier |
| `GATEWAY_SITE_ID` | (none) | UUID of the site this gateway belongs to. Optional: gateways without a site appear as "Unassigned" in the UI and can be assigned later. |
| `GATEWAY_ZONE` | (deprecated) | Legacy zone label. Use `GATEWAY_SITE_ID` instead. |
| `BACKEND_RECONNECT_SECS` | `5` | Reconnect interval to backend |

### Agent
| Variable | Default | Description |
|----------|---------|-------------|
| `GATEWAY_URL` | `ws://localhost:4443/ws` | Gateway WebSocket URL |
| `AGENT_ID` | `auto` (hostname-based UUID) | Agent identifier |
| `GATEWAY_RECONNECT_SECS` | `10` | Reconnect interval |
| `TLS_ENABLED` | `false` | Enable mTLS |
| `TLS_CERT_FILE` | - | Agent certificate path |
| `TLS_KEY_FILE` | - | Agent private key path |
| `TLS_CA_FILE` | - | CA certificate path |

### CLI
| Variable | Default | Description |
|----------|---------|-------------|
| `APPCONTROL_URL` | `http://localhost:3000` | Backend API URL |
| `APPCONTROL_API_KEY` | - | API key for authentication |

## Docker Compose Ports

| Service | Internal Port | External Port | Purpose |
|---------|--------------|---------------|---------|
| postgres | 5432 | 5432 | Database |
| backend | 3000 | 3000 | REST API + WebSocket |
| frontend | 8080 | 8080 | Web UI |
| gateway | 4443 | 4443 | Agent connections |

## Multi-Site (DR) Topology

```
                    +-----------------------+
                    |    Central Backend    |
                    |    + PostgreSQL       |
                    +-----------+-----------+
                                |
                +---------------+---------------+
                |                               |
    +-----------+-----------+       +-----------+-----------+
    |   Gateway PRD         |       |   Gateway DR          |
    |   zone: PRD           |       |   zone: DR            |
    |   :4443               |       |   :4443               |
    +---+-------+-------+--+       +---+-------+-------+---+
        |       |       |              |       |       |
     +--+--+ +--+--+ +--+--+       +--+--+ +--+--+ +--+--+
     |Agent| |Agent| |Agent|       |Agent| |Agent| |Agent|
     |db-p | |app-p| |web-p|       |db-d | |app-d| |web-d|
     +-----+ +-----+ +-----+       +-----+ +-----+ +-----+
       PRD site servers              DR site servers
```

- Each site has its own gateway in its zone
- Agents connect to the gateway in their zone
- Both gateways connect to the same central backend
- Switchover: stop PRD components → start DR components → commit
- Backend tracks `active_site_id` per application

## Data Flow: Health Check Cycle

```
1. Agent scheduler tick (every 30s)
   |
2. Execute check_cmd (shell command)
   |
3. Compare exit_code to cached value (deduplication)
   |
   +-- Same exit_code? → skip (no delta sent)
   |
   +-- Different exit_code? → send CheckResult message
       |
4. Agent --[WebSocket]--> Gateway --[WebSocket]--> Backend
       |
5. Backend: FSM transition (next_state_from_check)
   |
   +-- exit 0: STARTING→RUNNING, DEGRADED→RUNNING
   +-- exit 1: RUNNING→DEGRADED
   +-- exit ≥2: *→FAILED
   |
6. Backend: INSERT INTO state_transitions (append-only)
   |
7. Backend: INSERT INTO check_events (append-only)
   |
8. Backend: WebSocket push to subscribed frontends
   |
9. Frontend: React Query cache invalidation → UI update
```

## Data Flow: Start Application (DAG Sequencing)

```
1. User clicks "Start" / POST /api/v1/apps/{id}/start
   |
2. Backend: INSERT INTO action_log (log before execute)
   |
3. Backend: Kahn's topological sort → execution plan
   |  Level 0: [Oracle-DB]
   |  Level 1: [Tomcat-App, RabbitMQ]
   |  Level 2: [Apache-Front, Batch-Processor]
   |
4. For each level (sequentially):
   |  For each component in level (in parallel):
   |    4a. Set state STARTING → INSERT INTO state_transitions
   |    4b. Send ExecuteCommand (start_cmd) → Gateway → Agent
   |    4c. Agent: double-fork + setsid → detached process
   |    4d. Wait for check_cmd to return exit 0 (RUNNING)
   |    4e. If timeout/fail → mark FAILED, suspend job
   |
5. All levels complete → all components RUNNING → job complete
```

## Data Flow: DR Switchover (6 Phases)

```
Phase 1 - PREPARE:    Verify DR agents connected, run health checks
Phase 2 - FREEZE:     Block user operations on active site
Phase 3 - STOP_SOURCE: Stop all components on PRD (reverse DAG)
Phase 4 - START_TARGET: Start all components on DR (DAG order)
Phase 5 - VERIFY:     Run integrity checks on DR site
Phase 6 - COMMIT:     Update active_site_id (point of no return)

Rollback possible before COMMIT → restart source, cancel switchover
RTO measured: time from FREEZE to COMMIT
```

## Data Flow: 3-Level Diagnostic + Rebuild

```
1. POST /api/v1/apps/{id}/diagnose
   |
2. For each component, run 3 check levels:
   |  Level 1 (Health):       check_cmd → exit code
   |  Level 2 (Integrity):    integrity_check_cmd → exit code
   |  Level 3 (Infrastructure): infra_check_cmd → exit code
   |
3. Recommendation matrix (8 combinations):
   |  H=OK, I=OK, F=OK  → HEALTHY (no action)
   |  H=OK, I=OK, F=KO  → HEALTHY (infra issue but app works)
   |  H=OK, I=KO, *     → HEALTHY (integrity issue, informational)
   |  H=KO, I=OK, F=OK  → RESTART (process died, data OK)
   |  H=KO, I=OK, F=KO  → INFRA_REBUILD (process + infra bad)
   |  H=KO, I=KO, F=OK  → APP_REBUILD (process + data bad)
   |  H=KO, I=KO, F=KO  → INFRA_REBUILD (everything bad)
   |
4. POST /api/v1/apps/{id}/rebuild
   |  Check rebuild_protected flag (409 if protected)
   |  Execute in DAG order (databases before appservers)
   |  For INFRA_REBUILD: use bastion agent (rebuild_agent_id)
   |  Measure RTR (Recovery Time for Rebuild)
```

## WebSocket Considerations for Legacy Environments

WebSocket (RFC 6455) is used for all real-time communication. This is the right
choice for AppControl because:

- **Agent connections are long-lived** (hours/days) — HTTP polling would be wasteful
- **Bidirectional messaging** — backend can push commands to agents without polling
- **Low latency** — state changes are reflected in the UI within milliseconds

### Potential Issues on Legacy Infrastructure

| Scenario | Problem | Mitigation |
|----------|---------|------------|
| Old HTTP/1.0 proxy | Cannot upgrade to WebSocket | Use WSS on port 443 (looks like HTTPS, proxy passes it through) |
| Corporate firewall | Blocks non-HTTP protocols | WSS on 443 is indistinguishable from HTTPS at the TLS level |
| Load balancer timeout | Closes idle connections | Agent heartbeat every 60s keeps connections alive |
| WAF inspection | Deep packet inspection rejects WS frames | Whitelist the gateway endpoint, or use mTLS (WAF cannot inspect) |
| Reverse proxy misconfigured | Missing `Upgrade`/`Connection` headers | nginx config includes proper WebSocket proxy headers (see `nginx.conf`) |

### Why Not HTTP Long Polling?

- Agent-to-gateway requires **bidirectional** real-time messaging
- Long polling would mean agents cannot receive commands instantly (only on next poll)
- The 60s heartbeat interval already acts as a keep-alive mechanism
- WebSocket overhead per message is 2-6 bytes vs. ~500 bytes for HTTP headers

### Deployment Recommendation

For environments with strict proxy requirements:

```
Gateway: listen on port 443 (standard HTTPS port)
         with TLS termination + mTLS client verification
         → passes through all corporate firewalls
         → indistinguishable from HTTPS traffic

LISTEN_PORT=443  (in gateway config / env var)
```

The agent's outbound-only connection model means no firewall rules need
to be opened for inbound traffic on monitored servers.

## Message Protocol (Agent <-> Backend)

### Agent → Backend

| Message | Fields | Purpose |
|---------|--------|---------|
| `Register` | agent_id, hostname, labels, version | First message on connect |
| `Heartbeat` | agent_id, cpu, memory, at | Every 60s, keeps connection alive |
| `CheckResult` | component_id, check_type, exit_code, stdout, duration_ms, at | Delta-only health/integrity/infra check result |
| `CommandResult` | request_id, exit_code, stdout, stderr, duration_ms | Response to ExecuteCommand |

### Backend → Agent

| Message | Fields | Purpose |
|---------|--------|---------|
| `ExecuteCommand` | request_id, component_id, command, timeout_seconds | Run a command on target host |
| `UpdateConfig` | components: Vec<ComponentConfig> | Push new component configuration |
| `Ack` | request_id | Acknowledge receipt |

## FSM State Machine

```
                  +----------+
           +----->| UNKNOWN  |<-----+
           |      +----+-----+      |
           |           | first check|
           |           v            |
      +----+----+  +--------+  +---+-----+
      | STOPPED |  |RUNNING |  | FAILED  |
      +----+----+  +---+----+  +----+----+
           |           |            |
     start |    exit 1 |    retry   |
           v           v            v
      +----+-----+ +---+-----+ +---+-----+
      | STARTING | |DEGRADED | | STOPPING|
      +----+-----+ +----+----+ +----+----+
           |             |           |
     exit 0|       exit 0|     stop  |
           v             v     done  v
        RUNNING       RUNNING    STOPPED

  Any state --[heartbeat timeout]--> UNREACHABLE
  UNREACHABLE --[reconnect]--> previous state
```

Valid transitions:
- `UNKNOWN → RUNNING | STOPPED | FAILED` (first check determines initial state)
- `STOPPED → STARTING` (start command issued)
- `STARTING → RUNNING` (check returns exit 0)
- `STARTING → FAILED` (check returns exit ≥2 or timeout)
- `RUNNING → DEGRADED` (check returns exit 1)
- `RUNNING → FAILED` (check returns exit ≥2)
- `RUNNING → STOPPING` (stop command issued)
- `DEGRADED → RUNNING` (check returns exit 0)
- `DEGRADED → FAILED` (check returns exit ≥2)
- `DEGRADED → STOPPING` (stop command issued)
- `STOPPING → STOPPED` (stop confirmed by check)
- `FAILED → STARTING` (restart attempt)
- `FAILED → STOPPING` (cleanup stop)
- `* → UNREACHABLE` (heartbeat timeout, 3 missed cycles)
- `UNREACHABLE → *` (agent reconnects, replays buffered state)

## Permission Model

```
view < operate < edit < manage < owner

  view:    Read app status, components, reports
  operate: Start, stop, restart, execute commands
  edit:    Modify app config, add/remove components, dependencies
  manage:  Grant permissions, create share links, manage switchovers
  owner:   Delete app, transfer ownership

  Effective = MAX(direct_user_permission, team_permissions...)
  Org admin = implicit owner on ALL apps in the organization
```

Note: `edit` does NOT include `operate`. An editor can change config but
cannot start/stop. This is intentional — separation of config and operations.

## Process Detachment (Agent)

When the agent starts a process (start_cmd, rebuild_cmd), it uses
double-fork + setsid to ensure the process survives agent crash:

```
Agent process
  |
  fork()
  |
  +-- Intermediate child
       |
       setsid()   ← new session leader, detached from agent's session
       |
       fork()
       |
       +-- Grandchild (detached process)
            |
            close(0,1,2)  ← close stdin/stdout/stderr
            /dev/null redirect
            execvp(command)
            |
            Now reparented to PID 1 (init)
            Survives agent crash/restart

  Intermediate child exits immediately
  Agent reaps intermediate child (waitpid)
  Grandchild PID returned via pipe for tracking
```
