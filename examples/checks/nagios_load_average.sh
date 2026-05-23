#!/bin/bash
# Nagios-style check emitting perfdata for system load average.
#
# Demonstrates the legacy Nagios plugin output convention that AppControl
# parses transparently. Drop this script in unchanged from your existing
# Nagios/Icinga/Centreon installation — AppControl will extract the
# `load1`, `load5`, `load15` metrics and store them alongside the exit code.
#
# Output: "<STATUS> | label=value[UOM];warn;crit;min;max ..."

set -e

WARN="${WARN:-1.0}"
CRIT="${CRIT:-2.0}"

read -r LOAD1 LOAD5 LOAD15 _ < /proc/loadavg

STATUS="OK"
EXIT=0
if awk "BEGIN { exit !($LOAD1 >= $CRIT) }"; then
    STATUS="CRITICAL"
    EXIT=2
elif awk "BEGIN { exit !($LOAD1 >= $WARN) }"; then
    STATUS="WARNING"
    EXIT=1
fi

echo "${STATUS} - load average: ${LOAD1}, ${LOAD5}, ${LOAD15} | load1=${LOAD1};${WARN};${CRIT};0; load5=${LOAD5};${WARN};${CRIT};0; load15=${LOAD15};${WARN};${CRIT};0;"
exit $EXIT
