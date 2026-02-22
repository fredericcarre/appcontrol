#!/bin/bash
# Start a test process. Creates a PID file and runs a background loop.
# Usage: start_process.sh <name> [pid_dir]
# Exit 0 = success, Exit 1 = failure

NAME="${1:?Usage: start_process.sh <name> [pid_dir]}"
PID_DIR="${2:-/tmp/appcontrol-e2e}"

mkdir -p "$PID_DIR"
PID_FILE="$PID_DIR/${NAME}.pid"

# If already running, exit success
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        echo "Process $NAME already running (PID $PID)"
        exit 0
    fi
    # Stale PID file
    rm -f "$PID_FILE"
fi

# Start the process: a simple sleep loop that writes a heartbeat file
(
    while true; do
        echo "$(date -Iseconds)" > "$PID_DIR/${NAME}.heartbeat"
        sleep 1
    done
) &

PID=$!
echo "$PID" > "$PID_FILE"
echo "Started $NAME with PID $PID"
exit 0
