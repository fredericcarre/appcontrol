# Limits and Quotas

This page documents the limits enforced by AppControl, along with the configurable knobs and what happens when each limit is reached. Numbers come from the code; treat the recommended values as guidance, not hard guarantees.

For sizing the deployment to stay within these limits, see [Capacity Planning](CAPACITY_PLANNING.md).

## Table of contents

- [Per-entity limits](#per-entity-limits)
- [Storage limits](#storage-limits)
- [Rate limits](#rate-limits)
- [Connection and pool limits](#connection-and-pool-limits)
- [Agent-side limits](#agent-side-limits)
- [Token and credential limits](#token-and-credential-limits)
- [HTTP and WebSocket limits](#http-and-websocket-limits)
- [Cluster limits](#cluster-limits)
- [DAG limits](#dag-limits)

---

## Per-entity limits

### Components per application

- **Hard limit:** none. The schema accepts any number of `components` rows linked to one `applications` row.
- **Recommended:** ≤ 500 for a usable DAG visualization. Above this, the Map View becomes hard to navigate even with React Flow's optimizations.
- **What happens at the limit:** No technical failure — performance degrades. The map's auto-layout step grows roughly O(n²); a 1000-node application takes several seconds to render initially.
- **Workaround for large apps:** Split into multiple applications and reference one from another via a [referenced app](GLOSSARY.md#referenced-app) component type.

### Applications per organization

- **Hard limit:** none.
- **Recommended:** plan capacity per [Capacity Planning](CAPACITY_PLANNING.md). The list endpoint (`GET /apps`) loads all applications the user has access to; for very large orgs this becomes a Postgres heavy-read.
- **What happens at the limit:** Above ~5000 apps per user, the dashboard's initial load slows visibly. The user can filter by tag or site to recover responsiveness.

### Components per agent

- **Hard limit:** none.
- **Recommended:** ≤ 500 per agent. The agent runs every component's checks on its local scheduler; CPU rises proportionally with the count.
- **What happens at the limit:** The agent may exceed its `RLIMIT_CPU` for the host. Splitting work across multiple agents on the same host is supported via explicit `AGENT_ID` overrides.

### Sites and hostings per organization

- **Hard limit:** none.
- **Recommended:** ≤ 50 sites, ≤ 20 hostings. Beyond that, the site picker in the UI becomes unwieldy.

### Users, teams, workspaces

- **Hard limit:** none.
- **Recommended:** ≤ 5000 users, ≤ 1000 teams, ≤ 100 workspaces per organization.

---

## Storage limits

### Action log row size

- **`stdout` field truncation:** the agent truncates `stdout` to **4 KB per check** before sending. The full command output is captured in `command_executions` for sync commands.
- **Why 4 KB:** A larger payload would dominate the WebSocket frame; check outputs are summaries, not full logs. To capture the full output, set `output_streaming = true` on the component (migration `V019`) and stream into the `command_executions.stdout_chunks` table. _(verify in code; was not found at audit time)_

### `action_log` retention

- **Default:** `RETENTION_ACTION_LOG_DAYS=0` (unlimited).
- **Recommended:** `1825` (5 years) for DORA compliance.
- **What happens at the limit:** When the daily retention task runs, rows older than the cutoff are **archived** to `action_log_archive` (not deleted — respects the append-only rule, see Phase 10 CHANGELOG entry).

### `check_events` retention

- **Default:** `RETENTION_CHECK_EVENTS_DAYS=0` (unlimited).
- **Recommended:** `90` for production (3 months of hot health-check history).
- **Mechanism:** `check_events` is partitioned by month. The retention task **drops entire partitions** older than the cutoff. This is O(1) per partition — no row scan.
- **What happens at the limit:** Older partitions are no longer queryable from PostgreSQL. If you need long-term retention for regulatory reasons, archive monthly partitions to cold storage **before** they are dropped. See [Backup & Restore — Partitioned check_events](BACKUP_RESTORE.md#partitioned-check_events).

### `state_transitions`, `switchover_log`, `config_versions`

- **Retention:** no separate knob; growth is moderate (state transitions are ~ 5 % of check rates).
- **Cleanup:** none by default. These tables are essential audit; design for permanent retention.
- **What happens at the limit:** Disk fills up. Plan storage growth from day one (see [Capacity Planning — PostgreSQL](CAPACITY_PLANNING.md#postgresql-sizing)).

---

## Rate limits

All rate limits are configurable per organization via environment variables on the backend. Defaults below.

### Authentication endpoints

| Variable | Default | Scope | Endpoints |
|----------|--------:|-------|-----------|
| `RATE_LIMIT_AUTH` | **10 req / minute per IP** | Per source IP | `/auth/oidc/callback`, `/auth/saml/acs`, `/auth/login`, `/api-keys` |

**What happens at the limit:** HTTP `429 Too Many Requests`. Response headers:

```
HTTP/2 429
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 32
Retry-After: 32
```

### Operation endpoints

| Variable | Default | Scope | Endpoints |
|----------|--------:|-------|-----------|
| `RATE_LIMIT_OPERATIONS` | **5 req / minute per user** | Per authenticated user | `POST /apps/:id/start`, `/stop`, `/switchover`, `/rebuild`, etc. |

The lower per-user limit is intentional: a single operator rarely needs more than 5 start/stop operations per minute. Schedulers using API keys are subject to the same per-user limit (where the API key counts as one user).

### Read endpoints

| Variable | Default | Scope | Endpoints |
|----------|--------:|-------|-----------|
| `RATE_LIMIT_READS` | **200 req / minute per user** | Per authenticated user | All `GET /api/v1/*` |

This is high enough to support live dashboards that poll the API. The frontend's React Query layer further reduces calls via caching.

### Tuning

To raise limits, set the env var on the backend:

```yaml
backend:
  env:
    RATE_LIMIT_AUTH:       "20"
    RATE_LIMIT_OPERATIONS: "10"
    RATE_LIMIT_READS:      "500"
```

To **disable** rate limiting (not recommended in production), set the value to `0`. The limiter short-circuits before touching its counter, so `RATE_LIMIT_AUTH=0`, `RATE_LIMIT_OPERATIONS=0` and `RATE_LIMIT_READS=0` all behave as "unlimited". Disabling applies in both single-instance and `HA_MODE=true` deployments.

### HA mode

When `HA_MODE=true`, rate-limit counters move from in-process `DashMap` to PostgreSQL (`INCR + EXPIRE` pattern). This is critical for multi-replica deployments — otherwise the limits are per-replica, not global.

---

## Connection and pool limits

### Backend DB pool

| Variable | Default | Description |
|----------|--------:|-------------|
| `DB_POOL_SIZE` | **20** | Maximum connections per replica |
| `DB_IDLE_TIMEOUT_SECS` | **600** | Close idle connections after N seconds |
| `DB_CONNECT_TIMEOUT_SECS` | **30** | Timeout for acquiring a connection from the pool |

**What happens at the limit:**

- Pool exhausted → new requests wait up to `DB_CONNECT_TIMEOUT_SECS` for a connection.
- Timeout reached → backend returns `503 Service Unavailable` with `acquire timeout` in the log.
- See [Troubleshooting — DB pool exhausted](TROUBLESHOOTING.md#db-pool-exhausted) for the diagnosis flow.

### PostgreSQL `max_connections`

PostgreSQL itself has a global `max_connections` (default 100 in upstream, often 200 in managed services). Plan:

```
max_connections ≥ (replicas × DB_POOL_SIZE) + admin_headroom
```

Run [PgBouncer](https://www.pgbouncer.org/) in front of PostgreSQL if you need many replicas — see [High Availability — Database HA](HIGH_AVAILABILITY.md#database-ha).

### WebSocket: 1 connection per agent → gateway

By design. Each agent maintains exactly one persistent WebSocket to its currently-selected gateway. Failover replaces the connection, never duplicates it.

### Gateway → Backend: 1 multiplexed WebSocket

Each gateway opens **one** WebSocket to the backend over which all agents' traffic is multiplexed. This means N gateways → N WebSockets at the backend, regardless of the agent count.

**What happens at the limit:** N is small (typically ≤ 10 gateways per install), so this is rarely a limit. If a gateway's WS is saturated, the gateway uses backpressure: when its 1024-message send buffer to an agent or 4096-message buffer to the backend fills, it **drops** messages with a warning log (see CHANGELOG Phase 8 "Bounded channels with backpressure").

### Per-gateway file descriptor limit

Each agent connection consumes one file descriptor on the gateway host. Defaults are:

- Linux: `ulimit -n 1024` by default — too tight for > 1024 agents.
- Recommended: `LimitNOFILE=65536` in the systemd unit or `securityContext.sysctls` in Kubernetes.

**What happens at the limit:** Accept fails with `EMFILE`; agents see "connection refused" and fail over.

---

## Agent-side limits

### Disk buffer

- **Storage backend:** sled embedded KV store.
- **Path:** `/var/lib/appcontrol/buffer-{agent_id}` (Linux), `%PROGRAMDATA%\AppControl\buffer-{id}` (Windows).
- **Default cap:** ~ 100 MB (FIFO rotation).
- **What happens at the limit:** Oldest entries are evicted to make room. The agent does not block check execution.

### Per-command resource limits

The agent applies these limits before `exec` for sync check commands:

| Limit | Value | Configurable |
|-------|-------|--------------|
| `RLIMIT_CPU` | **30 seconds** | env var, _(verify in code; was not found at audit time)_ |
| `RLIMIT_AS` | **512 MB** | env var, _(verify in code; was not found at audit time)_ |
| `RLIMIT_NOFILE` | **512 file descriptors** | env var, _(verify in code; was not found at audit time)_ |
| `RLIMIT_NPROC` | **64 child processes** | env var, _(verify in code; was not found at audit time)_ |
| Default check timeout | **120 seconds** | per-component `check_interval_seconds` |

**What happens at the limit:**

- `RLIMIT_CPU` exceeded → process receives SIGXCPU, then SIGKILL.
- `RLIMIT_AS` exceeded → `malloc` returns NULL; well-behaved processes exit.
- Timeout exceeded → agent kills the process group (SIGTERM, then SIGKILL after 5s grace, returns `exit_code = -1`).

Detached commands (`start_cmd`, `stop_cmd`, `rebuild_cmd`) **do not have these limits applied** — they intentionally inherit the parent shell context to support long-running services. Securing them is the responsibility of the system service manager (systemd `MemoryMax`, etc.).

### Check command deduplication

If multiple components on the same agent share the same `check_cmd` (verbatim), the agent executes it **once** per scheduling tick and shares the result. Key: SHA-256 of the command string.

- **Hard limit:** none.
- **Implication:** changing the command (even whitespace) invalidates the dedup key.

---

## Token and credential limits

### Enrollment tokens

| Field | Default | Hard limit |
|-------|---------|------------|
| `max_uses` | 1 | none |
| `valid_hours` | **24** | none (but recommended ≤ 168 / 7 days) |
| Token format | `ac_enroll_<28 random chars>` | n/a |

- **What happens at `max_uses`:** Next enrollment request returns HTTP `409 Conflict`. The token must be regenerated.
- **What happens at expiry:** HTTP `410 Gone`. Same fix.

### API keys

| Field | Default | Hard limit |
|-------|---------|------------|
| `expires_in_days` | none (caller-supplied) | none |
| `name` length | 1–100 chars | varchar(100) in `api_keys` table |
| Rate of use | n/a — only the API-level rate limit applies | n/a |
| Number of keys per user | none | none |

**Recommendation:** set `expires_in_days: 90` for production. See [Hardening — API key rotation](HARDENING.md#api-key-rotation-policy).

### Share links

| Field | Default | Hard limit |
|-------|---------|------------|
| `max_uses` | unlimited | none |
| `expires_at` | unlimited | none |
| Permission level granted | configurable | up to `manage` (cannot grant `owner` via a link) |

### JWT tokens

- **Expiry:** 24 hours.
- **Refresh:** the frontend silently refreshes when < 5 min remain.
- **Revocation:** entries in the `revoked_tokens` table block reuse until natural expiry.

---

## HTTP and WebSocket limits

### HTTP request size

- **Default:** Axum's default body limit is **2 MB**. Configurable in code, not via env var.
- **What happens at the limit:** HTTP `413 Payload Too Large`.
- **Relevant endpoints:**
  - `POST /apps/:id/import` — YAML/JSON application import. The 2 MB ceiling allows ~ 5000 components per import.
  - `POST /admin/agent-binaries` — agent binary upload. Increased to **50 MB** for this endpoint. _(verify in code; was not found at audit time)_

### Request timeout

- **Default:** no global timeout (axum); per-route timeouts vary.
- **WebSocket idle timeout:** 60 s heartbeat + 30 s grace = ~ 90 s before the server forcibly closes a silent socket.
- **Long operations:** operations that take > 60 s return an `operation_id` immediately; the client polls or watches a WebSocket for progress.

### WebSocket message size

- **Default:** 64 MB per frame (tokio-tungstenite default).
- **Practical limit:** most frames are < 1 KB; large ones come from `UpdateConfig` snapshots with many components.

### CORS preflight

- **Cache:** `Access-Control-Max-Age: 600` (10 min). Browsers cache the preflight result.

---

## Cluster limits

### Members per fan-out cluster

- **Hard limit:** none.
- **Recommended:** ≤ 100. Each member is an independently-monitored entity; the parent state is computed by the [cluster health policy](GLOSSARY.md#cluster-health-policy) over all members.
- **What happens at the limit:** Performance degrades. The state-rollup query (`SELECT count(*) FILTER (WHERE current_state = 'RUNNING') ... FROM cluster_member_state`) scales linearly. At 1000+ members, expect 50–100 ms rollup latency.

### Aggregate clusters

Aggregate clusters have no native limit because they are represented by a single component — the user-defined `check_cmd` handles cluster-wide aggregation externally.

### Cluster member overrides

Each `cluster_members` row may override `check_cmd`, `start_cmd`, `stop_cmd`, `env_vars`. Overrides are unbounded in count.

---

## DAG limits

### Depth (longest dependency chain)

- **Hard limit:** none.
- **Tested up to:** 50 levels deep.
- **What happens at the limit:** Topological sort (Kahn's algorithm) is O(V + E). Operations on very deep DAGs serialize sequentially because each level must complete before the next starts.

### Width (parallelism per level)

- **Hard limit:** none.
- **Tested up to:** 200 components in parallel.
- **What happens at the limit:** All components at the same level are dispatched in parallel via `tokio::join!`. Bottleneck becomes the agent's ability to handle concurrent ExecuteCommand frames.

### Total edges

- **Hard limit:** none.
- **What happens at the limit:** Cycle detection in import / edit becomes slower (O(V + E) per check). For 10 000+ edges, this may take seconds.

### Cycle detection

- AppControl rejects cycles at write time (import, edit). The check uses Kahn's algorithm: if the topological sort fails (count < |V|), a cycle exists.
- HTTP `400 Bad Request` is returned with the offending cycle nodes in the error body.

---

## Native command limits

### HTTP probes

For `check_native = { "kind": "http", ... }`:

- **Default timeout:** 5 s (in the spec).
- **Max URL length:** 8 KB (curl's limit).
- **Max response inspected:** 64 KB (then truncated).
- **Headers:** any user-provided headers.

### TCP probes (planned)

- **Default timeout:** 3 s.

### Process probes (planned)

- **Implementation:** name-matching via `sysinfo` — no limit on process count scanned.

---

## What happens at the limit — summary table

| Limit | Default | At the limit |
|-------|---------|--------------|
| Rate limit | 10/200/5 per min | HTTP 429 |
| DB pool | 20 conns | HTTP 503 after 30 s wait |
| `action_log` retention | unlimited | rows archived |
| `check_events` retention | unlimited | oldest partition dropped |
| Agent disk buffer | 100 MB | FIFO eviction |
| `RLIMIT_CPU` (checks) | 30 s | process killed |
| Enrollment token max_uses | 1 | HTTP 409 |
| Enrollment token validity | 24 h | HTTP 410 |
| API key expiry | none set | HTTP 401 |
| HTTP body | 2 MB | HTTP 413 |
| Gateway agent connections | open fds | accept fails (EMFILE) |
| DAG cycle in edit/import | n/a | HTTP 400 |

---

## Reference

- [Capacity Planning](CAPACITY_PLANNING.md) — how to size the install to stay within limits
- [High Availability](HIGH_AVAILABILITY.md) — pool sizing for multi-replica deployments
- [Troubleshooting](TROUBLESHOOTING.md) — diagnostic flows when a limit is hit
- [Hardening](HARDENING.md) — recommended retention and rate-limit values for production
- [migrations/V005__event_tables.sql](https://github.com/fredericcarre/appcontrol/blob/main/migrations/V005__event_tables.sql) — `check_events` partition definition
- [migrations/V016__fsm_cache_notifications_locks.sql](https://github.com/fredericcarre/appcontrol/blob/main/migrations/V016__fsm_cache_notifications_locks.sql) — operation lock schema
