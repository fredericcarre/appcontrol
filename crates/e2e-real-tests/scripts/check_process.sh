#!/bin/bash
# Check if a test process is alive.
# Usage: check_process.sh <name> [pid_dir]
# Exit 0 = running (healthy), Exit 1 = not running (failed)

NAME="${1:?Usage: check_process.sh <name> [pid_dir]}"
PID_DIR="${2:-/tmp/appcontrol-e2e}"

PID_FILE="$PID_DIR/${NAME}.pid"

if [ ! -f "$PID_FILE" ]; then
    exit 1
fi

PID=$(cat "$PID_FILE")
if kill -0 "$PID" 2>/dev/null; then
    exit 0
else
    exit 1
fi
