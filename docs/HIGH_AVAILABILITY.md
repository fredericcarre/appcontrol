# High Availability

This page describes how to deploy AppControl in a high-availability topology and explains the failure modes survived by each pattern. For DR scenarios that survive site loss, see [Disaster Recovery](DISASTER_RECOVERY.md).

## Table of contents

- [HA topology overview](#ha-topology-overview)
- [Backend HA](#backend-ha)
- [Gateway HA](#gateway-ha)
- [Database HA](#database-ha)
- [Agent reconnect semantics](#agent-reconnect-semantics)
- [WebSocket affinity](#websocket-affinity)
- [Example load balancer configurations](#example-load-balancer-configurations)
- [Example Helm values overlay](#example-helm-values-overlay)
- [What survives, what does not](#what-survives-what-does-not)

---

## HA topology overview

```
                ┌──────────────────────────┐
                │ External LB / Ingress    │  L7, TLS, sticky NOT required
                │   appcontrol.example.com │
                └────────┬─────────────────┘
                         │
        ┌────────────────┴────────────────┐
        │                                 │
   ┌────▼─────┐                      ┌────▼─────┐
   │ Frontend │  x N (nginx)         │ Backend  │  x 3 (Rust + Axum)
   │  pods    │                      │  pods    │
   └──────────┘                      └────┬─────┘
                                          │ sqlx pool (TCP+TLS)
                                          ▼
                                  ┌───────────────┐
                                  │ PostgreSQL 16 │  Primary + sync replica
                                  │   Patroni /   │  (or managed: RDS Multi-AZ,
                                  │   pgBouncer   │   CloudSQL HA, Azure HA)
                                  └───────────────┘

                ┌──────────────────────────┐
                │ External LB (TCP)         │
                │  gateway.example.com:4443 │  Layer 4, NLB
                └────────┬─────────────────┘
                         │
        ┌────────────────┴────────────────┐
        │                                 │
   ┌────▼──────┐                     ┌────▼──────┐
   │ Gateway 1 │ (PRD zone)          │ Gateway 2 │ (DR zone)
   └────┬──────┘                     └────┬──────┘
        │ WSS mTLS                        │ WSS mTLS
        ▼                                 ▼
    Agents in PRD                     Agents in DR
```

Three failure domains are independent:

- **Backend** is stateless except for `DashMap` caches; share the database, scale horizontally.
- **Gateway** is fully stateless. Failover is **agent-driven**, not server-driven.
- **Database** uses PostgreSQL native replication (managed or self-hosted with Patroni).

---

## Backend HA

The backend is stateless: all persistent state lives in PostgreSQL. Caches in process memory (`DashMap`) are advisory; the DB is authoritative.

### Required configuration for HA

| Environment variable | Required for HA | Notes |
|----------------------|-----------------|-------|
| `HA_MODE=true` | **Yes** | Rate limiting moves from in-memory `DashMap` to PostgreSQL counters; safe across replicas. |
| `DATABASE_URL` | **Yes** | All replicas point at the same DB (or a load-balanced read-write endpoint). |
| `JWT_SECRET` | **Yes** | Must be **identical** across replicas; JWTs signed by replica A must validate on replica B. |
| `JWT_ISSUER` | Yes | Must be identical across replicas. |
| `DB_POOL_SIZE` | Tune | Each replica opens its own pool. Total connections = `replicas × DB_POOL_SIZE`. Don't exceed PostgreSQL's `max_connections`. |
| `SHUTDOWN_TIMEOUT_SECS` | Recommended | Set to ~ Kubernetes `terminationGracePeriodSeconds - 5s` so drain completes before pod kill. |

See [Configuration — Backend HA](CONFIGURATION.md#high-availability).

### Replica count

For most installs, **3 replicas** is the sweet spot:

- One replica handles ~ 5000 agents and ~ 100 RPS comfortably.
- 3 replicas gives N+1 redundancy for rolling upgrades and node failures.
- `PodDisruptionBudget.minAvailable: 2` keeps service up during voluntary disruptions.

For very large installs (> 10k agents), scale to 5+ replicas and shard by load-balancer rules — there is currently no application-level sharding.

### Load-balancer health checks

| Probe | Endpoint | Frequency | Treats as unhealthy if |
|-------|----------|-----------|------------------------|
| Liveness | `/health` | 10 s | Pod fails the probe — Kubernetes restarts it |
| Readiness | `/ready` | 5 s | Pod removed from Service endpoints (DB unreachable, migrations in progress) |
| LB pool health | `/ready` | 5 s | LB stops routing new requests to the pod |

```yaml
# Helm probes (defaults)
livenessProbe:
  httpGet: { path: /health, port: 3000 }
  initialDelaySeconds: 30
  periodSeconds: 10
readinessProbe:
  httpGet: { path: /ready, port: 3000 }
  periodSeconds: 5
  failureThreshold: 3
```

### Sticky sessions

**Not required.** Every request carries a JWT or API key — any replica can serve any request. The WebSocket endpoint (`/ws`) is also stateless: subscriptions are recomputed on every connect from the user's permissions. See [WebSocket affinity](#websocket-affinity).

### Drain behavior

On `SIGTERM`:

1. Backend logs `shutdown initiated`.
2. Backend stops accepting new TCP connections (axum `with_graceful_shutdown`).
3. In-flight HTTP requests continue until completion or `SHUTDOWN_TIMEOUT_SECS`.
4. WebSocket clients receive a normal close frame; they reconnect to another replica.
5. Backend exits 0.

If `SHUTDOWN_TIMEOUT_SECS` expires, axum drops in-flight requests and returns 502 to clients. For long-running operations, ensure the timeout is generous.

---

## Gateway HA

Gateways are stateless WebSocket relays. They hold no database connection and no agent state — each agent's `(agent_id, gateway_id)` mapping is tracked by the backend in `agents.last_gateway_id` for routing.

### Why server-side gateway clustering is not needed

The gateway-failover problem is solved **on the agent side**: each agent is configured with multiple `gateway.urls` and falls over automatically. This is the same pattern Consul uses for client → server failover.

```yaml
# /etc/appcontrol/agent.yaml
gateway:
  urls:
    - wss://gateway-prd-01.example.com:4443/ws
    - wss://gateway-prd-02.example.com:4443/ws
  failover_strategy: ordered     # try first, fall through on failure
  primary_retry_secs: 300        # return to primary every 5 min once it recovers
  reconnect_interval_secs: 10
```

### Behind a TCP load balancer

If you want a single DNS entry, place a Layer-4 LB (AWS NLB, GCP TCP LB, HAProxy) in front of your gateway pool:

```yaml
gateway:
  urls:
    - wss://gateway.example.com:4443/ws       # NLB VIP
```

In this case, you do not need the multi-URL agent config. **However:**

- An NLB conceals the actual gateway pod identity; `agents.last_gateway_id` may be incorrect during a brief failover window.
- The agent does not know which pod it lands on; if the pod has a stale cert, the agent gets a TLS error and retries.
- Mixing NLB + multi-URL gives the best of both — if the NLB itself is down, agents try the individual gateway URLs.

### How many gateways

| Zone size | Recommended gateway count | Why |
|-----------|---------------------------:|-----|
| < 500 agents | 2 | N+1 redundancy |
| 500–5000 agents | 3 | Load distribution + N+1 |
| > 5000 agents | ≥ 4 | Distribute connection count |

Each gateway can handle thousands of WebSocket connections. The practical limit is the host's file-descriptor `ulimit` and memory (one connection ~ 64 KB resident).

### Maintenance workflow

1. Mark the gateway inactive in the backend:
   ```bash
   curl -X PATCH https://appcontrol.example.com/api/v1/gateways/<id> \
     -H "Authorization: Bearer $TOKEN" \
     -d '{"is_active": false}'
   ```
2. Agents detect the disconnection, fail over to the next gateway in their list.
3. Perform maintenance (kernel patch, upgrade, cert rotation).
4. Re-enable the gateway:
   ```bash
   curl -X PATCH https://appcontrol.example.com/api/v1/gateways/<id> \
     -H "Authorization: Bearer $TOKEN" \
     -d '{"is_active": true}'
   ```
5. With `failover_strategy: ordered` and `primary_retry_secs: 300`, agents return to the primary within 5 min.

---

## Database HA

AppControl supports any production-grade PostgreSQL 16 HA setup.

### Managed services (recommended)

| Provider | Service | HA option |
|----------|---------|-----------|
| AWS | RDS for PostgreSQL | Multi-AZ standby (sync), automatic failover |
| GCP | CloudSQL | HA with failover replica (sync) |
| Azure | Azure Database for PostgreSQL Flexible Server | Zone-redundant HA |
| Generic | CrunchyData PGO, Zalando Patroni | Streaming replication + Etcd consensus |

### Recommended PostgreSQL config

```ini
# postgresql.conf
max_connections = 200                # replicas × DB_POOL_SIZE + headroom
shared_buffers = 4GB                 # 25 % of host RAM
effective_cache_size = 12GB          # 75 % of host RAM
work_mem = 16MB
maintenance_work_mem = 1GB
wal_level = replica
synchronous_commit = on              # required: lose less than 1 commit on failover
synchronous_standby_names = 'replica-1'
checkpoint_timeout = 15min
max_wal_size = 4GB
archive_mode = on
archive_command = 'your-wal-archive-command'  # see BACKUP_RESTORE.md
```

### Failover handling

The backend uses a sqlx connection pool. On primary loss:

1. Open connections fail with `connection terminated by server`.
2. The pool marks connections as dead and reconnects.
3. Within `DB_CONNECT_TIMEOUT_SECS` (default 30), the pool succeeds against the new primary if the LB / DNS has switched.
4. In-flight requests return HTTP 503 until the pool recovers.

For zero-503 failover, place [PgBouncer](https://www.pgbouncer.org/) between the backend and PostgreSQL:

```
backend pods ─── (sqlx) ──▶ pgbouncer ──▶ PostgreSQL primary
```

PgBouncer absorbs the connection churn, lets you scale to 10s of backend replicas without exceeding PostgreSQL's `max_connections`, and gives you a single endpoint to reconfigure during failover.

### Read replicas

AppControl does not currently route any query to a read replica — the backend assumes the DB is read-write. Adding read-replica routing is on the roadmap (would benefit reports and audit queries). _(verify in code; was not found at audit time)_

---

## Agent reconnect semantics

Agents are designed to survive any single gateway or backend failure transparently.

### State at the agent

The agent maintains:

- **Component config snapshot** — last `UpdateConfig` from the backend, kept in memory.
- **Local scheduler** — runs checks autonomously based on the snapshot; does not poll the backend.
- **Delta tracking** — for each component, the last `exit_code` sent. A new `CheckResult` is only sent when the exit code changes.
- **Offline buffer** — sled DB at `/var/lib/appcontrol/buffer-{agent-id}`, ≤ 100 MB, FIFO rotation.

### Reconnect sequence

1. WebSocket close detected.
2. Agent enters reconnect loop:
   - Try `gateway.urls[0]`. If failure, wait `reconnect_interval_secs` and try `[1]`, etc.
   - Exponential backoff on repeated failure: 1, 2, 4, 8, 16, 32, 60 (max) seconds.
3. On successful connect, agent sends `Register { agent_id, hostname, ip_addresses, labels, version }`.
4. Agent replays the offline buffer in chronological order before resuming real-time deltas.

### Delta resume — `last_seq`

For `CommandResult` and `CheckResult` (state-changing messages), the agent stamps a monotonic `sequence_id` per connection (resets on each connect). The backend acks each message. Unacked messages older than 30 s are retransmitted (max 3 attempts).

The backend deduplicates by `(agent_id, sequence_id)` so the same operation is never recorded twice. See [Security Architecture §4](SECURITY_ARCHITECTURE.md).

### Buffer-on-disk during disconnect

While disconnected:

- Agent continues running checks.
- Each `CheckResult` is written to the local sled DB with a nanosecond-precision timestamp key.
- Buffer is FIFO bounded at 100 MB (configurable via `BUFFER_MAX_BYTES` env var _(verify in code; was not found at audit time)_).
- On reconnect, the buffer is drained in order, then the agent resumes real-time mode.

This means a 1 h disconnect on a 50-component agent at 30 s check interval produces ≈ 6000 buffered events × 200 bytes = ~1.2 MB — well under the buffer cap.

---

## WebSocket affinity

The backend exposes two WebSocket endpoints:

- `/ws` for the frontend (browsers + CLI watch operations).
- `/ws/gateway` for gateways (one persistent connection per gateway).

### Hub model — no sticky session needed

The hub keeps an in-memory `DashMap<user_id, Vec<Sender>>` of WebSocket senders. When a backend replica receives an event, it does **not** know which other replicas have subscribers; instead:

1. Replica A persists the event to PostgreSQL.
2. PostgreSQL issues a `NOTIFY` on a dedicated channel.
3. All replicas listen via `LISTEN`; each replica forwards the event to its local subscribers.

The result: a user connected to replica A receives every event regardless of which replica produced it.

### Implication for load balancing

- **Sticky session is not required** for correctness.
- **Sticky session is acceptable** if your LB does session affinity by cookie or IP — it just means the user's WebSocket stays on the same replica until that replica restarts.
- During a replica rollout, browsers reconnect to another replica automatically; in-flight operations resume because state is in the DB.

### Implication for monitoring

Track `ws_connections_active` per replica in Prometheus. Imbalance is fine over short periods but should rebalance after a few minutes of client churn.

---

## Example load balancer configurations

### HAProxy — backend (HTTP/HTTPS + WebSocket)

```haproxy
global
    log stdout format raw daemon
    maxconn 50000

defaults
    mode http
    timeout connect 5s
    timeout client  60s
    timeout server  60s
    timeout tunnel  3600s     # long for WebSocket /ws

frontend appcontrol_https
    bind *:443 ssl crt /etc/haproxy/certs/appcontrol.pem
    http-request set-header X-Forwarded-Proto https
    default_backend appcontrol_backend

backend appcontrol_backend
    balance roundrobin
    option httpchk GET /ready
    http-check expect status 200
    server backend-1 10.0.0.11:3000 check
    server backend-2 10.0.0.12:3000 check
    server backend-3 10.0.0.13:3000 check
```

### HAProxy — gateway (TCP, mTLS pass-through)

mTLS pass-through means HAProxy cannot inspect the cert — the gateway terminates TLS.

```haproxy
frontend gateway_tls
    mode tcp
    bind *:4443
    default_backend gateway_pool

backend gateway_pool
    mode tcp
    balance roundrobin
    option tcp-check
    tcp-check connect port 4443 ssl
    server gw-1 10.0.0.21:4443 check
    server gw-2 10.0.0.22:4443 check
    server gw-3 10.0.0.23:4443 check
```

### nginx — backend (with WebSocket upgrade)

```nginx
upstream appcontrol_backend {
    least_conn;
    server backend-1:3000 max_fails=3 fail_timeout=10s;
    server backend-2:3000 max_fails=3 fail_timeout=10s;
    server backend-3:3000 max_fails=3 fail_timeout=10s;
}

map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl http2;
    server_name appcontrol.example.com;
    ssl_certificate     /etc/nginx/certs/appcontrol.crt;
    ssl_certificate_key /etc/nginx/certs/appcontrol.key;

    location /api/ {
        proxy_pass http://appcontrol_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
        proxy_read_timeout 60s;
    }

    location /ws {
        proxy_pass http://appcontrol_backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade    $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_set_header Host       $host;
        proxy_read_timeout 3600s;
    }

    location / {
        proxy_pass http://frontend:8080;
    }
}
```

---

## Example Helm values overlay

```yaml
# ha-values.yaml — overlay over production-values.yaml
backend:
  replicaCount: 3
  env:
    HA_MODE: "true"
    SHUTDOWN_TIMEOUT_SECS: "25"
  resources:
    requests: { cpu: "1",   memory: 1Gi }
    limits:   { cpu: "2",   memory: 2Gi }
  affinity:
    podAntiAffinity:
      preferredDuringSchedulingIgnoredDuringExecution:
        - weight: 100
          podAffinityTerm:
            labelSelector:
              matchLabels:
                app.kubernetes.io/component: backend
            topologyKey: kubernetes.io/hostname

gateway:
  replicaCount: 2
  affinity:
    podAntiAffinity:
      preferredDuringSchedulingIgnoredDuringExecution:
        - weight: 100
          podAffinityTerm:
            labelSelector:
              matchLabels:
                app.kubernetes.io/component: gateway
            topologyKey: topology.kubernetes.io/zone   # spread across AZs

frontend:
  replicaCount: 2

podDisruptionBudget:
  enabled: true
  backend:  { minAvailable: 2 }
  gateway:  { minAvailable: 1 }
  frontend: { minAvailable: 1 }

# Disable in-cluster PostgreSQL — use managed
postgresql:
  enabled: false

# External Service for gateway agents
gatewayService:
  type: LoadBalancer
  loadBalancerSourceRanges:
    - 10.0.0.0/8        # restrict to internal networks
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-type: nlb   # Layer 4 NLB
```

Apply:

```bash
helm upgrade appcontrol ./helm/appcontrol \
  --namespace appcontrol \
  --values production-values.yaml \
  --values ha-values.yaml
```

---

## What survives, what does not

| Failure | Backend HA (3 replicas) | Gateway HA (2 gateways) | Database HA (managed) | What still works | What is degraded |
|---------|:-----------------------:|:-----------------------:|:---------------------:|------------------|------------------|
| 1 backend pod | yes | n/a | n/a | All HTTP / WS; LB removes failed pod within 5 s | Brief 5xx burst (< 100 ms typical) |
| All backend pods | no | n/a | n/a | Nothing; agents stay connected to the gateway but commands fail | 100 % control plane outage |
| 1 gateway pod | n/a | yes | n/a | All agents fail over to other gateway within 10–60 s | Agents using the failed gateway briefly UNREACHABLE |
| All gateways | n/a | no | n/a | Agents continue running checks locally; backend cannot reach them | Operations cannot execute |
| PostgreSQL primary | yes | n/a | yes | Backend retries; new primary serves traffic within ~ 30 s | 30 s of 503 |
| PostgreSQL primary + replica | yes | n/a | no | Nothing; backend cannot start | Total outage; restore from backup |
| 1 AZ (with anti-affinity across AZs) | yes | yes | yes (managed multi-AZ) | All HTTP / WS; agents in other AZs fine | Agents in the lost AZ unreachable until AZ recovers |
| Network partition between AZs | partial | partial | depends on managed-DB topology | Each AZ continues serving its local agents | See DR §5 |
| Network from agents to gateway | n/a | n/a | n/a | Agent continues local checks, buffers events | Agents UNREACHABLE in UI; events replayed on reconnect |
| Agent process killed | yes | yes | yes | Managed processes survive (double-fork + setsid) | Component shows UNREACHABLE until agent restarts |
| Agent host rebooted | yes | yes | yes | Agent reconnects on boot via systemd / service | Components UNREACHABLE during reboot window |

For scenarios beyond this matrix (regional outage, doomsday), see [Disaster Recovery](DISASTER_RECOVERY.md).

---

## Verification

After deploying HA, run this checklist to confirm topology.

```bash
# 3 backend replicas, all ready
kubectl get pods -n appcontrol -l app.kubernetes.io/component=backend
# Expect: 3 pods, all Running, Ready 1/1

# Anti-affinity spread across nodes
kubectl get pods -n appcontrol -l app.kubernetes.io/component=backend -o wide
# Expect: each replica on a different NODE

# 2 gateways across zones
kubectl get pods -n appcontrol -l app.kubernetes.io/component=gateway -o jsonpath='{.items[*].spec.nodeName}'

# PDB
kubectl get pdb -n appcontrol
# Expect: backend minAvailable=2, gateway minAvailable=1

# Database HA
psql $DATABASE_URL -c "SELECT pg_is_in_recovery();"   # primary: f
psql $REPLICA_URL -c "SELECT pg_is_in_recovery();"   # replica: t

# HA mode in backend
kubectl exec -n appcontrol deploy/appcontrol-backend -- env | grep HA_MODE
# Expect: HA_MODE=true
```

Once verified, run a chaos drill: delete one backend pod, watch the LB shed traffic, verify a CLI `appctl status` succeeds without retry. Repeat for gateway, then for the database (managed-failover button).

---

## Reference

- [PRODUCTION_DEPLOYMENT.md](PRODUCTION_DEPLOYMENT.md) — initial deployment
- [DISASTER_RECOVERY.md](DISASTER_RECOVERY.md) — beyond single-pod failures
- [CAPACITY_PLANNING.md](CAPACITY_PLANNING.md) — sizing the cluster
- [Security Architecture §5](SECURITY_ARCHITECTURE.md) — Multi-Gateway Failover details
