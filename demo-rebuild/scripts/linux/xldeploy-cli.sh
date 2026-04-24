#!/usr/bin/env bash
# Mock XL Deploy CLI
set -euo pipefail

DEMO_DIR="/tmp/appcontrol-demo-rebuild"
LOG_FILE="$DEMO_DIR/xldeploy.log"
TS="$(date -Is)"

echo "[$TS] XL DEPLOY CLI INVOKED" >> "$LOG_FILE"
echo "  args: $*" >> "$LOG_FILE"

PKG=""
TARGET=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --package) PKG="$2"; shift 2;;
        --target)  TARGET="$2"; shift 2;;
        *)         shift;;
    esac
done

echo "[$TS]   package=$PKG target=$TARGET" >> "$LOG_FILE"

echo
echo "[XL DEPLOY] Deployment triggered"
echo "[XL DEPLOY]   Package: $PKG"
echo "[XL DEPLOY]   Target:  $TARGET"
echo "[XL DEPLOY]   Phase 1/3: Pre-deploy checks..."
sleep 1
echo "[XL DEPLOY]   Phase 2/3: Copying artifacts to target..."
sleep 1
echo "[XL DEPLOY]   Phase 3/3: Activating new version..."
sleep 1

if [[ -n "$TARGET" ]]; then
    mkdir -p "$DEMO_DIR/primary"
    case "$TARGET" in
        jboss-prd-*)
            NODE=$(echo "$TARGET" | grep -oP 'jboss-prd-\K[0-9]+')
            [[ -n "$NODE" ]] && touch "$DEMO_DIR/primary/jboss-${NODE}.running" \
                && echo "[XL DEPLOY]   Marker restored: $DEMO_DIR/primary/jboss-${NODE}.running"
            ;;
        mq-prd*)
            touch "$DEMO_DIR/primary/mq-series.running"
            echo "[XL DEPLOY]   Marker restored: $DEMO_DIR/primary/mq-series.running"
            ;;
        wsp-prd*)
            touch "$DEMO_DIR/primary/websphere-portal.running"
            echo "[XL DEPLOY]   Marker restored: $DEMO_DIR/primary/websphere-portal.running"
            ;;
    esac
fi

echo "[XL DEPLOY] Deployment complete. Target is healthy."
echo "[$TS] Deployment complete" >> "$LOG_FILE"
exit 0
