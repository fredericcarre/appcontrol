#!/usr/bin/env bash
# =============================================================================
# Agent wrapper: enrolls the agent via the gateway, then starts the agent
# =============================================================================
set -euo pipefail

AGENT_DATA_DIR="/var/lib/appcontrol"
AGENT_CONFIG="/etc/appcontrol/agent.yaml"
GATEWAY_ZONE="${GATEWAY_ZONE:-azure}"

# Wait for gateway to be ready
echo "[AGENT] Waiting for gateway to be ready..."
for i in $(seq 1 60); do
    if curl -sk https://127.0.0.1:4443/health >/dev/null 2>&1; then
        echo "[AGENT] Gateway is ready"
        break
    fi
    sleep 1
done

# Enroll if we have a token and no certificate yet
if [ ! -f "${AGENT_DATA_DIR}/tls/agent.crt" ] && [ -n "${AGENT_ENROLLMENT_TOKEN:-}" ]; then
    HOSTNAME=$(hostname)
    echo "[AGENT] Enrolling agent '${HOSTNAME}' with enrollment token..."

    ENROLL_URL="https://127.0.0.1:4443/enroll"
    RESP=$(curl -sk -X POST "${ENROLL_URL}" \
        -H "Content-Type: application/json" \
        -d "{\"token\": \"${AGENT_ENROLLMENT_TOKEN}\", \"hostname\": \"${HOSTNAME}\"}" \
        2>&1)

    if echo "${RESP}" | jq -e '.cert_pem' >/dev/null 2>&1; then
        # Extract and save certificates
        mkdir -p "${AGENT_DATA_DIR}/tls"
        echo "${RESP}" | jq -r '.cert_pem' > "${AGENT_DATA_DIR}/tls/agent.crt"
        echo "${RESP}" | jq -r '.key_pem' > "${AGENT_DATA_DIR}/tls/agent.key"
        echo "${RESP}" | jq -r '.ca_pem' > "${AGENT_DATA_DIR}/tls/ca.crt"
        chmod 600 "${AGENT_DATA_DIR}/tls/agent.key"

        ENROLLED_AGENT_ID=$(echo "${RESP}" | jq -r '.agent_id // "auto"')
        echo "[AGENT] Enrolled successfully with ID: ${ENROLLED_AGENT_ID}"

        # Generate agent config with mTLS
        cat > "${AGENT_CONFIG}" <<EOF
agent:
  id: "${ENROLLED_AGENT_ID}"
  mode: "active"

gateway:
  url: "wss://127.0.0.1:4443/ws"
  reconnect_interval_secs: 10

tls:
  enabled: true
  cert_file: "${AGENT_DATA_DIR}/tls/agent.crt"
  key_file: "${AGENT_DATA_DIR}/tls/agent.key"
  ca_file: "${AGENT_DATA_DIR}/tls/ca.crt"

labels:
  provider: azure
  role: vm-controller
  zone: "${GATEWAY_ZONE}"

data_dir: "${AGENT_DATA_DIR}"
EOF
    else
        echo "[AGENT] Enrollment failed: ${RESP}"
        echo "[AGENT] Falling back to insecure mode (may not connect)"
        HOSTNAME=$(hostname)
        cat > "${AGENT_CONFIG}" <<EOF
agent:
  id: "${HOSTNAME}"
  mode: "active"

gateway:
  url: "wss://127.0.0.1:4443/ws"
  tls_insecure: true

labels:
  provider: azure
  role: vm-controller
  zone: "${GATEWAY_ZONE}"

data_dir: "${AGENT_DATA_DIR}"
EOF
    fi
elif [ -f "${AGENT_DATA_DIR}/tls/agent.crt" ]; then
    echo "[AGENT] Already enrolled (certificate exists)"
else
    echo "[AGENT] No enrollment token, using insecure mode"
fi

# Start the agent
echo "[AGENT] Starting appcontrol-agent..."
exec /usr/local/bin/appcontrol-agent
