#!/usr/bin/env bash
# scripts/methodology-walkthrough.sh
#
# Hands-on tour of the AppControl methodology. Drives a running stack
# through every phase via curl, so you can read along on the UI and
# see each step land in real time.
#
# Phases exercised:
#   1. Import a mature map declared `reviewed`
#   2. Add a raw CMDB scrape declared `candidate`
#   3. Bump the application's activation level (advisory → diagnostic)
#   4. Drop an annotation on a component
#   5. Promote one component to `validated`
#   6. Create a pattern from a (fake) past incident
#   7. List propagation candidates and apply the pattern
#   8. Push the resulting map to a configured Git remote (skipped if
#      no remote configured)
#
# Usage:
#   BACKEND_URL=http://localhost:3000 \
#   ADMIN_EMAIL=admin@localhost \
#   ADMIN_PASSWORD=admin \
#   ./scripts/methodology-walkthrough.sh
#
# Set DRY_RUN=1 to print the curl commands without executing them.

set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://localhost:3000}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin}"
EXAMPLES_DIR="${EXAMPLES_DIR:-$(cd "$(dirname "$0")/../examples" && pwd)}"
DRY_RUN="${DRY_RUN:-0}"

# ─── helpers ──────────────────────────────────────────────────────

bold()  { printf '\033[1m%s\033[0m\n' "$*"; }
ok()    { printf '  \033[32m✓\033[0m %s\n' "$*"; }
info()  { printf '  \033[36mℹ\033[0m %s\n' "$*"; }
warn()  { printf '  \033[33m!\033[0m %s\n' "$*"; }
fail()  { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

api() {
  local method="$1"; shift
  local path="$1"; shift
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "curl -X $method $BACKEND_URL$path $*"
    return 0
  fi
  curl -sS -X "$method" "$BACKEND_URL$path" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" "$@"
}

login() {
  bold "Step 0 — Login"
  if [[ "$DRY_RUN" == "1" ]]; then
    TOKEN="DRY-RUN-FAKE-TOKEN"
    info "DRY_RUN: skipping real login"
    return 0
  fi
  local resp
  resp=$(curl -sS -X POST "$BACKEND_URL/api/v1/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\"}")
  TOKEN=$(echo "$resp" | jq -r '.access_token // .token // empty')
  if [[ -z "$TOKEN" ]]; then
    fail "Could not extract a token from /auth/login. Response: $resp"
  fi
  ok "Logged in as $ADMIN_EMAIL"
}

# ─── phase 1 ──────────────────────────────────────────────────────

phase1_import_mature() {
  bold "Phase 1 — Import a mature map (source declares 'reviewed')"
  local body
  body=$(jq -n \
    --arg json "$(cat "$EXAMPLES_DIR/methodology-demo.json")" \
    '{json: $json, default_knowledge_status: "reviewed", default_confidence_score: 0.85}')
  local resp
  resp=$(api POST /api/v1/import/json -d "$body")
  APP_ID=$(echo "$resp" | jq -r '.application_id // empty')
  [[ -n "$APP_ID" ]] || fail "Import failed: $resp"
  ok "Imported as application $APP_ID"
  info "Open $BACKEND_URL/apps/$APP_ID to see the map. The knowledge"
  info "pips should mostly be HIDDEN (validated) or INDIGO (reviewed)."
  info "Only 'webshop-cache' will show a SLATE pip (candidate)."
}

phase1_scrape_candidate() {
  bold "Phase 1 — Add a raw CMDB scrape (source declares 'candidate')"
  local body
  body=$(jq --arg appid "$APP_ID" '.application_id = $appid' \
    "$EXAMPLES_DIR/raw-cmdb-scrape.json")
  local resp
  resp=$(api POST /api/v1/ingestion/cmdb -d "$body")
  local created
  created=$(echo "$resp" | jq -r '.report.created // 0')
  ok "Created $created component(s) — they'll show SLATE pips on the map"
}

# ─── phase 4 ──────────────────────────────────────────────────────

phase4_advisory_then_diagnostic() {
  bold "Phase 4 — Move activation 4 → 1 (advisory) → 2 (diagnostic)"
  api PUT "/api/v1/apps/$APP_ID/activation" -d '{"level":1}' >/dev/null
  ok "Application now in advisory — checks observe, no commands allowed"
  sleep 1
  api PUT "/api/v1/apps/$APP_ID/activation" -d '{"level":2}' >/dev/null
  ok "Application now in diagnostic — checks active, still no start/stop"
}

# ─── phase 3 ──────────────────────────────────────────────────────

phase3_annotate_and_promote() {
  bold "Phase 3 — Annotate webshop-cache and promote its dependency"
  # Find webshop-cache component id
  local cache_id
  cache_id=$(api GET "/api/v1/apps/$APP_ID" | jq -r \
    '.components[] | select(.name == "webshop-cache") | .id')
  [[ -n "$cache_id" ]] || fail "Could not find webshop-cache"

  # Drop a TODO annotation
  api POST /api/v1/annotations -d "{
    \"target_type\": \"component\",
    \"target_id\":  \"$cache_id\",
    \"kind\":       \"todo\",
    \"body\":       \"Pas de health check pour l'instant. À raffiner après la revue avec l'équipe Redis.\"
  }" >/dev/null
  ok "Annotation posted on webshop-cache"

  # Promote it to draft (it's currently candidate)
  api PUT "/api/v1/components/$cache_id/knowledge" \
    -d '{"knowledge_status":"draft","confidence_score":0.55}' >/dev/null
  ok "webshop-cache promoted candidate → draft (slate pip becomes amber)"
}

# ─── phase 5 ──────────────────────────────────────────────────────

phase5_pattern_and_propagate() {
  bold "Phase 5 — Create a Spring Boot JDBC pattern, propagate to all candidates"
  local pattern_body
  pattern_body=$(cat "$EXAMPLES_DIR/pattern-spring-boot-jdbc.json")
  local resp
  resp=$(api POST /api/v1/patterns -d "$pattern_body")
  PATTERN_ID=$(echo "$resp" | jq -r '.id // empty')
  [[ -n "$PATTERN_ID" ]] || fail "Could not create pattern: $resp"
  ok "Pattern created — id $PATTERN_ID"

  # List candidates
  local candidates
  candidates=$(api GET "/api/v1/patterns/$PATTERN_ID/candidates")
  local count
  count=$(echo "$candidates" | jq -r '.total // 0')
  info "Pattern matches $count candidate component(s)"
  if [[ "$count" -gt 0 ]]; then
    local ids
    ids=$(echo "$candidates" | jq -r '.candidates[].component_id')
    local json_ids
    json_ids=$(echo "$ids" | jq -R . | jq -s .)
    api POST "/api/v1/patterns/$PATTERN_ID/propagate" \
      -d "{\"component_ids\": $json_ids}" >/dev/null
    ok "Pattern propagated to $count component(s)"
  else
    info "No candidates today — patterns library is populated, candidates list will grow with the parc"
  fi
}

# ─── phase 6 ──────────────────────────────────────────────────────

phase6_knowledge_summary() {
  bold "Phase 6 — Read the knowledge maturity summary"
  local summary
  summary=$(api GET "/api/v1/apps/$APP_ID/knowledge/summary")
  local total validated coverage
  total=$(echo "$summary"     | jq -r '.component_total // 0')
  validated=$(echo "$summary" | jq -r '.component_validated // 0')
  coverage=$(echo "$summary"  | jq -r '.validated_coverage // 0')
  printf "  Components total: %s\n" "$total"
  printf "  Validated:        %s (%.0f%%)\n" "$validated" \
    "$(awk "BEGIN {print $coverage*100}")"
}

# ─── git push ─────────────────────────────────────────────────────

git_push_optional() {
  bold "Git roundtrip — optional"
  if api GET "/api/v1/apps/$APP_ID/git" | jq -e '.git_remote_id != null' >/dev/null; then
    api POST "/api/v1/apps/$APP_ID/git/push" >/dev/null
    ok "Pushed current map to the configured Git remote"
  else
    info "No Git remote bound to this application — skipping push."
    info "Configure one via /api/v1/git/remotes + PUT /apps/$APP_ID/git"
  fi
}

# ─── main ─────────────────────────────────────────────────────────

main() {
  command -v jq  >/dev/null || fail "jq is required (apt-get install jq)"
  command -v curl >/dev/null || fail "curl is required"

  login
  phase1_import_mature
  phase1_scrape_candidate
  phase4_advisory_then_diagnostic
  phase3_annotate_and_promote
  phase5_pattern_and_propagate
  phase6_knowledge_summary
  git_push_optional

  bold ""
  bold "All phases exercised. Open $BACKEND_URL to explore the result."
}

main "$@"
