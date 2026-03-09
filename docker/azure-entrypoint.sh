#!/usr/bin/env bash
# =============================================================================
# Azure Gateway + Agent Entrypoint
# Handles Azure Managed Identity login, enrollment, then starts supervisord.
# =============================================================================
set -euo pipefail

echo "============================================"
echo "  AppControl Azure Gateway + Agent"
echo "============================================"

# ── Azure Managed Identity login ──────────────────────────────────────────────
if [ "${AZURE_AUTH_ENABLED:-true}" = "true" ]; then
    echo "[INFO] Logging in with Azure Managed Identity..."

    if [ -n "${AZURE_CLIENT_ID:-}" ]; then
        # User-assigned managed identity
        az login --identity --username "${AZURE_CLIENT_ID}" --output none 2>/dev/null && \
            echo "[OK]   Logged in with user-assigned identity: ${AZURE_CLIENT_ID}" || \
            echo "[WARN] Azure login failed — commands requiring Azure access will fail"
    else
        # System-assigned managed identity
        az login --identity --output none 2>/dev/null && \
            echo "[OK]   Logged in with system-assigned managed identity" || \
            echo "[WARN] Azure login failed — commands requiring Azure access will fail"
    fi

    # Set default subscription if provided
    if [ -n "${AZURE_SUBSCRIPTION_ID:-}" ]; then
        az account set --subscription "${AZURE_SUBSCRIPTION_ID}" --output none
        echo "[OK]   Subscription set to: ${AZURE_SUBSCRIPTION_ID}"
    fi

    # Show identity info
    az account show --query '{subscription:name, tenantId:tenantId}' --output table 2>/dev/null || true
fi

# ── Gateway enrollment ────────────────────────────────────────────────────────
GATEWAY_CONFIG="/etc/appcontrol/gateway.yaml"
GATEWAY_ENROLLED_FLAG="/var/lib/appcontrol/.gateway_enrolled"

if [ -n "${GATEWAY_ENROLLMENT_TOKEN:-}" ] && [ ! -f "${GATEWAY_ENROLLED_FLAG}" ]; then
    echo "[INFO] Enrolling gateway with backend..."

    # Run gateway enrollment
    if /usr/local/bin/appcontrol-gateway enroll \
        --backend-url "${BACKEND_URL:-ws://localhost:3000/ws/gateway}" \
        --token "${GATEWAY_ENROLLMENT_TOKEN}" \
        --gateway-id "${GATEWAY_ID:-azure-gateway}" \
        --zone "${GATEWAY_ZONE:-azure}" \
        --config "${GATEWAY_CONFIG}" 2>&1; then
        echo "[OK]   Gateway enrolled successfully"
        touch "${GATEWAY_ENROLLED_FLAG}"
    else
        echo "[WARN] Gateway enrollment failed — will generate default config"
    fi
fi

# ── Generate gateway config if not enrolled and not mounted ───────────────────
if [ ! -f "${GATEWAY_CONFIG}" ]; then
    echo "[INFO] Generating gateway config: ${GATEWAY_CONFIG}"
    cat > "${GATEWAY_CONFIG}" <<EOF
gateway:
  id: "${GATEWAY_ID:-azure-gateway}"
  zone: "${GATEWAY_ZONE:-azure}"
  listen_addr: "0.0.0.0"
  listen_port: 4443

backend:
  url: "${BACKEND_URL:-ws://localhost:3000/ws/gateway}"
  reconnect_interval_secs: 5

tls:
  enabled: false
EOF
    chown appcontrol:appcontrol "${GATEWAY_CONFIG}"
fi

# ── Agent enrollment ──────────────────────────────────────────────────────────
AGENT_CONFIG="/etc/appcontrol/agent.yaml"
AGENT_ENROLLED_FLAG="/var/lib/appcontrol/.agent_enrolled"
AGENT_ID="${AGENT_ID:-$(hostname)}"

if [ -n "${AGENT_ENROLLMENT_TOKEN:-}" ] && [ ! -f "${AGENT_ENROLLED_FLAG}" ]; then
    echo "[INFO] Enrolling agent with gateway..."

    # Wait for gateway to be ready (it starts in parallel via supervisord, but we do enrollment first)
    # For embedded agent, connect to local gateway
    AGENT_GATEWAY_URL="${AGENT_GATEWAY_URL:-ws://127.0.0.1:4443/ws}"

    if /usr/local/bin/appcontrol-agent enroll \
        --gateway-url "${AGENT_GATEWAY_URL}" \
        --token "${AGENT_ENROLLMENT_TOKEN}" \
        --agent-id "${AGENT_ID}" \
        --config "${AGENT_CONFIG}" 2>&1; then
        echo "[OK]   Agent enrolled successfully"
        touch "${AGENT_ENROLLED_FLAG}"
    else
        echo "[WARN] Agent enrollment failed — will generate default config"
    fi
fi

# ── Generate agent config if not enrolled and not mounted ─────────────────────
if [ ! -f "${AGENT_CONFIG}" ]; then
    GATEWAY_URL="${GATEWAY_URL:-ws://127.0.0.1:4443/ws}"
    echo "[INFO] Generating agent config: ${AGENT_CONFIG}"
    cat > "${AGENT_CONFIG}" <<EOF
agent:
  id: "${AGENT_ID}"
  mode: "active"

gateway:
  url: "${GATEWAY_URL}"

tls:
  enabled: false

labels:
  provider: azure
  role: vm-controller
  zone: "${GATEWAY_ZONE:-azure}"
EOF
    chown appcontrol:appcontrol "${AGENT_CONFIG}"
fi

echo ""
echo "[INFO] Gateway ID:   ${GATEWAY_ID:-azure-gateway}"
echo "[INFO] Backend URL:  ${BACKEND_URL:-ws://localhost:3000/ws/gateway}"
echo "[INFO] Agent ID:     ${AGENT_ID}"
echo "[INFO] Starting supervisord..."
echo ""

# ── Start supervisord ─────────────────────────────────────────────────────────
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/appcontrol.conf
