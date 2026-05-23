#!/bin/bash
# E2E test check: returns Nagios-style perfdata when the test process is alive.
# Validates that the agent extracts metrics from `STATUS | label=value;...` format.
# Usage: check_with_perfdata.sh <name> [pid_dir]

NAME="${1:?Usage: check_with_perfdata.sh <name> [pid_dir]}"
PID_DIR="${2:-/tmp/appcontrol-e2e}"
PID_FILE="$PID_DIR/${NAME}.pid"

if [ ! -f "$PID_FILE" ] || ! kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "CRITICAL - ${NAME} not running"
    exit 2
fi

# Stable numeric values so the integration test can assert exact equality.
echo "OK - ${NAME} healthy | active_connections=42 queue_depth=7;100;500;0; cpu_usage=23.5%;80;95"
exit 0
