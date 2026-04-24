#!/usr/bin/env bash
# Mock XL Release CLI
set -euo pipefail

DEMO_DIR="/tmp/appcontrol-demo-rebuild"
LOG_FILE="$DEMO_DIR/xlrelease.log"
TS="$(date -Is)"

TEMPLATE=""
TARGET=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --template) TEMPLATE="$2"; shift 2;;
        --var)      if [[ "$2" == target=* ]]; then TARGET="${2#target=}"; fi; shift 2;;
        *)          shift;;
    esac
done

echo "[$TS] XL RELEASE CLI INVOKED" >> "$LOG_FILE"
echo "  template=$TEMPLATE target=$TARGET" >> "$LOG_FILE"

echo
echo "[XL RELEASE] Release pipeline triggered"
echo "[XL RELEASE]   Template: $TEMPLATE"
echo "[XL RELEASE]   Target:   $TARGET"
echo
echo "[XL RELEASE]   [Task 1/4] Pre-change approval (auto-approved for demo)..."
sleep 2
echo "[XL RELEASE]   [Task 2/4] Backup current state..."
sleep 1
echo "[XL RELEASE]   [Task 3/4] Executing rebuild playbook..."
sleep 2

if [[ "$TARGET" == *"oracle-rac"* ]]; then
    mkdir -p "$DEMO_DIR/primary"
    touch "$DEMO_DIR/primary/oracle-rac.running"
    echo "[XL RELEASE]   Marker restored: $DEMO_DIR/primary/oracle-rac.running"
fi

echo "[XL RELEASE]   [Task 4/4] Post-change validation..."
sleep 1
echo "[XL RELEASE] Release complete. All tasks successful."
echo "[$TS] Release complete" >> "$LOG_FILE"
exit 0
