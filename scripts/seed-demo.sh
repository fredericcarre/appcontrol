#!/usr/bin/env bash
# scripts/seed-demo.sh
#
# Seeds the running AppControl stack with the Core Banking System demo
# application from examples/banking-core-system.json. Used by the
# documentation screenshot pipeline and by anyone who wants a demo
# environment with realistic data.
#
# Idempotent: if the demo app is already present, exits silently.
#
# Required env:
#   BACKEND_URL          (default: http://localhost:3000)
#   SEED_ADMIN_EMAIL     (default: admin@localhost — same value as the
#                         backend SEED_* config; demo auth mode ignores
#                         the password)
#   EXAMPLE_FILE         (default: /workspace/examples/banking-core-system.json)
#   DEMO_APP_NAME        (default: "Core Banking System" — must match
#                         the "name" field in EXAMPLE_FILE for the
#                         idempotency check)

set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://localhost:3000}"
ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-admin@localhost}"
EXAMPLE_FILE="${EXAMPLE_FILE:-/workspace/examples/banking-core-system.json}"
DEMO_APP_NAME="${DEMO_APP_NAME:-Core Banking System}"
# All components in the demo are rewritten to a single host so that
# one agent (registering with this hostname) is auto-bound to all of
# them by the backend's resolve_host_to_agent / auto_bind_agent logic.
DEMO_AGENT_HOSTNAME="${DEMO_AGENT_HOSTNAME:-demo-host}"
# Path written inside the shared state volume so the demo-agent service
# can pick up the enrollment token after this script runs.
TOKEN_OUT="${TOKEN_OUT:-/workspace/state/enrollment-token}"

log() { echo "[seed-demo] $*"; }
fail() { echo "[seed-demo] ERROR: $*" >&2; exit 1; }

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

# 2. Login (demo auth: any password works)
log "Logging in as $ADMIN_EMAIL"
LOGIN_PAYLOAD=$(cat <<EOF
{"email":"$ADMIN_EMAIL","password":"demo"}
EOF
)
LOGIN_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d "$LOGIN_PAYLOAD")
TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')
if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
  fail "Login failed: $LOGIN_RESPONSE"
fi
log "Logged in"

AUTH_HEADER="Authorization: Bearer $TOKEN"

# 3. Import the demo app (idempotent)
EXISTING=$(curl -sf -H "$AUTH_HEADER" "$BACKEND_URL/api/v1/apps" \
  | jq -r --arg n "$DEMO_APP_NAME" '.[] | select(.name == $n) | .id' \
  | head -n 1)
if [ -n "$EXISTING" ]; then
  log "Demo app '$DEMO_APP_NAME' already exists ($EXISTING). Skipping import."
else
  if [ ! -f "$EXAMPLE_FILE" ]; then
    fail "Example file not found: $EXAMPLE_FILE"
  fi
  log "Importing $EXAMPLE_FILE (rewriting all component hosts to '$DEMO_AGENT_HOSTNAME')"

  # Rewrite all component "host" fields to DEMO_AGENT_HOSTNAME so they
  # all bind to the single demo agent we are about to enroll. The
  # original example uses fictitious hostnames per tier — useful for
  # the cartography but we only have one agent in the demo stack.
  REWRITTEN=$(jq --arg h "$DEMO_AGENT_HOSTNAME" \
    '.components |= map(.host = $h)' "$EXAMPLE_FILE")

  IMPORT_PAYLOAD=$(jq -n --arg json "$REWRITTEN" '{json: $json}')
  IMPORT_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/import/json" \
    -H "$AUTH_HEADER" \
    -H "Content-Type: application/json" \
    -d "$IMPORT_PAYLOAD")
  APP_ID=$(echo "$IMPORT_RESPONSE" | jq -r '.application_id // .app_id // .id // empty')
  log "Imported demo app: ${APP_ID:-?}"
fi

# 4. Provision an enrollment token for the demo agent
mkdir -p "$(dirname "$TOKEN_OUT")"
if [ -s "$TOKEN_OUT" ]; then
  log "Enrollment token already present at $TOKEN_OUT — leaving it"
else
  log "Creating enrollment token for the demo agent"
  TOKEN_PAYLOAD='{"name":"demo-agent","scope":"agent","valid_hours":168}'
  TOKEN_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/enrollment/tokens" \
    -H "$AUTH_HEADER" \
    -H "Content-Type: application/json" \
    -d "$TOKEN_PAYLOAD")
  TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.token // empty')
  if [ -z "$TOKEN" ]; then
    fail "Could not create enrollment token: $TOKEN_RESPONSE"
  fi
  printf '%s' "$TOKEN" > "$TOKEN_OUT"
  chmod 600 "$TOKEN_OUT" 2>/dev/null || true
  log "Enrollment token written to $TOKEN_OUT"
fi

log "Done."
