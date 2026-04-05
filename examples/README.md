# Example Application Maps

These example configurations demonstrate how to model real-world IT systems as AppControl dependency graphs (DAGs).

## Available Examples

### 1. Cluster Demo (`cluster-demo.json`)

Demonstrates the cluster feature with multiple components configured as multi-node clusters.

```
PostgreSQL Cluster (×3)
├── Redis Cluster (×6)
├── Kafka Brokers (×3)
    └── Backend API
        └── API Gateway (×2)
            └── Frontend
```

**Key concepts demonstrated:**
- `cluster_size`: Number of nodes in the cluster
- `cluster_nodes`: List of server hostnames/IPs
- Visual rendering with stacked cards and node count badge
- Commands execute on the primary node (first in the list)

### 2. Metrics Demo (`metrics-demo.json`)

Demonstrates check commands that output operational metrics for monitoring and dashboards.

```
PostgreSQL Database → connections, replication lag, cache hit ratio
Redis Cache → memory usage, key count, hit rate
Kafka Broker (×3) → topics, partitions, consumer lag
API Gateway (×2) → requests/min, error rate, p99 latency
Backend API → active users, orders, response time
Background Workers → queue depth, jobs completed, failures
Frontend → requests/sec, bandwidth, cache ratio
```

**Key concepts demonstrated:**
- Pure JSON output (entire stdout is JSON)
- Mixed output (logs + JSON on last line)
- METRICS marker format (`---METRICS---` followed by JSON)
- Business metrics (active users, orders)
- Infrastructure metrics (connections, memory, latency)

### 3. Three-Tier Web Application (`three-tier-webapp.json`)

Classic architecture with load balancer, application servers, and database cluster.

```
HAProxy Load Balancer
├── App Server 1
│   ├── PostgreSQL Primary
│   └── Redis Cache (weak)
├── App Server 2
│   ├── PostgreSQL Primary
│   └── Redis Cache (weak)
└── Batch Processor
    └── PostgreSQL Primary

PostgreSQL Standby → PostgreSQL Primary
```

**Key concepts demonstrated:**
- Strong vs weak dependencies (Redis is weak — app degrades but doesn't fail)
- Parallel start within DAG levels
- Database replication monitoring (standby integrity check)
- Infrastructure checks (disk space)

### 2. Microservices E-Commerce (`microservices-ecommerce.json`)

Modern microservices with API gateway, message broker, and independent services.

```
Frontend (React SPA)
└── API Gateway (Kong)
    ├── Order Service → PostgreSQL (Orders) + RabbitMQ + Redis
    ├── User Service → PostgreSQL (Users) + Redis
    ├── Catalog Service → MongoDB (Catalog)
    ├── Payment Service → Order Service + User Service
    └── Notification Service → RabbitMQ + User Service (weak)
```

**Key concepts demonstrated:**
- Complex DAG with shared infrastructure layer
- Failure isolation (catalog failure doesn't impact orders)
- Service-per-database pattern
- Message broker dependencies
- 12 components across 4 DAG levels

### 3. Core Banking System (`banking-core-system.json`)

Enterprise banking with mainframe integration, DR switchover, and scheduler integration.

```
F5 Load Balancer
└── Internet Banking Portal
    ├── WebSphere App Server 1 → Oracle RAC 1 + MQ Series
    └── WebSphere App Server 2 → Oracle RAC 1 + MQ Series

Nightly Batch Jobs → Batch Controller (Control-M) → Oracle RAC 1
Oracle RAC Node 2 → Oracle RAC Node 1
```

**Key concepts demonstrated:**
- DR switchover configuration (Paris → Lyon)
- Protected components (Oracle RAC Node 1 cannot be rebuilt without approval)
- Scheduler integration (Control-M triggers via API)
- DORA compliance (all operations audited)
- Integrity checks (DataGuard, ASM)
- Long timeout values for enterprise middleware

## Windows Examples

The default examples use Unix shell commands (`test -f`, `touch`, `rm -f`) which don't work on Windows CMD. Windows-compatible versions are provided:

| Linux/macOS | Windows |
|-------------|---------|
| `metrics-demo.json` | `metrics-demo-windows.json` |
| `three-tier-webapp.json` | `three-tier-webapp-windows.json` |

**Windows command equivalents:**

| Unix | Windows CMD |
|------|-------------|
| `touch file` | `echo.> file` |
| `test -f file` | `if exist file (exit /b 0) else (exit /b 1)` |
| `rm -f file` | `del /f /q file 2>nul` |
| `mkdir -p dir` | `if not exist dir mkdir dir` |

Windows examples use `%TEMP%\appcontrol\` instead of `/tmp/appcontrol/` for flag files.

## Getting the Examples

### From the latest release (recommended)

```bash
# Download the examples archive from the latest release
gh release download --repo fredericcarre/appcontrol --pattern 'examples.tar.gz'
tar xzf examples.tar.gz
```

### From a git clone

```bash
git clone https://github.com/fredericcarre/appcontrol.git
# Examples are in appcontrol/examples/
```

## How to Import

### Via API

```bash
curl -X POST http://localhost:3000/api/v1/apps/import \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d @examples/three-tier-webapp.json
```

### Via CLI

```bash
appctl import examples/three-tier-webapp.json --site-id YOUR_SITE_UUID
```

### Via UI

1. Go to **Applications** → **Import**
2. Upload the JSON file
3. Select the target site
4. Review the map preview
5. Click **Import**

## Creating Your Own Maps

Use these examples as templates. Key fields for each component:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique name within the application |
| `component_type` | Yes | database, application, webserver, middleware, cache, batch, loadbalancer, gateway, microservice |
| `host` | Yes | Target hostname (must match an agent) |
| `check_cmd` | Yes | Level 1 health check (exit 0 = healthy) |
| `start_cmd` | Yes | Command to start the component |
| `stop_cmd` | Yes | Command to stop the component |
| `check_interval_secs` | No | Health check frequency (default: 30) |
| `start_timeout_secs` | No | Max wait for start (default: 120) |
| `stop_timeout_secs` | No | Max wait for stop (default: 120) |
| `integrity_check_cmd` | No | Level 2 data integrity check |
| `infra_check_cmd` | No | Level 3 infrastructure check |
| `rebuild_cmd` | No | Rebuild command for diagnostic recovery |
| `protected` | No | If true, rebuild requires explicit approval |
| `position` | No | Map canvas position `{x, y}` |

Dependencies use `from` (dependent) → `to` (dependency) with type `strong` or `weak`.

### Application Reference Components (Synthetic Views)

You can create a component that references another application as a "synthetic" aggregate view. This is useful for:
- Combining multiple applications into a parent "super-application"
- Creating logical groupings (e.g., "All Production Services")
- Building tiered architectures where one app depends on another

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique name within the application |
| `component_type` | Yes | Must be `application` |
| `host` | Yes | Set to `aggregate` (no specific host) |
| `referenced_app_name` | Yes* | Name of the target application (resolved at import) |
| `referenced_app_id` | Yes* | UUID of the target application (alternative to name) |
| `check_cmd` | Yes | `@app:check` (internal command) |
| `start_cmd` | Yes | `@app:start` (internal command) |
| `stop_cmd` | Yes | `@app:stop` (internal command) |

*Either `referenced_app_name` or `referenced_app_id` is required. Using the name is recommended as it's more readable and portable across environments.

**Example:**

```json
{
  "name": "payment-system",
  "display_name": "Payment System",
  "component_type": "application",
  "host": "aggregate",
  "referenced_app_name": "Payment Gateway",
  "description": "Synthetic view of the Payment Gateway application",
  "check_cmd": "@app:check",
  "start_cmd": "@app:start",
  "stop_cmd": "@app:stop",
  "icon": "folder",
  "position": { "x": 400, "y": 200 }
}
```

**Internal Commands (`@app:` prefix):**
Commands prefixed with `@app:` are interpreted by the backend, not executed on an agent. The backend uses the `referenced_app_id` (resolved from `referenced_app_name` if needed) to:
- `@app:check` — Query aggregate state of all components in the referenced app
- `@app:start` — Trigger sequenced start of the referenced app (respecting DAG)
- `@app:stop` — Trigger sequenced stop of the referenced app (reverse DAG)

**Behavior:**
- The component's state reflects the aggregate state of the referenced app
- Start/stop operations cascade to all components in the referenced app (respecting its DAG)
- Referenced applications **cannot be deleted** while they are referenced by another app
- Deleting the referencing component removes the reference and allows deletion

**Deletion Protection:**
When you try to delete an application that is referenced by another app's component, the deletion will be blocked with an error listing the referencing applications. You must first remove the synthetic component(s) from those applications.

### Cluster Components

Mark a component as a multi-node cluster for visual representation and documentation:

| Field | Required | Description |
|-------|----------|-------------|
| `cluster_size` | No | Number of nodes (minimum 2) |
| `cluster_nodes` | No | Array of server hostnames/IPs |

**Example:**

```json
{
  "name": "postgres-cluster",
  "display_name": "PostgreSQL Cluster",
  "component_type": "database",
  "host": "pg-primary.prod",
  "cluster_size": 3,
  "cluster_nodes": ["pg-primary.prod", "pg-replica1.prod", "pg-replica2.prod"],
  "check_cmd": "pg_isready -h pg-primary.prod",
  "start_cmd": "systemctl start postgresql",
  "stop_cmd": "systemctl stop postgresql"
}
```

**Behavior:**
- Visual rendering shows stacked cards with a `×N` badge
- Commands execute on the `host` specified (typically the primary node)
- The first entry in `cluster_nodes` is considered the primary
- Future: Health aggregation across all nodes

### Metrics Output from Check Commands

Check commands can output JSON to provide operational metrics displayed in the UI and stored for time-series analysis.

**Supported Formats:**

1. **Pure JSON** — Entire stdout is valid JSON
   ```bash
   echo '{"connections": 42, "lag_ms": 150}'
   ```

2. **Mixed Output** — JSON on the last line, logs before
   ```bash
   echo "PostgreSQL is healthy"
   echo "Checking replication..."
   echo '{"connections": 42, "lag_ms": 150}'
   ```

3. **METRICS Marker** — JSON after `---METRICS---`
   ```bash
   echo "Running health check..."
   echo "All systems operational"
   echo "---METRICS---"
   echo '{"connections": 42, "lag_ms": 150}'
   ```

4. **Legacy Tags** — JSON wrapped in `<appcontrol>` tags
   ```bash
   echo "<appcontrol>{\"connections\": 42}</appcontrol>"
   ```

**Metric Types:**

| Type | Examples |
|------|----------|
| **Infrastructure** | `connections`, `memory_mb`, `cpu_pct`, `disk_used_gb` |
| **Performance** | `latency_ms`, `p99_latency_ms`, `requests_per_sec` |
| **Queue/Batch** | `queue_depth`, `consumer_lag`, `jobs_pending` |
| **Business** | `active_users`, `orders_today`, `transactions` |
| **Health** | `error_rate`, `cache_hit_ratio`, `replication_lag_ms` |

**API Access:**
```bash
# Latest metrics
GET /api/v1/components/:id/metrics

# Historical data (last 100 check events)
GET /api/v1/components/:id/metrics/history
```

**Real-World Example (PostgreSQL):**

```bash
#!/bin/bash
# check_postgres.sh - Level 1 health check with metrics

# Check if PostgreSQL is accepting connections
if ! pg_isready -h localhost -p 5432 > /dev/null 2>&1; then
    echo "PostgreSQL is not responding"
    exit 1
fi

# Gather metrics
CONNECTIONS=$(psql -t -c "SELECT count(*) FROM pg_stat_activity" | tr -d ' ')
LAG_BYTES=$(psql -t -c "SELECT COALESCE(pg_wal_lsn_diff(pg_current_wal_lsn(), replay_lsn), 0) FROM pg_stat_replication LIMIT 1" | tr -d ' ')
CACHE_HIT=$(psql -t -c "SELECT ROUND(sum(heap_blks_hit) / NULLIF(sum(heap_blks_hit) + sum(heap_blks_read), 0), 3) FROM pg_statio_user_tables" | tr -d ' ')

# Output status and metrics
echo "PostgreSQL is healthy"
echo "{\"connections\": $CONNECTIONS, \"replication_lag_bytes\": ${LAG_BYTES:-0}, \"cache_hit_ratio\": ${CACHE_HIT:-0}}"
exit 0
```
