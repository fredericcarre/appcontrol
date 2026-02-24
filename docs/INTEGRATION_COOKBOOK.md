# Integration Cookbook

Concrete, copy-paste recipes for integrating AppControl with common enterprise tools.

## Prerequisites

All recipes assume:
- AppControl backend is running at `https://appcontrol.example.com`
- You have an API key: `ac_your-api-key-here`
- The target application is already modeled in AppControl with ID `APP_ID`

### Create an API Key

```bash
# Via CLI
appctl api-key create --name "control-m-integration" --scopes "operate"

# Via API
curl -X POST https://appcontrol.example.com/api/v1/api-keys \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "control-m-integration", "scopes": ["operate"]}'
```

Save the returned key — it's shown only once.

---

## 1. Control-M Integration

### Start an Application from a Control-M Job

Create a Control-M job that calls the AppControl CLI:

```
Job Name: START_BILLING_APP
Command: /usr/local/bin/appctl start billing-prod --wait --timeout 300
Run As: appcontrol-svc
Exit Codes: 0=OK, 1=FAIL, 2=TIMEOUT
```

**AppControl CLI exit codes:**
| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Failure |
| 2 | Timeout |
| 3 | Auth error |
| 4 | Not found |
| 5 | Permission denied |

### Check Application Status Before a Job

```bash
#!/bin/bash
# pre-condition.sh — use as Control-M pre-condition
STATUS=$(appctl status billing-prod --format short)
if [ "$STATUS" != "RUNNING" ]; then
  echo "Application not running, aborting job"
  exit 1
fi
exit 0
```

### Stop After a Batch Window

```
Job Name: STOP_BILLING_APP
Command: /usr/local/bin/appctl stop billing-prod --wait --timeout 180
Condition In: END_OF_BATCH_WINDOW
```

---

## 2. AutoSys Integration

### JIL Definition for AppControl Start

```
insert_job: start_billing_app
job_type: CMD
command: /usr/local/bin/appctl start billing-prod --wait --timeout 300
machine: appcontrol-host
owner: autosys_svc
permission: gx,ge
alarm_if_fail: 1
std_out_file: /var/log/autosys/appcontrol_start.log
std_err_file: /var/log/autosys/appcontrol_start.err
```

### Conditional Start Based on AppControl State

```bash
#!/bin/bash
# autosys_wrapper.sh
APP_ID="billing-prod"
ACTION="$1"

# Check current state via API
STATE=$(curl -s -H "X-API-Key: $AC_API_KEY" \
  "https://appcontrol.example.com/api/v1/orchestration/apps/$APP_ID/status" \
  | jq -r '.all_running')

if [ "$ACTION" = "start" ] && [ "$STATE" = "true" ]; then
  echo "Already running, skipping"
  exit 0
fi

appctl "$ACTION" "$APP_ID" --wait --timeout 300
exit $?
```

---

## 3. Dollar Universe Integration

### Creating a Dollar Universe Task

```bash
# DU task script
#!/bin/bash
# u_start_app.sh — Dollar Universe uproc

AC_API_KEY="${AC_API_KEY}"
APP_ID="${U_APP_ID}"
TIMEOUT="${U_TIMEOUT:-300}"

# Start via REST API (Dollar Universe prefers API calls over CLI)
RESPONSE=$(curl -s -w "\n%{http_code}" \
  -X POST "https://appcontrol.example.com/api/v1/orchestration/apps/$APP_ID/start" \
  -H "X-API-Key: $AC_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"dry_run": false}')

HTTP_CODE=$(echo "$RESPONSE" | tail -1)
if [ "$HTTP_CODE" != "200" ]; then
  echo "Start failed with HTTP $HTTP_CODE"
  exit 1
fi

# Wait for completion
WAIT_RESPONSE=$(curl -s -w "\n%{http_code}" \
  "https://appcontrol.example.com/api/v1/orchestration/apps/$APP_ID/wait-running?timeout=$TIMEOUT" \
  -H "X-API-Key: $AC_API_KEY")

STATUS=$(echo "$WAIT_RESPONSE" | head -1 | jq -r '.status')
case "$STATUS" in
  running) exit 0 ;;
  timeout) exit 2 ;;
  failed)  exit 1 ;;
  *)       exit 1 ;;
esac
```

---

## 4. XL Release (Digital.ai Release) Integration

### HTTP Task: Start Application

In your XL Release template, add an **HTTP Request** task:

| Field | Value |
|-------|-------|
| URL | `https://appcontrol.example.com/api/v1/orchestration/apps/${app_id}/start` |
| Method | POST |
| Headers | `X-API-Key: ${api_key}`, `Content-Type: application/json` |
| Body | `{"dry_run": false}` |

### HTTP Task: Wait for Running

Add a second HTTP Request task:

| Field | Value |
|-------|-------|
| URL | `https://appcontrol.example.com/api/v1/orchestration/apps/${app_id}/wait-running?timeout=300` |
| Method | GET |
| Headers | `X-API-Key: ${api_key}` |
| Expected Status | 200 |

### Validate Your XL Release Sequence

Before migrating, validate that your XL Release template's restart order matches AppControl's DAG:

```bash
curl -X POST "https://appcontrol.example.com/api/v1/apps/$APP_ID/validate-sequence" \
  -H "X-API-Key: $AC_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "sequence": ["redis-cache", "postgres-db", "api-server", "web-frontend"],
    "operation": "start"
  }'
```

Response:
```json
{
  "valid": true,
  "conflicts": [],
  "expected_order": [["redis-cache", "postgres-db"], ["api-server"], ["web-frontend"]]
}
```

If there are conflicts, the response tells you exactly what's wrong:
```json
{
  "valid": false,
  "conflicts": [
    {
      "type": "dependency_order_violation",
      "dependent": "api-server",
      "dependency": "postgres-db",
      "message": "'api-server' depends on 'postgres-db' but starts at position 0 while postgres-db is at position 1"
    }
  ]
}
```

### Replace XL Release Restart Templates

**Before** (XL Release manages dependencies):
```
Phase: Restart Application
  Task 1: Start Redis (SSH)
  Task 2: Start PostgreSQL (SSH)
  Task 3: Wait 30s
  Task 4: Start API Server (SSH)
  Task 5: Wait 10s
  Task 6: Start Web Frontend (SSH)
  Task 7: Health check (HTTP)
```

**After** (AppControl manages dependencies):
```
Phase: Restart Application
  Task 1: POST /api/v1/orchestration/apps/{id}/start
  Task 2: GET /api/v1/orchestration/apps/{id}/wait-running?timeout=300
```

---

## 5. Jenkins Integration

### Post-Deploy Restart via Jenkinsfile

```groovy
pipeline {
    agent any
    environment {
        AC_API_KEY = credentials('appcontrol-api-key')
        AC_URL = 'https://appcontrol.example.com'
        APP_ID = 'your-app-id'
    }
    stages {
        stage('Deploy') {
            steps {
                // Your deployment steps here
                sh 'helm upgrade myapp ./helm/myapp'
            }
        }
        stage('Restart via AppControl') {
            steps {
                sh """
                    curl -f -X POST ${AC_URL}/api/v1/orchestration/apps/${APP_ID}/start \
                      -H "X-API-Key: ${AC_API_KEY}" \
                      -H "Content-Type: application/json" \
                      -d '{"dry_run": false}'
                """
                // Wait for all components to be running
                sh """
                    STATUS=\$(curl -s ${AC_URL}/api/v1/orchestration/apps/${APP_ID}/wait-running?timeout=300 \
                      -H "X-API-Key: ${AC_API_KEY}" | jq -r '.status')
                    if [ "\$STATUS" != "running" ]; then
                      echo "Application failed to start: \$STATUS"
                      exit 1
                    fi
                """
            }
        }
    }
}
```

---

## 6. GitLab CI Integration

### .gitlab-ci.yml

```yaml
restart_application:
  stage: post-deploy
  image: curlimages/curl:latest
  variables:
    AC_URL: "https://appcontrol.example.com"
    APP_ID: "your-app-id"
  script:
    - |
      # Trigger restart
      curl -f -X POST "$AC_URL/api/v1/orchestration/apps/$APP_ID/start" \
        -H "X-API-Key: $AC_API_KEY" \
        -H "Content-Type: application/json" \
        -d '{"dry_run": false}'

      # Wait for running
      STATUS=$(curl -s "$AC_URL/api/v1/orchestration/apps/$APP_ID/wait-running?timeout=300" \
        -H "X-API-Key: $AC_API_KEY" | jq -r '.status')

      if [ "$STATUS" != "running" ]; then
        echo "Restart failed: $STATUS"
        exit 1
      fi
      echo "Application is running"
  only:
    - main
```

---

## 7. PagerDuty / Opsgenie Integration

### Webhook Notification Setup

Configure AppControl to send webhooks on FAILED state transitions:

```bash
# Subscribe to failure events via API
curl -X POST "https://appcontrol.example.com/api/v1/notification-preferences" \
  -H "Authorization: Bearer $JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "event_type": "failure",
    "channel": "webhook",
    "webhook_url": "https://events.pagerduty.com/v2/enqueue",
    "webhook_headers": {
      "Content-Type": "application/json"
    }
  }'
```

### PagerDuty Webhook Payload Transform

AppControl webhook payload:
```json
{
  "event_type": "state_change",
  "component_id": "...",
  "component_name": "api-server",
  "application_name": "billing-prod",
  "from_state": "RUNNING",
  "to_state": "FAILED",
  "timestamp": "2026-02-24T10:30:00Z"
}
```

Use a PagerDuty integration or middleware to transform this into PagerDuty's event format.

---

## 8. ServiceNow Integration

### Change Request on Operations

Use AppControl's audit trail to feed ServiceNow change records:

```bash
# Export audit trail for a time range
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/reports/audit?from=2026-02-23&to=2026-02-24" \
  -H "X-API-Key: $AC_API_KEY" \
  | jq '.entries[] | {
      type: .action,
      target: .entity_type,
      user: .user_id,
      timestamp: .created_at
    }'
```

---

## 9. Topology Export for Documentation / CMDB

### Export DAG as Graphviz

```bash
# Get DOT format for Graphviz rendering
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/topology?format=dot" \
  -H "X-API-Key: $AC_API_KEY" \
  | jq -r '.content' > app_topology.dot

# Render to PNG
dot -Tpng app_topology.dot -o app_topology.png
```

### Export as YAML for Config Management

```bash
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/topology?format=yaml" \
  -H "X-API-Key: $AC_API_KEY" \
  | jq -r '.content' > app_topology.yaml
```

### Export as JSON for CMDB Ingestion

```bash
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/topology?format=json" \
  -H "X-API-Key: $AC_API_KEY" > app_topology.json
```

---

## 10. Dry Run and Plan Validation

### Get Execution Plan Before Any Operation

```bash
# What would a start do?
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/plan?operation=start" \
  -H "X-API-Key: $AC_API_KEY" | jq '.plan'

# What would a stop do?
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/plan?operation=stop" \
  -H "X-API-Key: $AC_API_KEY" | jq '.plan'

# Plan for a specific component and its dependencies
curl -s "https://appcontrol.example.com/api/v1/apps/$APP_ID/plan?operation=start&scope=$COMPONENT_ID" \
  -H "X-API-Key: $AC_API_KEY" | jq '.plan'
```

Response example:
```json
{
  "levels": [
    {
      "level": 0,
      "components": [
        {"name": "redis", "current_state": "RUNNING", "predicted_action": "skip"},
        {"name": "postgres", "current_state": "STOPPED", "predicted_action": "start"}
      ]
    },
    {
      "level": 1,
      "components": [
        {"name": "api-server", "current_state": "STOPPED", "predicted_action": "start"}
      ]
    }
  ],
  "total_actions": 2
}
```
