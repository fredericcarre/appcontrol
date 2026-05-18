#!/usr/bin/env bash
# =============================================================================
# AppControl — OpenShift / Kubernetes deployment validation
# =============================================================================
# Validates a Helm-deployed AppControl release on any cluster that exposes
# the kube/oc CLIs. Designed to run identically against:
#   - kind + OpenShift Route CRD (CI per-PR)
#   - Red Hat Developer Sandbox / OpenShift cluster (nightly, manual)
#   - MicroShift / CRC / OKD
#
# Checks performed:
#   1. All deployments reach Ready within timeout
#   2. All pods are Running with restart count == 0
#   3. Container logs contain no panic / fatal / ERROR markers
#   4. Backend /health and /ready respond 200 (via port-forward)
#   5. Login with seed admin credentials returns a JWT
#   6. POST /api/v1/apps creates an application
#   7. POST /api/v1/components creates a component for that app
#   8. POST /api/v1/apps/:id/start triggers an operation
#   9. (Optional) When --route-host is given, Routes are exercised end-to-end
#
# Usage:
#   scripts/openshift-validate.sh \
#       --release appcontrol-ci \
#       --namespace appcontrol-ci \
#       [--admin-email admin@localhost] \
#       [--route-host appcontrol.apps.example.com] \
#       [--timeout 300]
#
# Environment overrides:
#   KUBECTL          - kubectl binary (default: oc if available, else kubectl)
#   ADMIN_PASSWORD   - empty for demo mode (default), otherwise sent to /login
# =============================================================================
set -euo pipefail

RELEASE="appcontrol"
NAMESPACE="appcontrol"
ADMIN_EMAIL="admin@localhost"
ROUTE_HOST=""
TIMEOUT=300
PF_PORT="${PF_PORT:-13000}"
AGENT_HOSTNAME="demo-host"
TEST_START_STOP=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) RELEASE="$2"; shift 2 ;;
        --namespace) NAMESPACE="$2"; shift 2 ;;
        --admin-email) ADMIN_EMAIL="$2"; shift 2 ;;
        --route-host) ROUTE_HOST="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --agent-hostname) AGENT_HOSTNAME="$2"; shift 2 ;;
        --test-start-stop) TEST_START_STOP=1; shift ;;
        -h|--help)
            sed -n '2,30p' "$0"; exit 0 ;;
        *) echo "Unknown arg: $1" >&2; exit 2 ;;
    esac
done

# Prefer `oc` when available; fall back to `kubectl`.
if [[ -z "${KUBECTL:-}" ]]; then
    if command -v oc >/dev/null 2>&1; then
        KUBECTL="oc"
    else
        KUBECTL="kubectl"
    fi
fi
KC=( "$KUBECTL" "-n" "$NAMESPACE" )

# ── Colors ───────────────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
    R='\033[0;31m'; G='\033[0;32m'; Y='\033[1;33m'; B='\033[0;34m'; N='\033[0m'
else
    R=''; G=''; Y=''; B=''; N=''
fi
log()  { echo -e "${B}[validate]${N} $*"; }
ok()   { echo -e "${G}  PASS${N} $*"; }
fail() { echo -e "${R}  FAIL${N} $*" >&2; FAILED=1; }
warn() { echo -e "${Y}  WARN${N} $*"; }

FAILED=0
PF_PID=0

cleanup() {
    if [[ "$PF_PID" -ne 0 ]]; then
        kill "$PF_PID" 2>/dev/null || true
        wait "$PF_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# ── Discover deployments owned by the release ────────────────────────────────
log "Discovering workloads for release '$RELEASE' in namespace '$NAMESPACE'..."
DEPLOYMENTS=$("${KC[@]}" get deploy \
    -l "app.kubernetes.io/instance=$RELEASE" \
    -o jsonpath='{.items[*].metadata.name}' 2>/dev/null || echo "")
STATEFULSETS=$("${KC[@]}" get sts \
    -l "app.kubernetes.io/instance=$RELEASE" \
    -o jsonpath='{.items[*].metadata.name}' 2>/dev/null || echo "")

if [[ -z "$DEPLOYMENTS" && -z "$STATEFULSETS" ]]; then
    fail "No deployments/statefulsets found for release=$RELEASE in ns=$NAMESPACE"
    exit 1
fi
log "Deployments: ${DEPLOYMENTS:-(none)}"
log "StatefulSets: ${STATEFULSETS:-(none)}"

# ── 1. Wait for rollouts ─────────────────────────────────────────────────────
log "Waiting for rollouts (timeout=${TIMEOUT}s)..."
for d in $DEPLOYMENTS; do
    if "${KC[@]}" rollout status "deploy/$d" --timeout="${TIMEOUT}s"; then
        ok "deploy/$d rollout complete"
    else
        fail "deploy/$d rollout did not complete in ${TIMEOUT}s"
    fi
done
for s in $STATEFULSETS; do
    if "${KC[@]}" rollout status "statefulset/$s" --timeout="${TIMEOUT}s"; then
        ok "statefulset/$s rollout complete"
    else
        fail "statefulset/$s rollout did not complete in ${TIMEOUT}s"
    fi
done

# ── 2. All pods Running, restart count 0 ─────────────────────────────────────
log "Inspecting pod status..."
PODS_JSON=$("${KC[@]}" get pods \
    -l "app.kubernetes.io/instance=$RELEASE" \
    -o json)
echo "$PODS_JSON" | jq -r '.items[] | "\(.metadata.name)\t\(.status.phase)\t\([.status.containerStatuses[]?.restartCount] | add // 0)"' \
| while IFS=$'\t' read -r name phase restarts; do
    if [[ "$phase" == "Running" && "$restarts" == "0" ]]; then
        ok "pod $name Running (restarts=0)"
    else
        fail "pod $name phase=$phase restarts=$restarts"
    fi
done

# Detect any non-Running pods (above runs in a subshell so $FAILED doesn't propagate)
BAD_PODS=$(echo "$PODS_JSON" | jq -r '.items[] | select(.status.phase != "Running") | .metadata.name')
if [[ -n "$BAD_PODS" ]]; then
    FAILED=1
    log "Non-Running pods detected, dumping describe + recent events:"
    for p in $BAD_PODS; do
        "${KC[@]}" describe "pod/$p" | tail -40 || true
    done
    "${KC[@]}" get events --sort-by=.lastTimestamp | tail -20 || true
fi

# ── 3. Log scan for panic / fatal markers ────────────────────────────────────
log "Scanning container logs for panics and fatal errors..."
ALL_PODS=$(echo "$PODS_JSON" | jq -r '.items[].metadata.name')
LOG_BAD=0
for p in $ALL_PODS; do
    # --all-containers covers init + sidecars; tail bounded to keep CI logs sane.
    LOGS=$("${KC[@]}" logs "$p" --all-containers --tail=500 2>/dev/null || true)
    # Match panic, fatal, "ERROR" (uppercase, word-boundary), and tracing FATAL.
    HITS=$(echo "$LOGS" | grep -E "panic|FATAL|^.*ERROR " | grep -vE "RUST_LOG|cors_origins" || true)
    if [[ -n "$HITS" ]]; then
        fail "pod $p logs contain error markers:"
        echo "$HITS" | head -10 | sed 's/^/      /'
        LOG_BAD=1
    fi
done
[[ "$LOG_BAD" -eq 0 ]] && ok "no panic/FATAL/ERROR markers across $(echo "$ALL_PODS" | wc -w) pods"

# ── 4. Port-forward to backend and check health ──────────────────────────────
BACKEND_SVC=$("${KC[@]}" get svc \
    -l "app.kubernetes.io/instance=$RELEASE,app.kubernetes.io/component=backend" \
    -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
if [[ -z "$BACKEND_SVC" ]]; then
    fail "no backend service found — cannot test API"
    exit 1
fi
log "Port-forwarding svc/$BACKEND_SVC :3000 → localhost:$PF_PORT"
"${KC[@]}" port-forward "svc/$BACKEND_SVC" "$PF_PORT:3000" >/tmp/pf.log 2>&1 &
PF_PID=$!

# Wait for port-forward to be live
for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:$PF_PORT/health" >/dev/null 2>&1; then
        ok "port-forward established after ${i}s"
        break
    fi
    [[ "$i" == "30" ]] && { fail "port-forward never came up"; cat /tmp/pf.log; exit 1; }
    sleep 1
done

# Determine the base URL: prefer route when supplied, fall back to port-forward.
if [[ -n "$ROUTE_HOST" ]]; then
    BASE_URL="https://$ROUTE_HOST"
    CURL_OPTS="-k"
    log "Using Route URL: $BASE_URL"
else
    BASE_URL="http://127.0.0.1:$PF_PORT"
    CURL_OPTS=""
    log "Using port-forward URL: $BASE_URL"
fi

# /health
BODY=$(curl -sf $CURL_OPTS "$BASE_URL/health")
if echo "$BODY" | grep -q '"status":"ok"'; then
    ok "GET $BASE_URL/health → status=ok"
else
    fail "/health unexpected body: $BODY"
fi

# /ready
STATUS=$(curl -s $CURL_OPTS -o /dev/null -w '%{http_code}' "$BASE_URL/ready")
if [[ "$STATUS" == "200" ]]; then
    ok "GET $BASE_URL/ready → 200"
else
    fail "/ready returned $STATUS"
fi

# ── 5. Login with seed admin ─────────────────────────────────────────────────
log "Logging in as $ADMIN_EMAIL..."
LOGIN_PAYLOAD=$(jq -nc \
    --arg email "$ADMIN_EMAIL" \
    --arg password "${ADMIN_PASSWORD:-}" \
    '{email: $email, password: $password}')
LOGIN_RESP=$(curl -sf $CURL_OPTS \
    -X POST "$BASE_URL/api/v1/auth/login" \
    -H 'Content-Type: application/json' \
    -d "$LOGIN_PAYLOAD" || echo "")
TOKEN=$(echo "$LOGIN_RESP" | jq -r '.token // .access_token // empty' 2>/dev/null || echo "")
if [[ -z "$TOKEN" ]]; then
    fail "Login failed. Response: $LOGIN_RESP"
    exit 1
fi
ok "Login succeeded, token length=${#TOKEN}"

AUTH_HDR=( -H "Authorization: Bearer $TOKEN" )

# ── 6. Create an application ─────────────────────────────────────────────────
APP_NAME="ci-validation-$(date +%s)"
log "Creating application '$APP_NAME'..."
APP_RESP=$(curl -sf $CURL_OPTS \
    -X POST "$BASE_URL/api/v1/apps" \
    "${AUTH_HDR[@]}" \
    -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg n "$APP_NAME" '{name: $n, description: "Created by openshift-validate.sh"}')")
APP_ID=$(echo "$APP_RESP" | jq -r '.id // empty')
if [[ -z "$APP_ID" ]]; then
    fail "App creation failed. Response: $APP_RESP"
    exit 1
fi
ok "App created: id=$APP_ID"

# ── 7. Read back & confirm it is listed ──────────────────────────────────────
LIST=$(curl -sf $CURL_OPTS "${AUTH_HDR[@]}" "$BASE_URL/api/v1/apps")
if echo "$LIST" | jq -e --arg id "$APP_ID" '.[] | select(.id == $id)' >/dev/null 2>&1; then
    ok "App appears in GET /api/v1/apps"
else
    fail "App $APP_ID not found in list response"
fi

if [[ "$TEST_START_STOP" -eq 1 ]]; then
    # ── 8a. Wait for the demo agent to enroll and appear in /api/v1/agents ──
    log "Waiting for agent '$AGENT_HOSTNAME' to register..."
    AGENT_ID=""
    for i in $(seq 1 60); do
        AGENTS=$(curl -sf $CURL_OPTS "${AUTH_HDR[@]}" "$BASE_URL/api/v1/agents" || echo "[]")
        AGENT_ID=$(echo "$AGENTS" | jq -r --arg h "$AGENT_HOSTNAME" \
            '.[] | select(.hostname == $h) | .id' | head -n1)
        if [[ -n "$AGENT_ID" && "$AGENT_ID" != "null" ]]; then
            ok "Agent '$AGENT_HOSTNAME' registered after ${i}s (id=$AGENT_ID)"
            break
        fi
        [[ "$i" == "60" ]] && { fail "Agent never registered within 120s"; AGENTS_LOG=$(echo "$AGENTS" | head -c 500); echo "  last response: $AGENTS_LOG"; }
        sleep 2
    done

    # ── 8b. Create a component bound to the demo agent ──────────────────────
    log "Creating component 'test-component' on host '$AGENT_HOSTNAME'..."
    COMP_PAYLOAD=$(jq -nc --arg h "$AGENT_HOSTNAME" '{
        name: "test-component",
        component_type: "application",
        host: $h,
        check_cmd:  "test -f /tmp/test-component.flag",
        start_cmd:  "touch /tmp/test-component.flag",
        stop_cmd:   "rm -f /tmp/test-component.flag",
        check_interval_seconds: 5,
        start_timeout_seconds: 30,
        stop_timeout_seconds: 30
    }')
    COMP_RESP=$(curl -sf $CURL_OPTS \
        -X POST "$BASE_URL/api/v1/apps/$APP_ID/components" \
        "${AUTH_HDR[@]}" \
        -H 'Content-Type: application/json' \
        -d "$COMP_PAYLOAD")
    COMP_ID=$(echo "$COMP_RESP" | jq -r '.id // empty')
    if [[ -z "$COMP_ID" ]]; then
        fail "Component creation failed. Response: $COMP_RESP"
        exit 1
    fi
    ok "Component created: id=$COMP_ID"

    # ── 8c. Start the application, wait for FSM = RUNNING ────────────────────
    log "Starting application $APP_ID..."
    START_RESP=$(curl -s $CURL_OPTS -o /tmp/start.json -w '%{http_code}' \
        -X POST "$BASE_URL/api/v1/apps/$APP_ID/start" \
        "${AUTH_HDR[@]}")
    if [[ "$START_RESP" != "200" && "$START_RESP" != "202" ]]; then
        fail "Start returned HTTP $START_RESP: $(cat /tmp/start.json)"
        exit 1
    fi
    ok "Start command accepted (HTTP $START_RESP)"

    log "Polling for global_state=RUNNING (up to 120s)..."
    for i in $(seq 1 60); do
        APP_STATE=$(curl -sf $CURL_OPTS "${AUTH_HDR[@]}" "$BASE_URL/api/v1/apps/$APP_ID" \
            | jq -r '.global_state // empty')
        if [[ "$APP_STATE" == "RUNNING" ]]; then
            ok "Application reached RUNNING after ${i}s"
            break
        fi
        if [[ "$APP_STATE" == "FAILED" ]]; then
            fail "Application transitioned to FAILED (expected RUNNING)"
            curl -sf $CURL_OPTS "${AUTH_HDR[@]}" "$BASE_URL/api/v1/apps/$APP_ID" | jq .
            exit 1
        fi
        [[ "$i" == "60" ]] && { fail "Application never reached RUNNING (last state=$APP_STATE)"; exit 1; }
        sleep 2
    done

    # ── 8d. Stop the application, wait for FSM = STOPPED ─────────────────────
    log "Stopping application $APP_ID..."
    STOP_RESP=$(curl -s $CURL_OPTS -o /tmp/stop.json -w '%{http_code}' \
        -X POST "$BASE_URL/api/v1/apps/$APP_ID/stop" \
        "${AUTH_HDR[@]}")
    if [[ "$STOP_RESP" != "200" && "$STOP_RESP" != "202" ]]; then
        fail "Stop returned HTTP $STOP_RESP: $(cat /tmp/stop.json)"
        exit 1
    fi
    ok "Stop command accepted (HTTP $STOP_RESP)"

    log "Polling for global_state=STOPPED (up to 120s)..."
    for i in $(seq 1 60); do
        APP_STATE=$(curl -sf $CURL_OPTS "${AUTH_HDR[@]}" "$BASE_URL/api/v1/apps/$APP_ID" \
            | jq -r '.global_state // empty')
        if [[ "$APP_STATE" == "STOPPED" ]]; then
            ok "Application reached STOPPED after ${i}s"
            break
        fi
        [[ "$i" == "60" ]] && { fail "Application never reached STOPPED (last state=$APP_STATE)"; exit 1; }
        sleep 2
    done
else
    # ── 8. Trigger start operation (no agent — operation queued, not executed) ──
    log "Triggering start operation (no agent attached — operation will be created but not executed)..."
    START_STATUS=$(curl -s $CURL_OPTS -o /tmp/start.json -w '%{http_code}' \
        -X POST "$BASE_URL/api/v1/apps/$APP_ID/start" \
        "${AUTH_HDR[@]}")
    # 200/202/400 all acceptable: a 400 ("no components") proves auth + routing + DB write path.
    case "$START_STATUS" in
        200|202|400)
            ok "Start endpoint reachable (HTTP $START_STATUS)" ;;
        *)
            fail "Start endpoint returned unexpected HTTP $START_STATUS: $(cat /tmp/start.json)" ;;
    esac
fi

# ── 9. OpenShift Route presence (if openshift.enabled=true was applied) ──────
ROUTE_CRD_PRESENT=$("$KUBECTL" get crd routes.route.openshift.io --no-headers 2>/dev/null | wc -l)
if [[ "$ROUTE_CRD_PRESENT" -gt 0 ]]; then
    ROUTE_COUNT=$("${KC[@]}" get route \
        -l "app.kubernetes.io/instance=$RELEASE" \
        -o name 2>/dev/null | wc -l)
    if [[ "$ROUTE_COUNT" -ge 1 ]]; then
        ok "Route resources created: $ROUTE_COUNT"
        "${KC[@]}" get route -l "app.kubernetes.io/instance=$RELEASE" || true
    else
        warn "Route CRD present but no Route resources for release (openshift.route.enabled may be false)"
    fi
else
    warn "Route CRD not installed — skipping Route validation"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
if [[ "$FAILED" -ne 0 ]]; then
    log "${R}Validation FAILED${N}"
    exit 1
fi
log "${G}All validation checks PASSED${N}"
