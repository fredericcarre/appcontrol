#!/bin/bash
# Modern AppControl check emitting a pure JSON metrics object.
#
# Recommended format for new scripts: cleaner, supports nesting, and avoids
# the legacy Nagios perfdata edge cases (UOM parsing, quoted labels, ...).
#
# Usage: json_queue_depth.sh <queue_name>

set -e

QUEUE="${1:?Usage: json_queue_depth.sh <queue_name>}"
CRIT_DEPTH="${CRIT_DEPTH:-10000}"
CRIT_LAG_S="${CRIT_LAG_S:-300}"

# Replace with your real probe (rabbitmqctl, kafkacat, redis-cli, ...).
DEPTH=$((RANDOM % 12000))
CONSUMERS=$((1 + RANDOM % 5))
LAG_S=$((RANDOM % 600))

EXIT=0
if [ "$DEPTH" -gt "$CRIT_DEPTH" ] || [ "$LAG_S" -gt "$CRIT_LAG_S" ]; then
    EXIT=2
fi

cat <<JSON
{
  "queue": "${QUEUE}",
  "depth": ${DEPTH},
  "consumers": ${CONSUMERS},
  "lag_s": ${LAG_S}
}
JSON

exit $EXIT
