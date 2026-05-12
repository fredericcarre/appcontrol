#!/usr/bin/env bash
# scripts/seed-demo.sh
#
# Seeds the running AppControl stack with the Core Banking System demo
# application from examples/banking-core-system.json. Used by the
# documentation screenshot pipeline and by anyone who wants a demo
# environment with realistic data.
#
# Idempotent: if the demo app is already present, the import is skipped.

set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://localhost:3000}"
ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${SEED_ADMIN_PASSWORD:-admin}"
EXAMPLE_FILE="${EXAMPLE_FILE:-/workspace/examples/banking-core-system.json}"
DEMO_APP_NAME="${DEMO_APP_NAME:-Core Banking System}"
DEMO_AGENT_HOSTNAME="${DEMO_AGENT_HOSTNAME:-demo-host}"
TOKEN_OUT="${TOKEN_OUT:-/workspace/state/enrollment-token}"

log() { echo "[seed-demo] $*"; }
fail() { echo "[seed-demo] ERROR: $*" >&2; exit 1; }

# api_call METHOD PATH [JSON_BODY] [EXTRA_CURL_ARGS...]
# Performs the call, prints the response body to stdout on success, and
# exits 1 with the response body on stderr on any non-2xx status. This
# replaces "curl -sf" so callers always know why an HTTP call failed.
api_call() {
  local method=$1 path=$2 body=${3:-}
  shift 3 || true
  local extra=("$@")
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
    "${auth_args[@]}" "${body_args[@]}" "${extra[@]}" || echo "000")
  resp_body=$(cat "$tmp")
  rm -f "$tmp"

  if [ "$http_code" = "000" ]; then
    log "[$method $path] connection failed"
    return 22
  fi
  if [ "$http_code" -lt 200 ] || [ "$http_code" -ge 300 ]; then
    log "[$method $path] HTTP $http_code"
    log "  response: $resp_body"
    return 22
  fi
  printf '%s' "$resp_body"
}

# 1. Wait for backend health
log "Waiting for backend at $BACKEND_URL ..."
for i in $(seq 1 60); do
  if curl -sf "$BACKEND_URL/health" >/dev/null 2>&1; then
    log "Backend healthy after ${i}s"
    break
  fi
  if [ "$i" = "60" ]; then
    fail "Backend not healthy after 120s"
  fi
  sleep 2
done

# 2. Login
log "Logging in as $ADMIN_EMAIL"
LOGIN_PAYLOAD=$(jq -n --arg e "$ADMIN_EMAIL" --arg p "$ADMIN_PASSWORD" \
  '{email: $e, password: $p}')
LOGIN_RESPONSE=$(api_call POST /api/v1/auth/login "$LOGIN_PAYLOAD") \
  || fail "Login failed (see HTTP body above)"
AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token // empty')
[ -n "$AUTH_TOKEN" ] || fail "No token in login response: $LOGIN_RESPONSE"
log "Logged in"

# 3. Ensure at least one site exists in the org.
SITES_RESPONSE=$(api_call GET /api/v1/sites '') \
  || fail "Listing sites failed (see HTTP body above)"
SITE_COUNT=$(echo "$SITES_RESPONSE" | jq -r '(.sites // .) | length' 2>/dev/null || echo 0)
log "Sites in org: $SITE_COUNT"
if [ "${SITE_COUNT:-0}" = "0" ]; then
  log "Creating default site"
  api_call POST /api/v1/sites \
    '{"name":"Default","code":"default","site_type":"primary"}' >/dev/null \
    || fail "Creating default site failed"
fi

# 4. Import the demo app (idempotent)
APPS_RESPONSE=$(api_call GET /api/v1/apps '') \
  || fail "Listing apps failed"
EXISTING=$(echo "$APPS_RESPONSE" \
  | jq -r --arg n "$DEMO_APP_NAME" '.apps[] | select(.name == $n) | .id' \
  | head -n 1)

if [ -n "$EXISTING" ]; then
  log "Demo app '$DEMO_APP_NAME' already exists ($EXISTING). Skipping import."
else
  [ -f "$EXAMPLE_FILE" ] || fail "Example file not found: $EXAMPLE_FILE"
  log "Importing $EXAMPLE_FILE (hosts → '$DEMO_AGENT_HOSTNAME')"

  REWRITTEN=$(jq --arg h "$DEMO_AGENT_HOSTNAME" '
    {
      format_version: "4.0",
      application: (
        .components |= map(
          # Force every component onto our single demo agent.
          .host = $h
          # The v4 import expects commands nested under .commands as
          # { check: {cmd}, start: {cmd}, stop: {cmd}, integrity_check: ...}.
          # The example uses the flat shell-style fields. Wrap them.
          | .commands = {
              check: (if .check_cmd            then {cmd: .check_cmd}            else null end),
              start: (if .start_cmd            then {cmd: .start_cmd}            else null end),
              stop:  (if .stop_cmd             then {cmd: .stop_cmd}             else null end),
              integrity_check: (if .integrity_check_cmd then {cmd: .integrity_check_cmd} else null end),
              infra_check:     (if .infra_check_cmd     then {cmd: .infra_check_cmd}     else null end),
              rebuild:         (if .rebuild_cmd         then {cmd: .rebuild_cmd}         else null end)
            }
          # Drop the flat *_cmd siblings — V4Component#[serde(deny_unknown)]
          # would not error here, but we keep the payload clean.
          | del(.check_cmd, .start_cmd, .stop_cmd,
                .integrity_check_cmd, .infra_check_cmd, .rebuild_cmd)
        )
        | .tags = (
            if (.tags | type) == "object" then
              (.tags | to_entries | map(.key + "=" + (.value | tostring)))
            else (.tags // [])
            end
          )
      )
    }
  ' "$EXAMPLE_FILE")

  IMPORT_PAYLOAD=$(jq -n --arg json "$REWRITTEN" '{json: $json}')
  IMPORT_RESPONSE=$(api_call POST /api/v1/import/json "$IMPORT_PAYLOAD") \
    || fail "Import failed (see HTTP body above)"
  APP_ID=$(echo "$IMPORT_RESPONSE" | jq -r '.application_id // .app_id // .id // empty')
  log "Imported demo app: ${APP_ID:-?}"
fi

# 5. Provision an enrollment token for the demo agent
mkdir -p "$(dirname "$TOKEN_OUT")"
if [ -s "$TOKEN_OUT" ]; then
  log "Enrollment token already present at $TOKEN_OUT — leaving it"
else
  log "Creating enrollment token for the demo agent"
  TOKEN_RESPONSE=$(api_call POST /api/v1/enrollment/tokens \
    '{"name":"demo-agent","scope":"agent","valid_hours":168}') \
    || fail "Creating enrollment token failed (see HTTP body above)"
  ENROLL_TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.token // empty')
  [ -n "$ENROLL_TOKEN" ] || fail "No token in response: $TOKEN_RESPONSE"
  printf '%s' "$ENROLL_TOKEN" > "$TOKEN_OUT"
  chmod 600 "$TOKEN_OUT" 2>/dev/null || true
  log "Enrollment token written to $TOKEN_OUT"
fi

log "Done."
