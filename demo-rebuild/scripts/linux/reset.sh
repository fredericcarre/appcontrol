#!/usr/bin/env bash
# Reset demo state
DEMO_DIR="/tmp/appcontrol-demo-rebuild"

rm -f "$DEMO_DIR/primary"/*.running 2>/dev/null
rm -f "$DEMO_DIR/dr"/*.running      2>/dev/null
rm -f "$DEMO_DIR/xldeploy.log"      2>/dev/null
rm -f "$DEMO_DIR/xlrelease.log"     2>/dev/null

echo "[RESET] Markers and logs cleared. Clean state ready."
echo
echo "You can now:"
echo "  - From AppControl: click 'Start app' on the Critical Banking App map"
echo "  - Or re-run the full demo scenario"
