# Check Script Examples

Reference check scripts illustrating each metrics extraction format that
the agent supports. Use them as starting templates when wiring AppControl
to your applications.

| File | Format | When to use |
|---|---|---|
| [`nagios_load_average.sh`](nagios_load_average.sh) | Nagios perfdata | Migrating existing Nagios/Icinga/Centreon plugins as-is |
| [`nagios_disk_usage.sh`](nagios_disk_usage.sh) | Nagios perfdata with quoted labels and UOM | Legacy plugin with spaces in metric names, units like `%` or `MB` |
| [`json_queue_depth.sh`](json_queue_depth.sh) | Pure JSON | New scripts (recommended) — clean, supports nesting |
| [`mixed_logs_python.py`](mixed_logs_python.py) | Auto-detect last JSON line | Scripts that also need verbose human-readable logs |

## What AppControl extracts

The agent parses stdout after each check and populates the
`check_events.metrics` JSONB column. Five formats are tried in order:

1. **Pure JSON** — entire stdout is a valid JSON object
2. **Tagged** — JSON inside `<appcontrol>...</appcontrol>`
3. **Marker** — JSON after a `---METRICS---` line
4. **Nagios perfdata** — `STATUS | label=value[UOM];warn;crit;min;max ...`
5. **Auto-detect** — last line of stdout that parses as a JSON object

Exit code drives the FSM (0=Running, 1=Degraded, ≥2=Failed). Metrics are
informational and feed dashboards, alert policies, and historical charts.

See [`docs/METRICS.md`](../../docs/METRICS.md) for the full reference.
