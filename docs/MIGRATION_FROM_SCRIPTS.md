# Migration Guide: From Scripts & Templates to AppControl

Step-by-step guide for teams migrating from shell scripts, XL Release templates, or scheduler-managed restarts to AppControl.

## Overview

Most teams don't start from zero — they already have operational procedures encoded in scripts, templates, or schedulers. This guide helps you migrate progressively without disrupting existing operations.

### Migration Phases

```
Phase 1: Model      →  Import your application topology
Phase 2: Observe    →  Deploy agents in advisory mode
Phase 3: Validate   →  Compare existing procedures with AppControl's DAG
Phase 4: Integrate  →  Existing tools call AppControl instead of scripts
Phase 5: Operate    →  AppControl handles operations natively
```

---

## Phase 1: Model Your Application

### Step 1.1: Identify Components

List all processes/services that make up your application:

| Component | Host | Type | Start Command | Stop Command | Check Command |
|-----------|------|------|---------------|-------------|---------------|
| PostgreSQL | db-01.prod | database | `systemctl start postgresql` | `systemctl stop postgresql` | `pg_isready -h localhost` |
| Redis | cache-01.prod | cache | `systemctl start redis` | `systemctl stop redis` | `redis-cli ping` |
| API Server | app-01.prod | application | `/opt/api/start.sh` | `/opt/api/stop.sh` | `curl -f http://localhost:8080/health` |
| Web Frontend | web-01.prod | webserver | `systemctl start nginx` | `systemctl stop nginx` | `curl -f http://localhost:80/` |

### Step 1.2: Identify Dependencies

Map which components depend on which (who must start first):

| Component | Depends On |
|-----------|-----------|
| API Server | PostgreSQL, Redis |
| Web Frontend | API Server |
| PostgreSQL | *(none)* |
| Redis | *(none)* |

### Step 1.3: Import into AppControl

**Option A: YAML Import**

Create a YAML file matching AppControl's import format:

```yaml
application:
  name: billing-prod
  description: Production billing application
  components:
    - name: PostgreSQL
      type: database
      host: db-01.prod
      start_cmd: "systemctl start postgresql"
      stop_cmd: "systemctl stop postgresql"
      check_cmd: "pg_isready -h localhost"
      check_interval_seconds: 30
    - name: Redis
      type: cache
      host: cache-01.prod
      start_cmd: "systemctl start redis"
      stop_cmd: "systemctl stop redis"
      check_cmd: "redis-cli ping"
      check_interval_seconds: 15
    - name: API Server
      type: application
      host: app-01.prod
      start_cmd: "/opt/api/start.sh"
      stop_cmd: "/opt/api/stop.sh"
      check_cmd: "curl -sf http://localhost:8080/health"
      start_timeout_seconds: 120
    - name: Web Frontend
      type: webserver
      host: web-01.prod
      start_cmd: "systemctl start nginx"
      stop_cmd: "systemctl stop nginx"
      check_cmd: "curl -sf http://localhost:80/"
  dependencies:
    - from: API Server
      to: PostgreSQL
    - from: API Server
      to: Redis
    - from: Web Frontend
      to: API Server
```

Upload via UI (Import page) or API:
```bash
curl -X POST https://appcontrol.example.com/api/v1/import/yaml \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/x-yaml" \
  --data-binary @billing-prod.yaml
```

**Option B: UI Wizard**

Use the onboarding wizard at `/onboarding` to create components visually with drag-and-drop dependency mapping.

**Option C: API Calls**

Create components and dependencies programmatically:
```bash
# Create application
APP_ID=$(curl -s -X POST /api/v1/apps \
  -d '{"name":"billing-prod","site_id":"..."}' | jq -r '.id')

# Create components
PG_ID=$(curl -s -X POST /api/v1/apps/$APP_ID/components \
  -d '{"name":"PostgreSQL","component_type":"database","host":"db-01.prod",...}' | jq -r '.id')

# Create dependencies
curl -X POST /api/v1/apps/$APP_ID/dependencies \
  -d '{"from_component_id":"'$API_ID'","to_component_id":"'$PG_ID'"}'
```

---

## Phase 2: Observe (Advisory Mode)

Deploy AppControl agents on your servers in **advisory mode**. Agents will:
- Run health check commands (`check_cmd`) on their normal schedule
- Report component state (RUNNING/STOPPED/FAILED) to the backend
- **NOT** execute any start/stop/rebuild commands

### Step 2.1: Install Agents

```bash
# On each server, install the agent
curl -LO https://releases.appcontrol.io/agent/latest/appcontrol-agent
chmod +x appcontrol-agent
mv appcontrol-agent /usr/local/bin/
```

### Step 2.2: Configure Advisory Mode

Create `/etc/appcontrol/agent.yaml`:

```yaml
agent:
  id: auto
mode: advisory    # <-- observation only, no command execution
gateway:
  urls:
    - wss://gateway.example.com:4443/ws
tls:
  enabled: true
  cert_file: /etc/appcontrol/agent.pem
  key_file: /etc/appcontrol/agent-key.pem
  ca_file: /etc/appcontrol/ca.pem
```

Or via environment variable:
```bash
export AGENT_MODE=advisory
```

### Step 2.3: Enroll and Start

```bash
# Enroll agent with gateway
appcontrol-agent --enroll https://gateway.example.com:4443 --token YOUR_TOKEN

# Start the agent
systemctl start appcontrol-agent
```

### Step 2.4: Verify Observation

Open the AppControl UI → Map View. You should see components with real-time health states, all driven by check commands. The agents are monitoring but not controlling.

---

## Phase 3: Validate

### Step 3.1: Compare Existing Procedures

If you have an XL Release template or script that defines a restart order, validate it:

```bash
# Your current script restarts in this order:
# 1. PostgreSQL  2. Redis  3. API Server  4. Web Frontend

curl -X POST https://appcontrol.example.com/api/v1/apps/$APP_ID/validate-sequence \
  -H "X-API-Key: $AC_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "sequence": ["PostgreSQL", "Redis", "API Server", "Web Frontend"],
    "operation": "start"
  }'
```

**If valid:**
```json
{"valid": true, "conflicts": []}
```

**If conflicts found:**
```json
{
  "valid": false,
  "conflicts": [
    {
      "dependent": "API Server",
      "dependency": "Redis",
      "message": "'API Server' depends on 'Redis' but starts at position 1 while Redis is at position 2"
    }
  ]
}
```

Fix the conflict in AppControl's DAG (or in your script, if the script was actually correct).

### Step 3.2: Get the Correct Execution Plan

```bash
curl -s https://appcontrol.example.com/api/v1/apps/$APP_ID/plan?operation=start \
  -H "X-API-Key: $AC_API_KEY" | jq '.plan.levels'
```

This shows exactly what AppControl would do, level by level.

### Step 3.3: Dry Run

Test the actual start logic without executing commands:

```bash
appctl start billing-prod --dry-run
```

---

## Phase 4: Integrate

### Step 4.1: Replace Scripts with AppControl Calls

**Before (shell script):**
```bash
#!/bin/bash
ssh db-01.prod "systemctl start postgresql"
sleep 10
ssh cache-01.prod "systemctl start redis"
sleep 5
ssh app-01.prod "/opt/api/start.sh"
sleep 15
ssh web-01.prod "systemctl start nginx"
```

**After (AppControl CLI):**
```bash
#!/bin/bash
appctl start billing-prod --wait --timeout 300
```

**After (AppControl API from another tool):**
```bash
curl -X POST https://appcontrol.example.com/api/v1/orchestration/apps/$APP_ID/start \
  -H "X-API-Key: $AC_API_KEY"
curl -s https://appcontrol.example.com/api/v1/orchestration/apps/$APP_ID/wait-running?timeout=300 \
  -H "X-API-Key: $AC_API_KEY"
```

### Step 4.2: Replace XL Release Templates

Replace multi-step restart templates with a single API call. See the [Integration Cookbook](INTEGRATION_COOKBOOK.md) for XL Release-specific recipes.

### Step 4.3: Replace Scheduler Custom Scripts

Scheduler jobs now call `appctl` or the REST API. Dependencies are managed by AppControl, not by scheduler job chains. See the [Integration Cookbook](INTEGRATION_COOKBOOK.md) for Control-M/AutoSys/Dollar Universe recipes.

---

## Phase 5: Full Operation

### Step 5.1: Switch Agents to Active Mode

Update agent configuration:

```yaml
mode: active    # <-- full control mode
```

Or:
```bash
export AGENT_MODE=active
systemctl restart appcontrol-agent
```

### Step 5.2: Decommission Old Scripts

- Archive (don't delete) old scripts in version control
- Remove restart templates from XL Release
- Simplify scheduler job chains to single AppControl calls
- Update runbooks to reference AppControl instead of manual procedures

### Step 5.3: Enable Advanced Features

Now that AppControl is the operational authority:

- **DR Switchover**: Configure multi-site failover
- **3-Level Diagnostics**: Add integrity and infrastructure check commands
- **Approval Workflows**: Enable 4-eyes principle for production operations
- **DORA Reporting**: Access compliance metrics and audit trails
- **Webhooks**: Route state change alerts to PagerDuty/ServiceNow

---

## Tips for a Smooth Migration

### Identifying Hidden Dependencies

Dependencies aren't always documented. Look for clues in:

- **Sleep commands** in scripts: `sleep 30` between starts often hides a dependency
- **Retry loops**: "Start X, check if Y is ready, retry" means X depends on Y
- **Error messages**: "Connection refused to database" in logs means the service depends on the database
- **Startup order in systemd**: `After=postgresql.service` in unit files
- **Network connections**: `netstat -tlnp` shows what each process connects to at startup

### Handling Components Without Check Commands

If a component has no obvious health check command:

| Component Type | Suggested check_cmd |
|---------------|-------------------|
| Systemd service | `systemctl is-active service-name` |
| Process | `pgrep -f process-name` |
| TCP port | `nc -z localhost PORT` or `bash -c '</dev/tcp/localhost/PORT'` |
| HTTP service | `curl -sf http://localhost:PORT/health` |
| Database | `pg_isready` / `mysqladmin ping` / `mongo --eval 'db.runCommand("ping")'` |

### Common Pitfalls

1. **Don't try to model everything at once.** Start with one critical application, prove the value, then expand.
2. **Don't skip advisory mode.** Running in observation before taking control prevents surprises.
3. **Don't delete old scripts immediately.** Keep them archived as fallback for the first few weeks.
4. **Don't ignore timeouts.** Set realistic `start_timeout_seconds` and `stop_timeout_seconds` based on observed behavior.
5. **Don't forget optional components.** Mark non-critical components as `is_optional: true` so they don't block the whole application start.
