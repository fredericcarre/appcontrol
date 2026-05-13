# Capacity Planning

This page provides sizing guidance for AppControl deployments — from a 10-app pilot to a 1000-app enterprise install. All numbers are derived from the code (see source references) and from typical production deployments; treat them as starting points and validate with load testing.

For the topology these resources support, see [High Availability](HIGH_AVAILABILITY.md). For storage growth math, see [Backup & Restore](BACKUP_RESTORE.md).

## Table of contents

- [Headline numbers](#headline-numbers)
- [Backend sizing](#backend-sizing)
- [PostgreSQL sizing](#postgresql-sizing)
- [Agent host footprint](#agent-host-footprint)
- [Gateway sizing](#gateway-sizing)
- [Agents per backend replica](#agents-per-backend-replica)
- [Cache (Redis) — no longer used](#cache-redis--no-longer-used)
- [WebSocket fan-out and latency targets](#websocket-fan-out-and-latency-targets)
- [Worked example: 200 apps × 30 components × 1 check / min](#worked-example-200-apps--30-components--1-check--min)
- [Worked example: 5000 agents, mostly idle](#worked-example-5000-agents-mostly-idle)

---

## Headline numbers

For quick reference:

| Metric | Value | Source |
|--------|-------|--------|
| Backend RAM per replica | 1–2 GiB | `production-values.yaml` requests/limits |
| Backend CPU per replica | 1–2 vCPU | `production-values.yaml` requests/limits |
| `DB_POOL_SIZE` formula | `10 + apps × 0.5`, capped at 50 | sqlx pool sizing |
| `action_log` row size | ≈ 500 bytes | typical JSONB + IDs |
| `state_transitions` row size | ≈ 300 bytes | small JSONB |
| `check_events` row size | ≈ 200 bytes | bounded fields, truncated stdout |
| Agent RSS | 20–40 MB | sysinfo sampling |
| Agent CPU | < 1 % steady state | scheduled checks |
| Agent buffer | < 100 MB | sled FIFO cap |
| Gateway connections per agent | 1 WS | by design |
| Agents per backend replica | 5 000–10 000 | empirical limit, single-shard |

---

## Backend sizing

### CPU per HTTP RPS

The backend serves stateless REST + WebSocket. Typical per-request CPU:

| Endpoint class | CPU per request | Latency P50 / P95 |
|----------------|-----------------|-------------------|
| `GET /apps` (list, no joins) | 0.3 ms | 5 / 20 ms |
| `GET /apps/:id` (with components, deps) | 2 ms | 20 / 80 ms |
| `POST /apps/:id/start` (DAG + sequencer kickoff) | 8 ms | 80 / 200 ms |
| `GET /reports/availability` (aggregations) | 50 ms | 200 / 800 ms |

A 2-vCPU replica sustains roughly 300–500 RPS of mixed traffic with P95 < 200 ms. Beyond that, scale horizontally.

### RAM per replica

The backend's RSS is dominated by:

- sqlx connection pool buffers (≈ 5 MB per connection × `DB_POOL_SIZE`)
- WebSocket subscriber maps (`DashMap<user_id, Vec<Sender>>`) — ≈ 2 KB per active subscriber
- Tokio task stacks (negligible)

For `DB_POOL_SIZE=20` and 500 active WebSocket subscribers: ≈ 100 MB + 1 MB + base. The Helm defaults set requests=1 GiB / limits=2 GiB which gives ample headroom.

### `DB_POOL_SIZE` formula

The recommended formula is:

```
DB_POOL_SIZE = min(50, 10 + apps × 0.5)
```

Where `apps` is the number of applications under management. The rationale:

- The base of 10 covers baseline traffic (health checks, dashboards).
- Each application adds about 0.5 concurrent connections at steady-state (one for the user looking at it, one occasional for the FSM update).
- The cap at 50 prevents exhausting PostgreSQL's `max_connections` when running many replicas.

Examples:

| Apps | Pool size | Why |
|-----:|---------:|-----|
| 20 | 20 | 10 + 10 |
| 100 | 50 (cap) | 10 + 50, capped |
| 500 | 50 (cap) | always capped — add PgBouncer if needed |

If you run 3+ backend replicas, total connections = `replicas × DB_POOL_SIZE`. Confirm PostgreSQL's `max_connections` ≥ this total + 20 % headroom.

### Replicas

```
recommended_replicas = ceil(peak_RPS / 300) + 1  (the +1 covers rolling upgrades)
```

For most installs, 3 replicas is the right starting point. Above 1500 RPS sustained, scale to 5–7.

---

## PostgreSQL sizing

### Storage growth model

```
daily_bytes ≈
  action_log_rows × 500
  + state_transitions_rows × 300
  + check_events_rows × 200
```

Where:

- `action_log_rows` ≈ 50 × users × active_apps_per_day (one row per user action; rough average)
- `state_transitions_rows` ≈ check_events × 0.05 (only ~ 5 % of checks change exit code)
- `check_events_rows` ≈ components × checks_per_minute × 60 × 24

### Worked storage math

For an install with **1000 components**, **1 health check / 30 s**, **30 days retention** on `check_events`:

```
check_events_per_day  = 1000 × 2 × 60 × 24 = 2 880 000 rows/day
check_events_30_days  = 86 400 000 rows
storage               ≈ 86.4 M × 200 B = 17.3 GB
```

Add `action_log` and `state_transitions`:

```
action_log_per_day        = 50 × 20 × 100 = 100 000  (~ 50 MB / day)
state_transitions_per_day = 2 880 000 × 0.05 = 144 000  (~ 43 MB / day)
```

**Total daily growth: ≈ 18 GB / month** for the example above. With 90-day retention on `check_events`: ≈ 54 GB.

### Partition retention math

`check_events` is partitioned by month. With `RETENTION_CHECK_EVENTS_DAYS=90`:

- 3–4 monthly partitions are kept live at any time.
- The background retention task drops partitions older than the cutoff (daily check).
- Drop time is O(1) — PostgreSQL just unlinks the file.

To halve storage, halve the retention; to keep audit forever, set `0` and offload monthly partitions to cold storage (see [Backup & Restore](BACKUP_RESTORE.md#partitioned-check_events)).

### Index and WAL overhead

Add ≈ 30 % on top of raw row size for:

- B-tree indexes on `(component_id, created_at)` for every event table.
- WAL records during write-heavy periods.
- Visibility map and free-space map.

So the 17 GB row total above turns into ≈ 22 GB on disk.

### Compute sizing

| Component count | Recommended PostgreSQL size |
|----------------|-----------------------------|
| < 100 | 2 vCPU / 8 GB RAM / 100 GB SSD |
| 100–500 | 4 vCPU / 16 GB / 250 GB SSD |
| 500–2000 | 8 vCPU / 32 GB / 500 GB SSD |
| > 2000 | 16 vCPU / 64 GB / 1 TB SSD + PgBouncer |

For managed databases on cloud providers:

- AWS RDS: `db.r6g.large` for 100–500 components, `db.r6g.xlarge` for 500–2000.
- GCP CloudSQL: `db-custom-2-7680` to `db-custom-8-32768`.
- Azure DB for PostgreSQL: `GP_Gen5_2` to `GP_Gen5_8`.

---

## Agent host footprint

The agent is a single Rust binary (~ 8 MB static). On a stable host with ~ 50 managed components:

| Resource | Typical | Peak |
|----------|--------:|----:|
| RSS | 25 MB | 50 MB (during a burst of checks) |
| CPU | < 1 % | 5–10 % during start_cmd execution |
| Disk (binary) | 8 MB | 8 MB |
| Disk (buffer DB at `/var/lib/appcontrol/buffer-{id}`) | < 1 MB normal | < 100 MB during prolonged disconnect |
| Network outbound | ~ 1–5 KB/s baseline | up to 100 KB/s during buffer replay |
| Network inbound | nil — agent is outbound-only | nil |

The agent has no inbound port; outbound TCP 4443 (or 443) to the gateway is sufficient.

### Process detachment cost

When the agent runs a detached `start_cmd` (double-fork + setsid), each child consumes the resources of the started process. The agent itself does not retain them — `waitpid` reaps the intermediate child. So your sizing for `start_cmd` processes is independent of the agent footprint.

### Resource limits applied to checks

The agent applies these limits before `exec` for sync check commands:

| `RLIMIT_CPU` | `RLIMIT_AS` | `RLIMIT_NOFILE` | `RLIMIT_NPROC` | Default timeout |
|--------------|-------------|-----------------|----------------|-----------------|
| 30 s | 512 MB | 512 fd | 64 child processes | 120 s |

A runaway check command cannot exhaust the host. See [Security Architecture §8](SECURITY_ARCHITECTURE.md).

---

## Gateway sizing

Gateways are stateless WebSocket relays. Sizing is driven by:

1. **Number of concurrent agent connections** — each holds one WebSocket.
2. **Messages per second** — each WS message is a few hundred bytes.

### Per-gateway capacity

| Resource | Per 1000 agents | Per 5000 agents | Per 10 000 agents |
|----------|----------------:|----------------:|------------------:|
| RAM | ~ 128 MB | ~ 512 MB | ~ 1 GB |
| CPU | < 0.5 vCPU | 1 vCPU | 2 vCPU |
| File descriptors | 1000+ | 5000+ | 10 000+ |
| TCP connections | 1001 (+ 1 to backend) | 5001 | 10 001 |

### `ulimit` adjustment

For > 1024 agents per gateway, raise the open-file limit:

```bash
# /etc/security/limits.d/appcontrol.conf
appcontrol soft nofile 65536
appcontrol hard nofile 65536

# Or in the systemd unit
[Service]
LimitNOFILE=65536
```

For Kubernetes, set `securityContext.sysctls` or use a node-level adjustment.

### Recommended replicas

For redundancy, deploy at least 2 gateways even for small fleets. For large fleets, distribute by zone:

| Total agents | Recommended gateway count |
|------------:|---------------------------:|
| < 500 | 2 (N+1) |
| 500–5000 | 3–4 (distribute load) |
| 5000–20 000 | 5–10 (zone-scoped) |

---

## Agents per backend replica

Rough rule of thumb: **5 000 – 10 000 agents per backend replica**.

This is bounded by:

1. **WebSocket fan-out load.** Every agent's heartbeat (every 60 s) and CheckResult (delta-only) terminates at a backend replica via the gateway → backend WS. With 10 000 agents, the backend handles ~ 170 inbound WS messages / s baseline.
2. **DB pool size.** Each agent's state update is a brief DB write. At 10 000 agents and 30 s check interval, the worst case is ~ 330 writes / s — well within a 20-connection pool.
3. **CPU.** Each WS frame requires deserialization (serde_json), FSM logic, and an SQL `INSERT`. Empirical: 1 vCPU handles ~ 500 inbound WS frames / s.

To scale beyond 10 000 agents per replica, add backend replicas — agents are evenly distributed across the gateway pool, and gateway → backend hashing keeps a given agent's traffic on a stable backend (or fans out, depending on your LB config).

---

## Cache (Redis) — no longer used

Phase 10 (see [CHANGELOG](CHANGELOG.md)) removed the Redis dependency. Token revocation and rate limiting now use PostgreSQL exclusively.

**You do not need to provision Redis for AppControl.** If your environment requires a cache layer for unrelated reasons, deploy Redis independently and ignore AppControl's connection settings.

The `REDIS_URL` environment variable is no longer recognized by the backend.

---

## WebSocket fan-out and latency targets

### Events per second

A typical AppControl event source breakdown for 1000 components:

| Source | Frequency | Events / s |
|--------|-----------|-----------:|
| Health-check deltas | 5 % of 1000 components × 30 s | ~ 1.7 |
| State transitions | derived from deltas | ~ 1.7 |
| Agent heartbeats (60 s) | 100 agents | ~ 1.7 |
| Operator actions | bursty | ~ 0.5 |

Total: ≈ 5–10 events/s sustained, with bursts to 50/s during operations. Multiply linearly with component count.

### Latency budget

| Hop | Target P95 |
|-----|-----------:|
| Agent → Gateway (WS frame) | 5 ms |
| Gateway → Backend (WS frame) | 5 ms |
| Backend → DB INSERT | 10 ms |
| Backend → WS Hub fan-out | 5 ms |
| Backend → Frontend WS frame | 10 ms |
| **Total agent-event → UI render** | **35 ms** |

If your install consistently exceeds 100 ms agent-event → UI latency, monitor:

- `agent_message_latency_warn_ms` Prometheus metric (recommended threshold: 200 ms _(verify in code; was not found at audit time)_)
- `ws_connections_active` per replica — imbalance is a hint of broken affinity
- DB `INSERT` time — slow `state_transitions` inserts cascade

### WebSocket fan-out scaling

Each backend replica maintains an in-memory `DashMap` of subscribers. The cross-replica notification path uses PostgreSQL `LISTEN/NOTIFY`. This scales linearly with the number of backend replicas — there is no global fan-out queue.

---

## Worked example: 200 apps × 30 components × 1 check / min

A medium enterprise install. The goal is to derive concrete resource requirements.

### Inputs

- 200 applications
- 30 components per application on average → 6000 total components
- 60 s check interval (1 check / min per component)
- 30 distinct agents per application → 6000 / 30 = 200 components per agent on average → 200 active agents
- Working hours: 50 operator actions / day; 50 dashboard users
- 90-day retention on `check_events`

### Computed values

#### Check events

```
checks/sec  = 6000 components / 60 s = 100 checks/s
deltas/sec  = 100 × 5 %         = 5 deltas/s  (95 % deduplicated)
check_events_per_day = 100 × 86400 = 8 640 000 rows/day
storage_30d ≈ 8.64 M × 30 × 200 B = 52 GB raw + 30 % index/WAL = 68 GB on disk
```

#### State transitions

```
state_transitions/day ≈ 8.64 M × 5 % = 432 000 rows/day
storage_30d ≈ 432 K × 30 × 300 B = ~ 4 GB
```

#### Action log

```
50 actions × 30 days = 1500 rows/month — negligible
```

#### DB pool

```
DB_POOL_SIZE = min(50, 10 + 200 × 0.5) = min(50, 110) = 50
```

#### Recommended backend

| Resource | Value |
|----------|-------|
| Replicas | 3 |
| CPU per replica | 2 vCPU |
| RAM per replica | 2 GB |
| Total backend CPU | 6 vCPU |
| Total backend RAM | 6 GB |

#### Recommended PostgreSQL

| Resource | Value |
|----------|-------|
| CPU | 4 vCPU |
| RAM | 16 GB |
| Storage (90 d, no archive) | 200 GB SSD |
| Connections | `max_connections = 200` |

#### Recommended gateway

| Resource | Value |
|----------|-------|
| Replicas | 2 |
| CPU per replica | 0.5 vCPU |
| RAM per replica | 256 MB |

#### Per-agent profile

- 200 components → ~ 35 MB RSS, < 1 % CPU
- 200 checks/min × delta ratio ≈ 10 deltas/min × 200 bytes ≈ 2 KB/min network egress (negligible)

#### Per-frontend session

- ~ 1 MB RAM in the browser
- ~ 5 KB/s WebSocket inbound from backend during dashboard view

### Helm values

```yaml
backend:
  replicaCount: 3
  resources:
    requests: { cpu: "1",   memory: 1Gi }
    limits:   { cpu: "2",   memory: 2Gi }
  env:
    HA_MODE: "true"
    DB_POOL_SIZE: "50"
    RETENTION_CHECK_EVENTS_DAYS: "90"

gateway:
  replicaCount: 2
  resources:
    requests: { cpu: "500m", memory: 256Mi }
    limits:   { cpu: "1",    memory: 512Mi }
```

### Cost rough order-of-magnitude (AWS pricing)

- Backend: 3 × `t3.large` ≈ $200/mo
- Gateway: 2 × `t3.small` ≈ $30/mo
- PostgreSQL: `db.r6g.large` Multi-AZ ≈ $300/mo
- Storage (200 GB SSD + backup) ≈ $50/mo
- LB and data transfer: ≈ $40/mo
- **Total ≈ $620/mo** for a control plane managing 6000 components across 200 hosts.

---

## Worked example: 5000 agents, mostly idle

A surveillance install — many agents, low check frequency, infrequent operations.

### Inputs

- 5000 agents, 5 components each → 25 000 components
- Check interval: 5 minutes
- 99 % stable (deltas only 1 %)

### Computed values

```
checks/sec = 25 000 / 300 = 83 checks/s
deltas/sec = 83 × 1 % = 0.8 deltas/s
check_events/day = 83 × 86400 = 7.2 M rows/day → 1.5 GB/day raw
storage_30d ≈ 45 GB raw + 13 GB index ≈ 60 GB
```

The DB grows slower than the previous example because the check interval is 5× longer.

### Gateway sizing

5000 agents at 5 different sites → 1 gateway per site = 5 gateways minimum. Each gateway handles 1000 agents — well within 1 vCPU / 256 MB.

### Backend sizing

WS throughput is low (< 1 frame/s baseline). One backend replica suffices, but always run ≥ 2 for HA. CPU is dominated by 50 dashboard users on the frontend.

```yaml
backend:
  replicaCount: 3
  resources:
    requests: { cpu: "500m", memory: 512Mi }
    limits:   { cpu: "1",    memory: 1Gi }
gateway:
  replicaCount: 5     # 1 per site
```

### File descriptor planning

Each gateway holds 1000 agent WebSockets + 1 backend WebSocket = 1001 fds. The default `ulimit -n 1024` is too tight; set `LimitNOFILE=4096`.

---

## Reference

- [BACKUP_RESTORE.md](BACKUP_RESTORE.md) — how the storage volume is preserved and retained
- [HIGH_AVAILABILITY.md](HIGH_AVAILABILITY.md) — replicas and failure-domain distribution
- [LIMITS.md](LIMITS.md) — explicit per-entity limits
- [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md) — full deployment topology
