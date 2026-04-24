#!/usr/bin/env bash
# Setup - creates directories and installs mock CLIs
set -euo pipefail

DEMO_DIR="/tmp/appcontrol-demo-rebuild"
BIN_DIR="$DEMO_DIR/bin"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

mkdir -p "$DEMO_DIR/primary" "$DEMO_DIR/dr" "$BIN_DIR"

cp "$SCRIPT_DIR/xldeploy-cli.sh" "$BIN_DIR/xldeploy-cli.sh"
cp "$SCRIPT_DIR/xlr-cli.sh"      "$BIN_DIR/xlr-cli.sh"
chmod +x "$BIN_DIR/xldeploy-cli.sh" "$BIN_DIR/xlr-cli.sh"

cat <<EOF
[SETUP] Directories created:
  $DEMO_DIR/primary    (primary site markers)
  $DEMO_DIR/dr         (DR site markers)
  $BIN_DIR             (mock XL Deploy / XL Release)

[SETUP] Mock CLIs installed in $BIN_DIR

Ready for demo. Use in another terminal:
  watch -n 2 ls -la $DEMO_DIR/primary $DEMO_DIR/dr    live marker view
  $SCRIPT_DIR/corrupt-jboss-003.sh                     simulate one member down
  $SCRIPT_DIR/disaster-primary.sh                      simulate primary-site outage
  $SCRIPT_DIR/reset.sh                                 reset to clean state
EOF
