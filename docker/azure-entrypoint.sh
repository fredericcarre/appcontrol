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

# ── Gateway config ────────────────────────────────────────────────────────────
# Note: Gateway enrollment is handled by the gateway itself when connecting to backend
GATEWAY_CONFIG="/etc/appcontrol/gateway.yaml"

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
EOF
    chown appcontrol:appcontrol "${GATEWAY_CONFIG}"
fi

# ── Agent config ──────────────────────────────────────────────────────────────
AGENT_CONFIG="/etc/appcontrol/agent.yaml"
AGENT_ID="${AGENT_ID:-$(hostname)}"

# ── Generate agent config if not mounted ──────────────────────────────────────
if [ ! -f "${AGENT_CONFIG}" ]; then
    GATEWAY_URL="${GATEWAY_URL:-ws://127.0.0.1:4443/ws}"
    echo "[INFO] Generating agent config: ${AGENT_CONFIG}"
    cat > "${AGENT_CONFIG}" <<EOF
agent:
  id: "${AGENT_ID}"
  mode: "active"

gateway:
  url: "${GATEWAY_URL}"

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
