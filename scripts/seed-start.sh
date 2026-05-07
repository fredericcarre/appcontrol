#!/usr/bin/env bash
# scripts/seed-start.sh
#
# Second-pass seed: waits for the demo agent to be registered and
# active, then starts the demo application via the REST API. After
# this script returns successfully, components have been driven
# through STARTING → RUNNING and the map shows realistic state.
#
# Runs from the demo-starter service in docker-compose.yaml, after
# both demo-seeder and demo-agent are up.

set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://backend:3000}"
ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${SEED_ADMIN_PASSWORD:-admin}"
DEMO_APP_NAME="${DEMO_APP_NAME:-Core Banking System}"
DEMO_AGENT_HOSTNAME="${DEMO_AGENT_HOSTNAME:-demo-host}"

log() { echo "[seed-start] $*"; }
fail() { echo "[seed-start] ERROR: $*" >&2; exit 1; }

# 1. Wait for backend
for i in $(seq 1 60); do
  if curl -sf "$BACKEND_URL/health" >/dev/null 2>&1; then break; fi
  sleep 2
done

# 2. Login
LOGIN_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\"}")
TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')
[ -n "$TOKEN" ] && [ "$TOKEN" != "null" ] || fail "Login failed: $LOGIN_RESPONSE"
AUTH_HEADER="Authorization: Bearer $TOKEN"

# 3. Wait for the demo agent to be registered and active
log "Waiting for agent '$DEMO_AGENT_HOSTNAME' to register"
for i in $(seq 1 120); do
  AGENT_STATUS=$(curl -sf -H "$AUTH_HEADER" "$BACKEND_URL/api/v1/agents" \
    | jq -r --arg h "$DEMO_AGENT_HOSTNAME" \
        '.agents[] | select(.hostname == $h) | .is_active' 2>/dev/null \
    | head -n 1)
  if [ "$AGENT_STATUS" = "true" ]; then
    log "Agent registered and active after $((i * 2))s"
    break
  fi
  if [ "$i" = "120" ]; then
    fail "Agent did not register within 240s"
  fi
  sleep 2
done

# 4. Find the demo app
APP_ID=$(curl -sf -H "$AUTH_HEADER" "$BACKEND_URL/api/v1/apps" \
  | jq -r --arg n "$DEMO_APP_NAME" '.apps[] | select(.name == $n) | .id' \
  | head -n 1)
[ -n "$APP_ID" ] || fail "Demo app '$DEMO_APP_NAME' not found"
log "Demo app id: $APP_ID"

# 5. Start the application
log "Starting application"
curl -sf -X POST "$BACKEND_URL/api/v1/apps/$APP_ID/start" \
  -H "$AUTH_HEADER" \
  -H "Content-Type: application/json" \
  -d '{}' >/dev/null \
  || log "Start request returned non-2xx (likely already started); continuing"

# 6. Wait for components to settle (RUNNING or DEGRADED is acceptable)
log "Waiting for components to converge to RUNNING"
for i in $(seq 1 90); do
  STATES=$(curl -sf -H "$AUTH_HEADER" "$BACKEND_URL/api/v1/apps/$APP_ID/components" \
    | jq -r '[.components[] | .current_state] | @csv' 2>/dev/null || echo "")
  RUNNING_COUNT=$(echo "$STATES" | tr ',' '\n' | grep -c '"RUNNING"' || true)
  TOTAL=$(echo "$STATES" | tr ',' '\n' | grep -c '"' || true)
  log "  ${RUNNING_COUNT}/${TOTAL} components RUNNING"
  if [ "$RUNNING_COUNT" -gt 0 ] && [ "$RUNNING_COUNT" = "$TOTAL" ]; then
    log "All components RUNNING"
    break
  fi
  sleep 4
done

log "Done."
