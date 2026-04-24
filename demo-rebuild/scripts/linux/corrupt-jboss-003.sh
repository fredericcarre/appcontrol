#!/usr/bin/env bash
# Simulate corruption of JBoss member 003
DEMO_DIR="/tmp/appcontrol-demo-rebuild"
MARKER="$DEMO_DIR/primary/jboss-003.running"

if [[ -f "$MARKER" ]]; then
    rm -f "$MARKER"
    echo "[CORRUPT] JBoss member 003: service down"
    echo "[CORRUPT] Marker removed: $MARKER"
    echo "[CORRUPT] check_cmd will detect the failure within 10s"
else
    echo "[CORRUPT] JBoss 003 marker already absent - component already KO."
fi
