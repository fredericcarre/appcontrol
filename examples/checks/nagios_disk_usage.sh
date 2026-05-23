#!/bin/bash
# Nagios-style check for disk usage with UOM and quoted labels.
#
# Demonstrates two perfdata features:
#   - Single-quoted labels (`'free space'=...`) for labels with spaces
#   - Unit-of-measure suffixes (`%`, `MB`) which AppControl strips when
#     extracting the numeric value
#
# Usage: nagios_disk_usage.sh [mountpoint]

set -e

MOUNT="${1:-/}"
WARN_PCT="${WARN_PCT:-80}"
CRIT_PCT="${CRIT_PCT:-90}"

USED_PCT=$(df --output=pcent "$MOUNT" | tail -1 | tr -d ' %')
FREE_MB=$(df -BM --output=avail "$MOUNT" | tail -1 | tr -d ' M')

STATUS="OK"
EXIT=0
if [ "$USED_PCT" -ge "$CRIT_PCT" ]; then
    STATUS="CRITICAL"
    EXIT=2
elif [ "$USED_PCT" -ge "$WARN_PCT" ]; then
    STATUS="WARNING"
    EXIT=1
fi

echo "${STATUS} - ${MOUNT} ${USED_PCT}% used | 'disk usage'=${USED_PCT}%;${WARN_PCT};${CRIT_PCT};0;100 'free space'=${FREE_MB}MB;;;0"
exit $EXIT
