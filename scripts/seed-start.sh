#!/usr/bin/env bash
# scripts/seed-start.sh
#
# Second-pass seed: waits for the demo agent to be registered and
# active, then starts the demo application via the REST API. After
# this script returns successfully, components have been driven
# through STARTING → RUNNING and the map shows realistic state.

set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://backend:3000}"
ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${SEED_ADMIN_PASSWORD:-admin}"
DEMO_APP_NAME="${DEMO_APP_NAME:-Core Banking System}"
DEMO_AGENT_HOSTNAME="${DEMO_AGENT_HOSTNAME:-demo-host}"

log() { echo "[seed-start] $*"; }
fail() { echo "[seed-start] ERROR: $*" >&2; exit 1; }

# Wraps curl to surface HTTP status + body on every non-2xx response,
# instead of swallowing them with `curl -sf`. Same shape as the helper
# in scripts/seed-demo.sh.
api_call() {
  local method=$1 path=$2 body=${3:-}
  local tmp http_code resp_body
  tmp=$(mktemp)
  local -a auth_args=()
  if [ -n "${AUTH_TOKEN:-}" ]; then
    auth_args+=(-H "Authorization: Bearer $AUTH_TOKEN")
  fi
  local -a body_args=()
  if [ -n "$body" ]; then
    body_args+=(-H "Content-Type: application/json" -d "$body")
  fi
  http_code=$(curl -sS -o "$tmp" -w "%{http_code}" \
    -X "$method" "${BACKEND_URL}${path}" \
    "${auth_args[@]}" "${body_args[@]}" || echo "000")
  resp_body=$(cat "$tmp")
  rm -f "$tmp"
  if [ "$http_code" = "000" ]; then
    log "[$method $path] connection failed"
    return 1
  fi
  if [ "$http_code" -lt 200 ] || [ "$http_code" -ge 300 ]; then
    log "[$method $path] HTTP $http_code"
    log "  response: $resp_body"
    return 1
  fi
  printf '%s' "$resp_body"
}

# 1. Wait for backend
log "Waiting for backend at $BACKEND_URL"
for i in $(seq 1 60); do
  if curl -sf "$BACKEND_URL/health" >/dev/null 2>&1; then break; fi
  if [ "$i" = "60" ]; then fail "Backend not healthy"; fi
  sleep 2
done

# 2. Login
LOGIN_PAYLOAD=$(jq -n --arg e "$ADMIN_EMAIL" --arg p "$ADMIN_PASSWORD" \
  '{email: $e, password: $p}')
LOGIN_RESPONSE=$(api_call POST /api/v1/auth/login "$LOGIN_PAYLOAD") \
  || fail "Login failed"
AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token // empty')
[ -n "$AUTH_TOKEN" ] || fail "No token in login response"
log "Logged in"

# 3. Wait for the demo agent to be registered and active
log "Waiting for agent '$DEMO_AGENT_HOSTNAME' to register"
for i in $(seq 1 120); do
  AGENTS=$(api_call GET /api/v1/agents '') || true
  AGENT_STATUS=$(echo "$AGENTS" | jq -r --arg h "$DEMO_AGENT_HOSTNAME" \
    '.agents[]? | select(.hostname == $h) | .is_active' 2>/dev/null | head -n 1)
  if [ "$AGENT_STATUS" = "true" ]; then
    log "Agent registered and active after $((i * 2))s"
    break
  fi
  if [ "$i" = "120" ]; then
    log "Agents currently visible: $AGENTS"
    fail "Agent did not register within 240s"
  fi
  sleep 2
done

# 4. Find the demo app
APPS_RESPONSE=$(api_call GET /api/v1/apps '') || fail "List apps failed"
APP_ID=$(echo "$APPS_RESPONSE" | jq -r --arg n "$DEMO_APP_NAME" \
  '.apps[] | select(.name == $n) | .id' | head -n 1)
[ -n "$APP_ID" ] || fail "Demo app '$DEMO_APP_NAME' not found"
log "Demo app id: $APP_ID"

# 5. Start the application
log "Starting application $APP_ID"
START_RESPONSE=$(api_call POST "/api/v1/apps/$APP_ID/start" '{}') \
  || fail "Start app failed (see HTTP body above)"
log "Start response: $(echo "$START_RESPONSE" | jq -c .)"

# 6. Wait for the components to converge to RUNNING/DEGRADED.
# /apps/:id/components does not expose current_state, but /apps
# returns per-app aggregate counts (running_count, degraded_count,
# failed_count, component_count) which is what we need.
log "Waiting for components to converge to RUNNING"
for i in $(seq 1 90); do
  APP_AGG=$(api_call GET /api/v1/apps '' \
    | jq --arg id "$APP_ID" '.apps[] | select(.id == $id)')
  RUNNING=$(echo "$APP_AGG" | jq -r '.running_count // 0')
  DEGRADED=$(echo "$APP_AGG" | jq -r '.degraded_count // 0')
  FAILED=$(echo "$APP_AGG" | jq -r '.failed_count // 0')
  STARTING=$(echo "$APP_AGG" | jq -r '.starting_count // 0')
  TOTAL=$(echo "$APP_AGG" | jq -r '.component_count // 0')
  log "  state run=$RUNNING starting=$STARTING degraded=$DEGRADED failed=$FAILED total=$TOTAL"
  if [ "${TOTAL:-0}" -gt 0 ] && \
     [ "$((RUNNING + DEGRADED))" = "$TOTAL" ]; then
    log "All components RUNNING/DEGRADED"
    break
  fi
  sleep 4
done

log "Done."
