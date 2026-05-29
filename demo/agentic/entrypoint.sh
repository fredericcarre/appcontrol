#!/usr/bin/env bash
# Start the real services, then run the agentic discovery → AI map chain.
set -euo pipefail

log() { echo "[agentic-demo] $*"; }

log "Starting PostgreSQL, Redis, nginx..."
service postgresql start >/dev/null 2>&1 || pg_ctlcluster "$(ls /etc/postgresql)" main start || true
redis-server --daemonize yes >/dev/null 2>&1 || service redis-server start || true
service nginx start >/dev/null 2>&1 || nginx || true

log "Starting the demo order-api (listens :8080, depends on PG + Redis)..."
python3 /opt/order-api/order-api.py &
# Give the order-api time to open its config and connect to PG/Redis.
sleep 8

echo
echo "==================================================================="
echo " STEP 1 — Agentic discovery (the agent scans THIS host, no backend)"
echo "==================================================================="
appcontrol-agent discover --json > /tmp/discovery.json
appcontrol-agent discover || true

echo
echo "==================================================================="
echo " STEP 2 — AI architect pass (raw discovery -> readable map)"
echo "==================================================================="
appcontrol-ai architect --input /tmp/discovery.json

echo
echo "Tip: inspect the raw discovery with:  cat /tmp/discovery.json | appcontrol-ai architect --json"
echo "Container stays up — re-run the chain anytime with:"
echo "  docker exec -it <container> bash -c 'appcontrol-agent discover --json | appcontrol-ai architect'"
exec tail -f /dev/null
