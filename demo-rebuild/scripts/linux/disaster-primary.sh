#!/usr/bin/env bash
# Simulate complete primary-site outage
DEMO_DIR="/tmp/appcontrol-demo-rebuild"
PRIMARY_DIR="$DEMO_DIR/primary"

if [[ -d "$PRIMARY_DIR" ]]; then
    rm -f "$PRIMARY_DIR"/*.running 2>/dev/null
    echo "[DISASTER] Primary site: major outage simulated"
    echo "[DISASTER] All PRIMARY markers removed"
    echo "[DISASTER] Components will transition to FAILED within 10-30s"
    echo
    echo "Next step:"
    echo "  1. Open the application in AppControl"
    echo "  2. Click 'Switchover' -> select DR site"
    echo "  3. Walk through the 6 phases"
else
    echo "[DISASTER] Directory $PRIMARY_DIR not found. Run setup.sh first."
fi
