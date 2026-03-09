# Component Metrics

AppControl supports **generic metrics extraction** from check commands. Any check script (health, integrity, infrastructure) can return structured JSON data that gets stored, visualized, and used for dashboards.

## Overview

When a check command runs, AppControl inspects its stdout for JSON data. If found, the metrics are:

1. **Stored** in the `check_events` table alongside the check result
2. **Displayed** in the component detail panel
3. **Available** for historical charts and dashboards
4. **Accessible** via API for external integrations

This approach is **non-intrusive**: existing scripts continue to work unchanged. Simply add JSON output to enable metrics.

## Output Formats

AppControl supports four detection strategies, tried in order:

### 1. Pure JSON (Recommended for new scripts)

The entire stdout is a valid JSON object:

```bash
#!/bin/bash
# health_check.sh - Pure JSON output

CONNECTIONS=$(netstat -an | grep ESTABLISHED | wc -l)
MEMORY_MB=$(free -m | awk '/^Mem:/{print $3}')
UPTIME_HOURS=$(awk '{print int($1/3600)}' /proc/uptime)

cat <<EOF
{
  "connections": $CONNECTIONS,
  "memory_mb": $MEMORY_MB,
  "uptime_hours": $UPTIME_HOURS,
  "status": "healthy"
}
EOF

exit 0
```

### 2. Tagged Format (`<appcontrol>...</appcontrol>`)

Embed metrics within normal log output using XML-style tags:

```bash
#!/bin/bash
# health_check.sh - Tagged format

echo "[INFO] Starting health check..."
echo "[INFO] Checking database connections..."

CONNECTIONS=$(netstat -an | grep :5432 | grep ESTABLISHED | wc -l)
QUERIES_PER_SEC=$(psql -t -c "SELECT xact_commit FROM pg_stat_database WHERE datname='mydb'")

echo "[INFO] Health check complete"

# Metrics embedded in tags (ignored by normal log processing)
echo "<appcontrol>{\"connections\": $CONNECTIONS, \"qps\": $QUERIES_PER_SEC}</appcontrol>"

exit 0
```

### 3. Marker Format (`---METRICS---`)

Use a clear marker to separate logs from metrics:

```bash
#!/bin/bash
# health_check.sh - Marker format

echo "Checking service health..."
systemctl is-active myservice || exit 1

RESPONSE_TIME=$(curl -w "%{time_total}" -o /dev/null -s http://localhost:8080/health)
ACTIVE_SESSIONS=$(redis-cli GET active_sessions)

echo "Service is healthy"
echo "---METRICS---"
cat <<EOF
{
  "response_time_ms": $(echo "$RESPONSE_TIME * 1000" | bc),
  "active_sessions": $ACTIVE_SESSIONS
}
EOF

exit 0
```

### 4. Auto-Detect (Last JSON Line)

AppControl scans stdout backwards for the last line that is a valid JSON object:

```bash
#!/bin/bash
# health_check.sh - Auto-detect (last JSON line)

echo "[2024-01-15 10:30:45] Starting health check"
echo "[2024-01-15 10:30:45] Checking process status..."
echo "[2024-01-15 10:30:46] Process myapp is running (PID 12345)"
echo "[2024-01-15 10:30:46] Checking memory usage..."
echo "[2024-01-15 10:30:46] Memory: 1.2GB / 4GB"
echo "[2024-01-15 10:30:46] Health check passed"

# Last line is JSON - automatically detected
echo '{"memory_gb": 1.2, "memory_percent": 30, "pid": 12345}'

exit 0
```

## Metric Data Structure

Metrics can contain any valid JSON object. Common patterns:

### Simple Key-Value Metrics

```json
{
  "cpu_percent": 45.2,
  "memory_mb": 2048,
  "disk_percent": 67,
  "connections": 150
}
```

### Nested Structures

```json
{
  "database": {
    "connections": 50,
    "queries_per_sec": 1200,
    "cache_hit_ratio": 0.95
  },
  "http": {
    "requests_per_sec": 500,
    "avg_response_ms": 45,
    "error_rate": 0.02
  }
}
```

### With Widget Hints

Add `_widget` suffix to suggest display type:

```json
{
  "cpu_percent": 78,
  "cpu_percent_widget": "gauge",
  "memory_breakdown": {
    "heap": 512,
    "stack": 64,
    "native": 128
  },
  "memory_breakdown_widget": "pie",
  "recent_requests": [100, 120, 95, 140, 130],
  "recent_requests_widget": "sparkline"
}
```

## Widget Types

The UI supports multiple widget types for metric visualization:

| Widget | Use Case | Data Format |
|--------|----------|-------------|
| `number` | Single value display | `42`, `"1.5GB"` |
| `gauge` | Percentage/progress | `0-100` number |
| `status` | OK/Warning/Error indicator | `"ok"`, `"warning"`, `"error"` |
| `sparkline` | Mini trend chart | Array of numbers |
| `bars` | Horizontal bar chart | `{"label": value, ...}` |
| `pie` | Pie/donut chart | `{"label": value, ...}` |
| `table` | Tabular data | Array of objects |
| `list` | Simple list | Array of strings |
| `text` | Formatted text | String |

### Widget Examples

**Gauge (CPU/Memory usage):**
```json
{
  "cpu_percent": 72,
  "cpu_percent_widget": "gauge"
}
```

**Sparkline (Request trend):**
```json
{
  "requests_1h": [120, 145, 132, 198, 176, 154],
  "requests_1h_widget": "sparkline"
}
```

**Pie (Memory breakdown):**
```json
{
  "memory": {
    "Java Heap": 2048,
    "Native": 512,
    "Metaspace": 256
  },
  "memory_widget": "pie"
}
```

**Table (Top processes):**
```json
{
  "top_processes": [
    {"name": "java", "cpu": 45, "memory": 2048},
    {"name": "nginx", "cpu": 12, "memory": 128},
    {"name": "redis", "cpu": 8, "memory": 512}
  ],
  "top_processes_widget": "table"
}
```

**Bars (Resource distribution):**
```json
{
  "disk_usage": {
    "/data": 75,
    "/logs": 45,
    "/tmp": 20
  },
  "disk_usage_widget": "bars"
}
```

## Display Locations

Metrics appear in multiple places in the UI:

### 1. Component Detail Panel

When you click a component, the detail panel shows:
- Latest metrics with appropriate widgets
- Historical chart for numeric values
- Last updated timestamp

### 2. Node Tooltips

Hovering over a component node shows key metrics as a quick summary.

### 3. Dashboard Cards

Create dashboard cards that aggregate metrics across components:
- Application-level summaries
- Cross-component comparisons
- Alert thresholds

### 4. Fullscreen View

Expand any metric to fullscreen for detailed analysis with historical data.

## API Access

### Get Latest Metrics

```bash
GET /api/v1/components/{id}/metrics

Response:
{
  "component_id": "uuid",
  "metrics": {
    "cpu_percent": 45,
    "memory_mb": 2048
  },
  "at": "2024-01-15T10:30:46Z"
}
```

### Get Metrics History

```bash
GET /api/v1/components/{id}/metrics/history?limit=100

Response:
{
  "component_id": "uuid",
  "history": [
    {
      "metrics": {"cpu_percent": 45, "memory_mb": 2048},
      "at": "2024-01-15T10:30:46Z"
    },
    {
      "metrics": {"cpu_percent": 42, "memory_mb": 2010},
      "at": "2024-01-15T10:30:16Z"
    }
  ]
}
```

## Example Scripts

### Database Health Check (PostgreSQL)

```bash
#!/bin/bash
set -e

DB_NAME="${1:-mydb}"
HOST="${2:-localhost}"

# Check connection
pg_isready -h "$HOST" -d "$DB_NAME" > /dev/null 2>&1 || exit 1

# Gather metrics
read CONNECTIONS ACTIVE_QUERIES <<< $(psql -h "$HOST" -d "$DB_NAME" -t -A -c "
  SELECT
    (SELECT count(*) FROM pg_stat_activity),
    (SELECT count(*) FROM pg_stat_activity WHERE state = 'active')
")

CACHE_HIT=$(psql -h "$HOST" -d "$DB_NAME" -t -A -c "
  SELECT round(100.0 * sum(blks_hit) / nullif(sum(blks_hit + blks_read), 0), 2)
  FROM pg_stat_database WHERE datname = '$DB_NAME'
")

DB_SIZE=$(psql -h "$HOST" -d "$DB_NAME" -t -A -c "
  SELECT pg_database_size('$DB_NAME') / 1024 / 1024
")

cat <<EOF
{
  "connections": $CONNECTIONS,
  "active_queries": $ACTIVE_QUERIES,
  "cache_hit_ratio": $CACHE_HIT,
  "cache_hit_ratio_widget": "gauge",
  "database_size_mb": $DB_SIZE
}
EOF

exit 0
```

### Java Application Health Check

```bash
#!/bin/bash
set -e

PID_FILE="${1:-/var/run/myapp.pid}"
JMX_PORT="${2:-9999}"

# Check process
if [ ! -f "$PID_FILE" ]; then
  echo '{"status": "error", "message": "PID file not found"}'
  exit 1
fi

PID=$(cat "$PID_FILE")
if ! kill -0 "$PID" 2>/dev/null; then
  echo '{"status": "error", "message": "Process not running"}'
  exit 1
fi

# Get JVM metrics via jstat
HEAP_USED=$(jstat -gc "$PID" | tail -1 | awk '{print ($3+$4+$6+$8)/1024}')
HEAP_MAX=$(jstat -gc "$PID" | tail -1 | awk '{print ($1+$2+$5+$7)/1024}')
GC_COUNT=$(jstat -gc "$PID" | tail -1 | awk '{print $13+$15}')
GC_TIME=$(jstat -gc "$PID" | tail -1 | awk '{print $14+$16}')

# Get thread count
THREADS=$(jstack "$PID" 2>/dev/null | grep -c "^\"" || echo 0)

cat <<EOF
{
  "heap_used_mb": $(printf "%.0f" $HEAP_USED),
  "heap_max_mb": $(printf "%.0f" $HEAP_MAX),
  "heap_percent": $(echo "scale=1; $HEAP_USED * 100 / $HEAP_MAX" | bc),
  "heap_percent_widget": "gauge",
  "gc_count": $GC_COUNT,
  "gc_time_ms": $(printf "%.0f" $GC_TIME),
  "thread_count": $THREADS
}
EOF

exit 0
```

### Web Service Health Check

```bash
#!/bin/bash
set -e

URL="${1:-http://localhost:8080/health}"
TIMEOUT="${2:-5}"

# Make request and capture timing
RESPONSE=$(curl -s -w "\n%{http_code}\n%{time_total}" \
  --connect-timeout "$TIMEOUT" \
  -o /tmp/health_response.txt \
  "$URL" 2>/dev/null)

HTTP_CODE=$(echo "$RESPONSE" | tail -2 | head -1)
RESPONSE_TIME=$(echo "$RESPONSE" | tail -1)

if [ "$HTTP_CODE" != "200" ]; then
  echo "{\"status\": \"error\", \"http_code\": $HTTP_CODE}"
  exit 1
fi

# Parse response if JSON
BODY=$(cat /tmp/health_response.txt 2>/dev/null || echo "{}")

cat <<EOF
{
  "http_code": $HTTP_CODE,
  "response_time_ms": $(echo "$RESPONSE_TIME * 1000" | bc | cut -d. -f1),
  "response_time_ms_widget": "number",
  "upstream_status": $(echo "$BODY" | jq -c '.checks // {}')
}
EOF

exit 0
```

### Windows Service Health Check (PowerShell)

```powershell
# health_check.ps1
param(
    [string]$ServiceName = "MyService"
)

$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue

if (-not $service) {
    @{status = "error"; message = "Service not found"} | ConvertTo-Json
    exit 1
}

if ($service.Status -ne "Running") {
    @{status = "error"; service_status = $service.Status} | ConvertTo-Json
    exit 1
}

# Get process metrics
$process = Get-Process -Name $ServiceName -ErrorAction SilentlyContinue

$metrics = @{
    status = "ok"
    service_status = $service.Status.ToString()
}

if ($process) {
    $metrics["cpu_percent"] = [math]::Round($process.CPU, 2)
    $metrics["memory_mb"] = [math]::Round($process.WorkingSet64 / 1MB, 0)
    $metrics["memory_mb_widget"] = "number"
    $metrics["thread_count"] = $process.Threads.Count
    $metrics["handle_count"] = $process.HandleCount
}

$metrics | ConvertTo-Json -Compress
exit 0
```

## Best Practices

### 1. Keep Metrics Small

Metrics are stored with every check execution (default: every 30 seconds). Keep the JSON payload small:

```json
// Good - focused metrics
{"cpu": 45, "mem": 2048, "conns": 150}

// Avoid - too verbose
{"cpu_usage_percent_current": 45.234567, "memory_usage_megabytes": 2048.123456, ...}
```

### 2. Use Consistent Keys

Use the same metric keys across check runs to enable historical charting:

```json
// Consistent - enables trending
{"qps": 1200}  // Run 1
{"qps": 1250}  // Run 2
{"qps": 1180}  // Run 3

// Avoid - breaks trending
{"queries_per_second": 1200}  // Run 1
{"qps": 1250}                 // Run 2
{"queryRate": 1180}           // Run 3
```

### 3. Include Status Indicators

Add status fields for quick visual assessment:

```json
{
  "db_status": "ok",
  "cache_status": "warning",
  "queue_status": "ok",
  "overall_status": "warning"
}
```

### 4. Widget Hints are Optional

The UI auto-detects appropriate widgets based on data type. Add `_widget` hints only when you need specific visualization:

```json
{
  "cpu": 75,           // Auto: gauge (0-100 range detected)
  "memory_mb": 2048,   // Auto: number
  "errors": [1,0,2,0,1] // Auto: sparkline (array of numbers)
}
```

### 5. Handle Errors Gracefully

Return error information in JSON format:

```json
{
  "status": "error",
  "error_code": "DB_CONNECTION_FAILED",
  "message": "Could not connect to database",
  "last_successful_check": "2024-01-15T10:25:00Z"
}
```

## Storage and Retention

Metrics are stored in the `check_events` table:

```sql
CREATE TABLE check_events (
    id UUID PRIMARY KEY,
    component_id UUID NOT NULL,
    check_type VARCHAR(20) NOT NULL,  -- health, integrity, infrastructure
    exit_code SMALLINT NOT NULL,
    stdout TEXT,
    duration_ms INTEGER,
    metrics JSONB,                     -- Extracted JSON metrics
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Index for efficient metrics queries
CREATE INDEX idx_check_events_metrics
    ON check_events (component_id, created_at DESC)
    WHERE metrics IS NOT NULL;
```

Default retention: 30 days. Configure via `CHECK_EVENTS_RETENTION_DAYS` environment variable.

## Troubleshooting

### Metrics Not Appearing

1. **Check JSON validity**: Use `jq` to validate your output
   ```bash
   ./health_check.sh | jq .
   ```

2. **Verify extraction**: Check agent logs for "extracted metrics" entries

3. **Check database**: Query `check_events` directly
   ```sql
   SELECT metrics FROM check_events
   WHERE component_id = 'your-id'
   ORDER BY created_at DESC LIMIT 1;
   ```

### Large Metrics Causing Issues

If metrics are too large (>64KB), they may be truncated. Solutions:
- Reduce metric granularity
- Remove verbose nested structures
- Use summary statistics instead of raw data

### Mixed Output Parsing

If auto-detect picks the wrong JSON line:
- Use `---METRICS---` marker for explicit separation
- Or output pure JSON (no log lines)
