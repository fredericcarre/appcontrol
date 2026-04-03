#!/usr/bin/env bash
# Full-stack E2E test: backend + gateway + agent + real process start/stop
#
# This test deploys ALL components and verifies the complete chain:
#   backend → gateway → agent → process start/stop/health-check
#
# Usage:
#   BACKEND_BIN=./target/release/appcontrol-backend \
#   GATEWAY_BIN=./target/release/appcontrol-gateway \
#   AGENT_BIN=./target/release/appcontrol-agent \
#   ./tests/full-stack-e2e.sh
#
# Environment:
#   BACKEND_BIN   - path to backend binary (required)
#   GATEWAY_BIN   - path to gateway binary (required)
#   AGENT_BIN     - path to agent binary (required)
#   DATABASE_URL  - database URL (default: sqlite in temp dir)
#   BACKEND_PORT  - backend listen port (default: 3210)
#   GATEWAY_PORT  - gateway listen port (default: 4453)

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
BACKEND_BIN="${BACKEND_BIN:?BACKEND_BIN must be set}"
GATEWAY_BIN="${GATEWAY_BIN:?GATEWAY_BIN must be set}"
AGENT_BIN="${AGENT_BIN:?AGENT_BIN must be set}"
BACKEND_PORT="${BACKEND_PORT:-3210}"
GATEWAY_PORT="${GATEWAY_PORT:-4453}"

WORKDIR=$(mktemp -d /tmp/appcontrol-e2e-XXXXXX)
LOGS="$WORKDIR/logs"
DATA="$WORKDIR/data"
mkdir -p "$LOGS" "$DATA"

PASS=0
FAIL=0
TOTAL=0
BACKEND_PID=0
GATEWAY_PID=0
AGENT_PID=0
TOKEN=""

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { echo -e "\033[36m[E2E]\033[0m $*"; }
ok()   { echo -e "\033[32m  ✓ $*\033[0m"; PASS=$((PASS+1)); TOTAL=$((TOTAL+1)); }
fail() { echo -e "\033[31m  ✗ $*\033[0m"; FAIL=$((FAIL+1)); TOTAL=$((TOTAL+1)); }

api() {
  local method="$1" path="$2"
  shift 2
  curl -s -X "$method" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    "http://localhost:${BACKEND_PORT}/api/v1${path}" "$@"
}

api_code() {
  local method="$1" path="$2"
  shift 2
  curl -s -o /dev/null -w '%{http_code}' -X "$method" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    "http://localhost:${BACKEND_PORT}/api/v1${path}" "$@"
}

wait_component_state() {
  local app_id="$1" comp_name="$2" expected="$3" timeout="${4:-30}"
  for i in $(seq 1 "$timeout"); do
    local state
    state=$(api GET "/apps/$app_id" | jq -r ".components[]? | select(.name==\"$comp_name\") | .current_state // .state // \"UNKNOWN\"" 2>/dev/null || echo "UNKNOWN")
    if [ "$state" = "$expected" ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

cleanup() {
  log "Cleaning up..."
  [ "$AGENT_PID" -gt 0 ] && kill "$AGENT_PID" 2>/dev/null || true
  [ "$GATEWAY_PID" -gt 0 ] && kill "$GATEWAY_PID" 2>/dev/null || true
  [ "$BACKEND_PID" -gt 0 ] && kill "$BACKEND_PID" 2>/dev/null || true
  # Kill any test processes we started
  pkill -f "sleep 99999.*e2e-marker" 2>/dev/null || true
  if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo "=== Backend logs (last 30 lines) ==="
    tail -30 "$LOGS/backend.log" 2>/dev/null || true
    echo "=== Gateway logs (last 15 lines) ==="
    tail -15 "$LOGS/gateway.log" 2>/dev/null || true
    echo "=== Agent logs (last 15 lines) ==="
    tail -15 "$LOGS/agent.log" 2>/dev/null || true
  fi
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# 1. Start backend
# ---------------------------------------------------------------------------
DB_URL="${DATABASE_URL:-sqlite:$DATA/appcontrol.db}"
DB_TYPE="SQLite"
if echo "$DB_URL" | grep -q "^postgres"; then DB_TYPE="PostgreSQL"; fi
log "Starting backend ($DB_TYPE) on port $BACKEND_PORT..."

DATABASE_URL="$DB_URL" \
JWT_SECRET="e2e-full-stack-test-secret-key-32chars!" \
LOCAL_AUTH_ENABLED=true \
SEED_ENABLED=true \
SEED_ADMIN_EMAIL=admin@e2e.test \
SEED_ADMIN_PASSWORD=e2e-password \
SEED_ORG_NAME=E2E-Org \
SEED_ORG_SLUG=e2e \
PORT="$BACKEND_PORT" \
RUST_LOG=info \
"$BACKEND_BIN" > "$LOGS/backend.log" 2>&1 &
BACKEND_PID=$!

for i in $(seq 1 30); do
  if curl -sf "http://localhost:$BACKEND_PORT/health" > /dev/null 2>&1; then
    ok "Backend started (pid=$BACKEND_PID)"
    break
  fi
  if [ "$i" = "30" ]; then
    fail "Backend failed to start within 30s"
    cat "$LOGS/backend.log"
    exit 1
  fi
  sleep 1
done

# ---------------------------------------------------------------------------
# 2. Login
# ---------------------------------------------------------------------------
log "Logging in..."
LOGIN_RESP=$(curl -s -X POST "http://localhost:$BACKEND_PORT/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@e2e.test","password":"e2e-password"}')
TOKEN=$(echo "$LOGIN_RESP" | jq -r '.token // empty')
if [ -z "$TOKEN" ]; then
  fail "Login failed: $LOGIN_RESP"
  exit 1
fi
ok "Logged in"

# ---------------------------------------------------------------------------
# 3. Create enrollment tokens
# ---------------------------------------------------------------------------
log "Creating enrollment tokens..."

GW_TOKEN=$(api POST "/enrollment/tokens" \
  -d '{"name":"e2e-gw","scope":"gateway","max_uses":1,"valid_hours":1}' \
  | jq -r '.token // empty')
if [ -z "$GW_TOKEN" ] || [ "$GW_TOKEN" = "null" ]; then
  fail "Gateway enrollment token creation failed"
  exit 1
fi
ok "Gateway enrollment token created"

AGENT_TOKEN=$(api POST "/enrollment/tokens" \
  -d '{"name":"e2e-agent","scope":"agent","max_uses":1,"valid_hours":1}' \
  | jq -r '.token // empty')
if [ -z "$AGENT_TOKEN" ] || [ "$AGENT_TOKEN" = "null" ]; then
  fail "Agent enrollment token creation failed"
  exit 1
fi
ok "Agent enrollment token created"

# ---------------------------------------------------------------------------
# 4. Start gateway
# ---------------------------------------------------------------------------
log "Starting gateway on port $GATEWAY_PORT..."

BACKEND_URL="ws://localhost:$BACKEND_PORT/ws/gateway" \
GATEWAY_LISTEN_PORT="$GATEWAY_PORT" \
GATEWAY_ZONE="e2e-zone" \
GATEWAY_ENROLLMENT_TOKEN="$GW_TOKEN" \
RUST_LOG=info \
"$GATEWAY_BIN" > "$LOGS/gateway.log" 2>&1 &
GATEWAY_PID=$!

sleep 5

if kill -0 "$GATEWAY_PID" 2>/dev/null; then
  ok "Gateway started (pid=$GATEWAY_PID)"
else
  fail "Gateway exited"
  cat "$LOGS/gateway.log"
  exit 1
fi

# ---------------------------------------------------------------------------
# 5. Start agent
# ---------------------------------------------------------------------------
log "Starting agent..."

GATEWAY_URL="ws://localhost:$GATEWAY_PORT" \
AGENT_ENROLLMENT_TOKEN="$AGENT_TOKEN" \
RUST_LOG=info \
"$AGENT_BIN" > "$LOGS/agent.log" 2>&1 &
AGENT_PID=$!

sleep 5

if kill -0 "$AGENT_PID" 2>/dev/null; then
  ok "Agent started (pid=$AGENT_PID)"
else
  fail "Agent exited"
  cat "$LOGS/agent.log"
  exit 1
fi

# ---------------------------------------------------------------------------
# 6. Verify agent registered
# ---------------------------------------------------------------------------
log "Checking agent registration..."
sleep 3

AGENTS_RESP=$(api GET "/agents")
AGENT_COUNT=$(echo "$AGENTS_RESP" | jq '[.agents // . | if type == "array" then .[] else empty end] | length' 2>/dev/null || echo "0")

if [ "$AGENT_COUNT" -ge 1 ]; then
  AGENT_ID=$(echo "$AGENTS_RESP" | jq -r '[.agents // . | if type == "array" then .[] else empty end][0].id' 2>/dev/null)
  ok "Agent registered (id=${AGENT_ID:0:8}..., count=$AGENT_COUNT)"
else
  fail "No agent registered (response: $AGENTS_RESP)"
  # Continue anyway — some tests might still work
fi

# ---------------------------------------------------------------------------
# 7. Create site
# ---------------------------------------------------------------------------
log "Creating site..."
SITE_RESP=$(api POST "/sites" -d '{"name":"E2E-Site","code":"E2E","site_type":"primary"}')
SITE_ID=$(echo "$SITE_RESP" | jq -r '.id // empty' 2>/dev/null)
if [ -z "$SITE_ID" ] || [ "$SITE_ID" = "null" ]; then
  # Site might already exist from seed
  SITE_ID=$(api GET "/sites" | jq -r '[.sites // . | if type == "array" then .[] else empty end][0].id' 2>/dev/null)
fi
if [ -n "$SITE_ID" ] && [ "$SITE_ID" != "null" ]; then
  ok "Site ready (id=${SITE_ID:0:8}...)"
else
  fail "No site available"
fi

# ---------------------------------------------------------------------------
# 8. Create application with real commands
# ---------------------------------------------------------------------------
log "Creating test application..."

# Commands use a marker so we can cleanly kill test processes
APP_RESP=$(api POST "/apps" -d "{
  \"name\": \"E2E-FullStack-App\",
  \"description\": \"Full stack E2E test app\",
  \"site_id\": \"$SITE_ID\"
}")
APP_ID=$(echo "$APP_RESP" | jq -r '.id // empty')
if [ -z "$APP_ID" ] || [ "$APP_ID" = "null" ]; then
  fail "App creation failed: $APP_RESP"
  exit 1
fi
ok "App created (id=${APP_ID:0:8}...)"

# Create a component with real start/stop/check commands
COMP_RESP=$(api POST "/apps/$APP_ID/components" -d "{
  \"name\": \"test-service\",
  \"component_type\": \"service\",
  \"agent_id\": \"$AGENT_ID\",
  \"start_cmd\": \"nohup bash -c 'while true; do sleep 1; done' &\",
  \"stop_cmd\": \"pkill -f 'while true; do sleep 1; done' || true\",
  \"check_cmd\": \"pgrep -f 'while true; do sleep 1; done'\",
  \"check_interval_seconds\": 5,
  \"start_timeout_seconds\": 30,
  \"stop_timeout_seconds\": 15
}")
COMP_ID=$(echo "$COMP_RESP" | jq -r '.id // empty')
if [ -z "$COMP_ID" ] || [ "$COMP_ID" = "null" ]; then
  fail "Component creation failed: $COMP_RESP"
else
  ok "Component created (id=${COMP_ID:0:8}...)"
fi

# ---------------------------------------------------------------------------
# 9. Start the application
# ---------------------------------------------------------------------------
log "Starting application..."
START_CODE=$(api_code POST "/apps/$APP_ID/start" -d '{}')
if [ "$START_CODE" = "200" ] || [ "$START_CODE" = "202" ]; then
  ok "Start command accepted (HTTP $START_CODE)"
else
  fail "Start command rejected (HTTP $START_CODE)"
fi

# Wait for component to reach RUNNING (agent executes start_cmd, then check_cmd returns 0)
log "Waiting for component to reach RUNNING state..."
if wait_component_state "$APP_ID" "test-service" "RUNNING" 60; then
  ok "Component reached RUNNING state"
else
  STATE=$(api GET "/apps/$APP_ID" | jq -r '.components[]? | select(.name=="test-service") | .current_state // .state // "UNKNOWN"' 2>/dev/null)
  fail "Component did not reach RUNNING (current: $STATE)"
fi

# ---------------------------------------------------------------------------
# 10. Verify state transitions were recorded
# ---------------------------------------------------------------------------
log "Checking state transitions..."
TRANSITIONS=$(api GET "/apps/$APP_ID/history" 2>/dev/null || echo "[]")
TRANSITION_COUNT=$(echo "$TRANSITIONS" | jq 'if type == "array" then length elif .transitions then (.transitions | length) else 0 end' 2>/dev/null || echo "0")
if [ "$TRANSITION_COUNT" -gt 0 ]; then
  ok "State transitions recorded ($TRANSITION_COUNT entries)"
else
  fail "No state transitions found"
fi

# ---------------------------------------------------------------------------
# 11. Stop the application
# ---------------------------------------------------------------------------
log "Stopping application..."
STOP_CODE=$(api_code POST "/apps/$APP_ID/stop" -d '{}')
if [ "$STOP_CODE" = "200" ] || [ "$STOP_CODE" = "202" ]; then
  ok "Stop command accepted (HTTP $STOP_CODE)"
else
  fail "Stop command rejected (HTTP $STOP_CODE)"
fi

# Wait for component to reach STOPPED
log "Waiting for component to reach STOPPED state..."
if wait_component_state "$APP_ID" "test-service" "STOPPED" 30; then
  ok "Component reached STOPPED state"
else
  STATE=$(api GET "/apps/$APP_ID" | jq -r '.components[]? | select(.name=="test-service") | .current_state // .state // "UNKNOWN"' 2>/dev/null)
  fail "Component did not reach STOPPED (current: $STATE)"
fi

# Verify the process was actually killed
sleep 2
if pgrep -f "while true; do sleep 1; done" > /dev/null 2>&1; then
  fail "Process still running after stop"
else
  ok "Process successfully terminated"
fi

# ---------------------------------------------------------------------------
# 12. Test restart cycle
# ---------------------------------------------------------------------------
log "Testing restart cycle..."
RESTART_CODE=$(api_code POST "/apps/$APP_ID/start" -d '{}')
if [ "$RESTART_CODE" = "200" ] || [ "$RESTART_CODE" = "202" ]; then
  ok "Restart command accepted"
else
  fail "Restart command rejected (HTTP $RESTART_CODE)"
fi

if wait_component_state "$APP_ID" "test-service" "RUNNING" 60; then
  ok "Component RUNNING again after restart"
else
  STATE=$(api GET "/apps/$APP_ID" | jq -r '.components[]? | select(.name=="test-service") | .current_state // .state // "UNKNOWN"' 2>/dev/null)
  fail "Component did not restart (current: $STATE)"
fi

# Stop again for cleanup
api POST "/apps/$APP_ID/stop" -d '{}' > /dev/null 2>&1 || true
sleep 5

# ---------------------------------------------------------------------------
# 13. Test heartbeat timeout → UNREACHABLE
# ---------------------------------------------------------------------------
log "Testing heartbeat timeout (kill agent → expect UNREACHABLE)..."

# First restart to get RUNNING state
api POST "/apps/$APP_ID/start" -d '{}' > /dev/null 2>&1
wait_component_state "$APP_ID" "test-service" "RUNNING" 60 || true

# Kill the agent to simulate disconnect
kill "$AGENT_PID" 2>/dev/null || true
AGENT_PID=0

# Wait for heartbeat monitor to detect stale agent (default timeout ~180s, but we check)
# The heartbeat monitor runs every 30s. With a short timeout it should detect within ~60s.
log "  Agent killed. Waiting for heartbeat timeout (this may take up to 60s)..."
if wait_component_state "$APP_ID" "test-service" "UNREACHABLE" 90; then
  ok "Component transitioned to UNREACHABLE after agent death"
else
  STATE=$(api GET "/apps/$APP_ID" | jq -r '.components[]? | select(.name=="test-service") | .current_state // .state // "UNKNOWN"' 2>/dev/null)
  fail "Component did not transition to UNREACHABLE (current: $STATE) — heartbeat timeout may be too long for CI"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "========================================"
if [ "$FAIL" -eq 0 ]; then
  echo -e "\033[32m  All $TOTAL tests passed!\033[0m"
else
  echo -e "\033[31m  $FAIL/$TOTAL tests failed\033[0m"
fi
echo "========================================"
echo ""
echo "  Backend log: $LOGS/backend.log"
echo "  Gateway log: $LOGS/gateway.log"
echo "  Agent log:   $LOGS/agent.log"
echo ""

exit "$FAIL"
