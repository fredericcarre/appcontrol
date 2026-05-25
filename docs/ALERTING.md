# Alerting

AppControl ships a built-in **policy-driven alerting engine** on top of
the FSM. Every state transition (component going from RUNNING → FAILED,
DEGRADED, UNREACHABLE, etc.) is evaluated against your alert policies; a
match opens an `alert_instance` with full lifecycle (firing →
acknowledged → resolved) and dispatches notifications to one or more
channels.

This rounds out the existing webhook firehose (see
[Integration cookbook](INTEGRATION_COOKBOOK.md)): use raw webhooks when
you want every event, use policies when you want human-facing alerts
with severity, sustain, anti-flap, and acknowledge / resolve tracking.

## Concepts

```
                 ┌──────────────────────────┐
   FSM ──state──▶│ alerting engine          │
   transition    │  • match selector        │
                 │  • check cooldown        │
                 │  • check sustain         │
                 │  • open alert_instance   │
                 │  • dispatch              │
                 └────────────┬─────────────┘
                              │
                ┌─────────────┴───────────┐
                ▼                         ▼
         notification_channels      alert_instances
              (Slack,                (firing →
               webhook, ...)          acknowledged →
                                      resolved)
```

* **Notification channel** — a destination. Today: generic webhook and
  Slack incoming webhook. Email, PagerDuty, Microsoft Teams, Opsgenie
  land in later sprints.
* **Alert policy** — declarative rule: *when components matching this
  selector enter one of these states for at least this long, dispatch to
  these channels with this severity*.
* **Alert instance** — one row per (policy, component) that has fired.
  Carries the lifecycle: `firing` → optionally `acknowledged` (by a user)
  → `resolved` (either by an operator or automatically when the
  component leaves the trigger state).

## Permissions

| Action | Required role |
|---|---|
| List channels / policies / alerts | any authenticated user in the org |
| Create / delete channels | org admin |
| Create / delete policies | org admin |
| Acknowledge / resolve alert | any authenticated user in the org |

Component-scoped filtering of the alerts feed lands in a follow-up.

## Notification channels

### Generic webhook

```bash
curl -X POST $APPCONTROL/api/v1/alert-channels \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "name": "ops-bridge",
       "enabled": true,
       "config": {
         "kind": "webhook",
         "url": "https://ops.example.com/hooks/appcontrol",
         "secret": "shared-with-receiver",
         "headers": { "X-Source": "appcontrol" }
       }
     }'
```

Each dispatch is an `POST` with `Content-Type: application/json`. If
`secret` is set, AppControl computes an HMAC-SHA256 over the raw body
and sends it as `X-AppControl-Signature: sha256=<hex>` so receivers can
authenticate the request. Custom headers from `config.headers` are
applied verbatim.

Payload shape (canonical, also used internally by all channels):

```json
{
  "alert_id":        "uuid",
  "policy_id":       "uuid",
  "policy_name":     "Production DB down",
  "component_id":    "uuid",
  "component_name":  "Oracle-DB",
  "app_id":          "uuid",
  "app_name":        "Payments",
  "severity":        "critical",
  "status":          "firing",
  "triggered_state": "FAILED",
  "fired_at":        "2026-05-25T13:42:01Z",
  "summary":         "Oracle-DB transitioned to FAILED (policy 'Production DB down')"
}
```

### Slack incoming webhook

```bash
curl -X POST $APPCONTROL/api/v1/alert-channels \
     -d '{
       "name": "slack-prod-alerts",
       "config": {
         "kind": "slack",
         "webhook_url": "<your Slack incoming-webhook URL>"
       }
     }'
```

AppControl renders the payload as Slack message blocks with a colour
strip (green = info, yellow = warning, red = critical) and a status
emoji (🚨 firing, 👀 acknowledged, ✅ resolved). The Slack webhook URL
embeds a credential; AppControl masks it in API responses and audit
logs.

## Alert policies

```bash
curl -X POST $APPCONTROL/api/v1/alert-policies \
     -d '{
       "name": "Production database down",
       "description": "Any DB-tier component going FAILED in prod",
       "enabled": true,
       "selector": { "tags": { "env": "prod", "tier": "database" } },
       "trigger_states": ["FAILED", "UNREACHABLE"],
       "sustain_seconds": 60,
       "severity": "critical",
       "cooldown_seconds": 600,
       "channel_ids": ["uuid-of-slack-channel", "uuid-of-pager-webhook"]
     }'
```

### Selector

Components match when every field in the selector matches. Empty fields
are wildcards.

| Field | Behaviour |
|---|---|
| `app_id` (UUID) | only components of this application |
| `component_id` (UUID) | exactly this component |
| `tags` (object) | the component's `tags` JSONB must contain all listed key/value pairs |

`{}` (empty) matches every component in the org. Use sparingly.

### Trigger states

Any of `UNKNOWN`, `RUNNING`, `DEGRADED`, `FAILED`, `STOPPED`,
`STARTING`, `STOPPING`, `UNREACHABLE`. Typical production set:
`["FAILED", "UNREACHABLE"]`.

### Sustain

`sustain_seconds > 0` means the component must have been in any of the
`trigger_states` for at least that long before the policy fires. This
prevents alert storms from transient flips (e.g. a check that briefly
exits 2 between two 0s). Implementation looks at
`state_transitions` history — no scheduler / cron needed.

### Cooldown

`cooldown_seconds` suppresses re-firing the same `(policy, component)`
fingerprint within that window, regardless of how many transitions
happen. Defaults to 5 minutes. Set to 0 to disable.

### Severity

`info` | `warning` | `critical`. Drives Slack colour, can be referenced
by receivers to gate paging / escalation.

## Alert lifecycle

```
              FSM enters trigger state
                       │
                       ▼
                   ╔═══════╗
                   ║firing ║───── operator acks ─────┐
                   ╚═══╤═══╝                         │
                       │                             ▼
       FSM leaves trigger state                ╔═══════════╗
                       │                       ║acknowledged║
                       │                       ╚═════╤══════╝
                       ▼                             │
                  ╔════════╗◀── FSM leaves trigger ──┘
                  ║resolved║   OR operator resolves
                  ╚════════╝
```

The engine auto-resolves an alert when the component transitions out of
all of the policy's `trigger_states`. Operators can also acknowledge
(I'm looking at it) or resolve (it's handled) manually:

```bash
curl -X POST $APPCONTROL/api/v1/alerts/$ID/acknowledge
curl -X POST $APPCONTROL/api/v1/alerts/$ID/resolve
```

Only one open instance can exist per `(policy, component)` fingerprint
at any time — enforced by a partial unique index in PostgreSQL — so
re-evaluations during a long outage do not duplicate.

## API reference

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/v1/alert-channels` | List channels (secrets redacted) |
| POST | `/api/v1/alert-channels` | Create a channel (admin) |
| DELETE | `/api/v1/alert-channels/:id` | Delete a channel (admin) |
| GET | `/api/v1/alert-policies` | List policies |
| POST | `/api/v1/alert-policies` | Create a policy (admin) |
| DELETE | `/api/v1/alert-policies/:id` | Delete a policy (admin) |
| GET | `/api/v1/alerts` | Last 500 alert instances |
| POST | `/api/v1/alerts/:id/acknowledge` | Move firing → acknowledged |
| POST | `/api/v1/alerts/:id/resolve` | Move firing/acknowledged → resolved |

All write operations are logged to `action_log` with the caller's
`user_id`.

## Operational notes

* **Backend:** PostgreSQL only for this MVP. The SQLite schema mirror
  exists (`migrations/sqlite/V057__alert_channels_and_policies.sql`);
  the engine port is the next sprint.
* **Failure isolation:** the engine runs on its own tokio task per
  transition. A misbehaving channel never blocks the FSM hot path. Per-
  channel failures land in `alert_instances.notifications_sent` and are
  visible in the alerts feed.
* **Idempotence:** the partial unique index on `(fingerprint)` for open
  instances means concurrent dispatches are safe — the second insert
  becomes a no-op via `ON CONFLICT DO NOTHING`.
* **Secrets:** channel `config` is stored verbatim in PostgreSQL; API
  responses redact `secret` and Slack webhook tokens. Use database
  access (or a future "show secret" admin endpoint) to recover raw
  values.

## Roadmap (deferred)

The MVP focuses on the engine and the two most common destinations.
Planned follow-ups:

1. **Vendor adapters:** Email (SMTP), PagerDuty Events API, Microsoft
   Teams adaptive cards, Opsgenie alerts API.
2. **Frontend:** `/alerts` page (firing inbox + history),
   `/settings/alert-policies` editor with selector preview & dry-run
   simulator, badges on the map for components with active alerts.
3. **On-call schedules:** simple rotations (day/night, week-on-week-off)
   and escalation chains — currently delegate to PagerDuty / Opsgenie.
4. **SQLite parity:** port the engine queries.
5. **Templates:** per-policy notification message templates so teams can
   inject runbook URLs / dashboard links into the payload.
