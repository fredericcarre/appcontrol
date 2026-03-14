# Example Application Maps

These example configurations demonstrate how to model real-world IT systems as AppControl dependency graphs (DAGs).

## Available Examples

### 1. Three-Tier Web Application (`three-tier-webapp.json`)

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
