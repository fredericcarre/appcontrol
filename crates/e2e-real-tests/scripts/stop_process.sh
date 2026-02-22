#!/bin/bash
# Stop a test process. Reads PID file and kills the process.
# Usage: stop_process.sh <name> [pid_dir]
# Exit 0 = success, Exit 1 = failure

NAME="${1:?Usage: stop_process.sh <name> [pid_dir]}"
PID_DIR="${2:-/tmp/appcontrol-e2e}"

PID_FILE="$PID_DIR/${NAME}.pid"

if [ ! -f "$PID_FILE" ]; then
    echo "Process $NAME not running (no PID file)"
    exit 0
fi

PID=$(cat "$PID_FILE")

if kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null
    # Wait for process to die (max 5s)
    for i in $(seq 1 50); do
        if ! kill -0 "$PID" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    # Force kill if still alive
    if kill -0 "$PID" 2>/dev/null; then
        kill -9 "$PID" 2>/dev/null
    fi
fi

rm -f "$PID_FILE" "$PID_DIR/${NAME}.heartbeat"
echo "Stopped $NAME (PID $PID)"
exit 0
