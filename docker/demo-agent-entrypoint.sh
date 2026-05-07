#!/bin/bash
# docker/demo-agent-entrypoint.sh
#
# Bootstraps the demo agent: waits for the enrollment token written by
# scripts/seed-demo.sh, performs one-shot enrollment if needed, then
# execs into the regular agent process.

set -euo pipefail

TOKEN_FILE="${TOKEN_FILE:-/var/lib/appcontrol/state/enrollment-token}"
GATEWAY_URL="${GATEWAY_URL:-wss://gateway:4443}"
ENROLL_DIR="${ENROLL_DIR:-/var/lib/appcontrol}"
CONFIG_FILE="${CONFIG_FILE:-$ENROLL_DIR/agent.yaml}"
# AGENT_HOSTNAME is read by the agent during enrollment. Must match the
# `host` value of the components imported by scripts/seed-demo.sh so
# that auto-bind links every component to this single agent.
export AGENT_HOSTNAME="${AGENT_HOSTNAME:-demo-host}"

log() { echo "[demo-agent] $*"; }
fail() { echo "[demo-agent] ERROR: $*" >&2; exit 1; }

# Wait for the token (the seeder writes it once it has logged in and
# created an enrollment token). 5 minutes max.
log "Waiting for enrollment token at $TOKEN_FILE"
for i in $(seq 1 150); do
  if [ -s "$TOKEN_FILE" ]; then
    log "Token available after $((i * 2))s"
    break
  fi
  if [ "$i" = "150" ]; then
    fail "Enrollment token did not appear within 300s"
  fi
  sleep 2
done

# Enroll if we have not done so yet (the agent.yaml file is the
# evidence that a previous run completed successfully).
if [ ! -f "$CONFIG_FILE" ]; then
  TOKEN=$(cat "$TOKEN_FILE")
  log "Enrolling against $GATEWAY_URL as hostname=$AGENT_HOSTNAME"
  /usr/local/bin/appcontrol-agent \
    --enroll "$GATEWAY_URL" \
    --token "$TOKEN" \
    --enroll-dir "$ENROLL_DIR"
  log "Enrollment complete"
else
  log "Already enrolled (config present at $CONFIG_FILE)"
fi

# Hand off to the long-running agent process.
log "Starting agent with config $CONFIG_FILE"
exec /usr/local/bin/appcontrol-agent --config "$CONFIG_FILE"
