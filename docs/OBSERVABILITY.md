---
title: Observability — monitor AppControl itself
description: Prometheus scrape config, Grafana dashboard, recommended alert rules, and log aggregation recipes for the AppControl backend, gateway, and agents.
---

# Observability — monitoring AppControl itself

AppControl is the platform you trust to operate your applications. So
you also need to know *its* health: backend latency, gateway
connectivity, agent heartbeats, FSM transitions per minute, audit log
backlog. This page covers everything an SRE needs to wire AppControl
into an existing Prometheus / Grafana / log stack.

A high-level recap: the backend exposes Prometheus metrics at
`/metrics`, structured JSON logs go to stdout, and every operational
event is also available via the WebSocket bus and webhooks.

---

## 1. Prometheus scrape

The backend, gateway and (optionally) agents expose `/metrics` in the
Prometheus exposition format. Drop this into your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'appcontrol-backend'
    metrics_path: /metrics
    scheme: https
    tls_config:
      ca_file: /etc/prometheus/appcontrol-ca.crt
    static_configs:
      - targets: ['backend-1.appcontrol.internal:8443',
                  'backend-2.appcontrol.internal:8443']

  - job_name: 'appcontrol-gateway'
    metrics_path: /metrics
    scheme: https
    tls_config:
      ca_file: /etc/prometheus/appcontrol-ca.crt
    static_configs:
      - targets: ['gateway-eu.appcontrol.internal:8443',
                  'gateway-us.appcontrol.internal:8443']
```

The endpoint is unauthenticated by default but bound only to the
internal admin interface. To require authentication, gate it via a
reverse proxy with mTLS or basic-auth and pass credentials in
`basic_auth` / `tls_config`.

A full reference of every emitted metric — name, labels, units and
business meaning — is in [reference/metrics.md](reference/metrics.md)
(auto-generated from the source on every build).

---

## 2. The 8 alerts you should have on day 1

Drop the following rules into a `appcontrol-alerts.yml` file and load
it from Prometheus. They cover the typical incident patterns: backend
overload, agent fleet drift, gateway flaps, audit-log pressure, and
DR-readiness regression.

```yaml
groups:
  - name: appcontrol
    interval: 30s
    rules:

      # ── Backend availability ──────────────────────────────────
      - alert: AppControlBackendDown
        expr: up{job="appcontrol-backend"} == 0
        for: 2m
        labels: { severity: critical }
        annotations:
          summary: AppControl backend {{ $labels.instance }} is down
          runbook: docs/RUNBOOKS.md#backend-cpu-100-percent

      - alert: AppControlBackendLatencyHigh
        expr: histogram_quantile(0.95,
                sum by (le) (rate(http_request_duration_seconds_bucket
                                  {job="appcontrol-backend"}[5m]))) > 1
        for: 10m
        labels: { severity: warning }
        annotations:
          summary: Backend p95 latency > 1s for 10m on {{ $labels.instance }}

      # ── Agent fleet health ────────────────────────────────────
      - alert: AppControlAgentFleetUnreachable
        expr: appcontrol_agents_unreachable_total
              / appcontrol_agents_total > 0.10
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: >
            More than 10% of agents are UNREACHABLE — check the gateway
            heartbeat path.

      - alert: AppControlAgentHeartbeatStale
        expr: time() - appcontrol_agent_last_heartbeat_timestamp > 600
        for: 0m
        labels: { severity: warning }
        annotations:
          summary: Agent {{ $labels.agent_id }} silent for > 10min

      # ── Operations volume ─────────────────────────────────────
      - alert: AppControlFSMTransitionsZero
        expr: rate(appcontrol_fsm_transitions_total[15m]) == 0
        for: 30m
        labels: { severity: warning }
        annotations:
          summary: >
            Zero FSM transitions for 30 minutes — backend may have lost
            connection to its agents, or no checks are running.

      - alert: AppControlOperationFailureRateHigh
        expr: rate(appcontrol_operations_total{status="failed"}[15m])
            / rate(appcontrol_operations_total[15m]) > 0.20
        for: 10m
        labels: { severity: critical }
        annotations:
          summary: >
            More than 20% of operations failing — investigate target
            agents and recent map changes.

      # ── Audit log pressure ────────────────────────────────────
      - alert: AppControlActionLogLagging
        expr: appcontrol_action_log_pending > 1000
        for: 5m
        labels: { severity: warning }
        annotations:
          summary: action_log INSERTs piling up — DB write path slow?

      # ── DORA readiness regression ─────────────────────────────
      - alert: AppControlDrillStale
        expr: time() - appcontrol_last_drill_timestamp > 86400 * 95
        for: 1h
        labels: { severity: warning }
        annotations:
          summary: >
            No rebuild drill in > 95 days for {{ $labels.application }} —
            DORA Art. 11 requires at-least-annual testing.
```

The names match what the backend emits (grep
`crates/backend/src/**/*.rs` for `metrics::counter!`,
`metrics::gauge!`, `metrics::histogram!`). If a label spelled in the
alert is missing on your instance, the auto-generated
[reference/metrics.md](reference/metrics.md) is the source of truth.

---

## 3. Building a Grafana dashboard

The panels worth having on the day-1 overview, with the PromQL
queries to drive them. Each one references metrics documented in the
auto-generated [reference/metrics.md](reference/metrics.md).

| Panel | Query (PromQL) |
|---|---|
| Backend RPS | `sum by (route) (rate(http_requests_total{job="appcontrol-backend"}[1m]))` |
| Backend p95 latency | `histogram_quantile(0.95, sum by (le,route) (rate(http_request_duration_seconds_bucket{job="appcontrol-backend"}[5m])))` |
| Backend error rate | `sum(rate(http_requests_total{job="appcontrol-backend",status=~"5.."}[5m])) / sum(rate(http_requests_total{job="appcontrol-backend"}[5m]))` |
| Agents — total / connected | `appcontrol_agents_total`, `appcontrol_agents_connected_total` |
| Heartbeat staleness | `time() - max by (agent_id) (appcontrol_agent_last_heartbeat_timestamp)` |
| Operations per minute | `sum by (op,status) (rate(appcontrol_operations_total[1m]))` |
| FSM transitions / min by from-state | `sum by (from_state) (rate(appcontrol_state_transitions_total[1m]))` |
| Audit log writes / s | `rate(appcontrol_action_log_writes_total[1m])` |
| Switchover phase counter | `appcontrol_switchover_phase_total` |
| Last drill, by application | `time() - appcontrol_last_drill_timestamp` |

Suggested layout: top row = backend health (RPS / p95 / errors),
middle row = fleet & operations (agents, ops/min, FSM transitions),
bottom row = compliance strip (audit throughput, last drill,
switchover phase). Save the dashboard JSON in your own
infrastructure repo — AppControl does not ship a canned one because
panel sets diverge from one organisation to the next.

When the backend gains a new metric family, the generated
[reference/metrics.md](reference/metrics.md) is updated automatically
on the next build, so the source of truth for query names stays in
sync with the code.

---

## 4. Logs — structured JSON to stdout

The backend, gateway and agents emit one JSON object per log line on
stdout, suitable for Fluent Bit / Vector / Filebeat collection:

```json
{
  "timestamp": "2026-05-25T10:14:00.123Z",
  "level": "INFO",
  "target": "appcontrol_backend::api::apps",
  "message": "Successfully started app",
  "app_id": "550e8400-e29b-41d4-a716-446655440000",
  "user_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "trace_id": "0af7651916cd43dd8448eb211c80319c"
}
```

Loki / Elasticsearch ingest works out of the box — every field is
queryable. Two index patterns to pre-build:

| Index | Use case |
|---|---|
| `appcontrol-backend-*` | API requests, FSM transitions, audit log |
| `appcontrol-agent-*` | Check execution, command dispatch, agent local state |

### Minimal Fluent Bit pipeline

```ini
[INPUT]
    Name          tail
    Path          /var/log/containers/appcontrol-*.log
    Parser        docker
    Tag           appcontrol.*

[FILTER]
    Name          parser
    Match         appcontrol.*
    Key_Name      log
    Parser        json
    Reserve_Data  on

[OUTPUT]
    Name          loki
    Match         appcontrol.*
    Host          loki.observability.svc.cluster.local
    Port          3100
    Labels        job=appcontrol, component=$kubernetes['labels']['app']
```

For Vector users, the same shape works with a `source = file` →
`transform = remap` → `sink = loki` pipeline. The point is: every
log line is already JSON, so no regex parsing is required.

---

## 5. WebSocket bus — push events without scraping

For real-time dashboards or alert routing, the WebSocket bus pushes
typed events to subscribed clients (state changes, command results,
switchover progress, permission changes). Connect with a JWT
authenticated as a service account; subscribe per application. See
`docs/USER_GUIDE.md` § *WebSocket protocol* for the message schema.

---

## 6. Webhooks — outbound notifications

For ITSM, ChatOps or paging tools, configure webhook endpoints (per
organisation or per application) on `/api/v1/webhooks`. Every state
change, switchover transition, operation completion and failure is
delivered with HMAC SHA-256 signing, circuit-breaker protection,
and exponential retry. See `crates/backend/src/core/notifications.rs`
for the wire format.

A typical wiring:

| Sink | Use |
|---|---|
| **PagerDuty** | Filter on `event_type=failure` and `severity=critical` |
| **Slack** | `event_type=state_change` → routed to the team's channel |
| **ServiceNow** | `event_type=switchover` → opens a change ticket |

---

## 7. Capacity baselines

The numbers below are from the criterion benchmarks in
[`docs/CAPACITY_PLANNING.md`](CAPACITY_PLANNING.md). Use them as
sanity checks when sizing your Prometheus retention and Grafana
panels.

| Object | Steady-state rate | Note |
|---|---|---|
| `appcontrol_check_events_total` | `agents × components × 2/min` | Every 30s on average |
| `appcontrol_state_transitions_total` | `≈ 1 / app / min` in healthy state | Spikes during start/stop |
| `appcontrol_action_log_writes_total` | `1 / user action` | Linear with operator traffic |
| `appcontrol_switchover_log_writes_total` | `≤ 1 / drill` | Annual on critical apps |
| `appcontrol_fsm_state_transitions_per_minute` | < 50 in normal ops | Above = investigate |

---

## 8. Where to go next

- [Hardening checklist](HARDENING.md) — security observability (auth
  failures, RBAC violations)
- [Disaster recovery](DISASTER_RECOVERY.md) — what each alert above
  means in terms of recovery scenario
- [Runbooks](RUNBOOKS.md) — concrete remediation steps for the
  alerts above
- [reference/metrics.md](reference/metrics.md) — authoritative list
  of every emitted metric
