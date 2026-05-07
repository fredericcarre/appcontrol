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

# 3. Idempotency check: skip if the demo app already exists
EXISTING=$(curl -sf -H "$AUTH_HEADER" "$BACKEND_URL/api/v1/apps" \
  | jq -r --arg n "$DEMO_APP_NAME" '.[] | select(.name == $n) | .id' \
  | head -n 1)
if [ -n "$EXISTING" ]; then
  log "Demo app '$DEMO_APP_NAME' already exists ($EXISTING). Nothing to do."
  exit 0
fi

# 4. Import the example as JSON
if [ ! -f "$EXAMPLE_FILE" ]; then
  fail "Example file not found: $EXAMPLE_FILE"
fi
log "Importing $EXAMPLE_FILE"

# The /api/v1/import/json endpoint expects {"json": "<stringified json>"}
JSON_CONTENT=$(cat "$EXAMPLE_FILE")
IMPORT_PAYLOAD=$(jq -n --arg json "$JSON_CONTENT" '{json: $json}')

IMPORT_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/import/json" \
  -H "$AUTH_HEADER" \
  -H "Content-Type: application/json" \
  -d "$IMPORT_PAYLOAD")
APP_ID=$(echo "$IMPORT_RESPONSE" | jq -r '.application_id // .app_id // .id // empty')

if [ -z "$APP_ID" ]; then
  log "Import succeeded but app id not found in response: $IMPORT_RESPONSE"
else
  log "Imported demo app: $APP_ID"
fi

log "Done."
