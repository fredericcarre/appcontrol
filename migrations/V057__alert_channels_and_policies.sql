-- V057: Alert policies, channels, and instances.
--
-- Builds an alerting layer on top of the existing webhook dispatch
-- (core::notifications). Where webhooks fire on every state change,
-- policies add:
--   * Selectors  — which components / apps the policy applies to
--   * Triggers   — which states + sustain duration before firing
--   * Severities — info / warning / critical
--   * Cooldown   — anti-flap: don't fire the same (policy, component) again
--                  within this window
--   * Lifecycle  — explicit firing → acknowledged → resolved instances
--                  so operators can see what's currently broken and ack/close
--
-- Channels carry the dispatch target. Webhooks remain available for raw
-- event streams; channels are for human-facing destinations (Slack today,
-- Email/PagerDuty/Teams/Opsgenie in follow-up sprints).

-- Notification destinations. `config` is vendor-specific JSON; the kind
-- column tells the backend which adapter to use.
CREATE TABLE notification_channels (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    kind            VARCHAR(32) NOT NULL
        CHECK (kind IN ('webhook', 'slack')),
    -- Vendor-specific config. For 'webhook': { "url": "...", "secret": "...", "headers": {} }.
    -- For 'slack': { "webhook_url": "https://hooks.slack.com/..." }.
    -- Secrets are NEVER returned by the API in responses (redacted server-side).
    config          JSONB NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, name)
);

CREATE INDEX idx_notification_channels_org ON notification_channels (org_id) WHERE enabled = TRUE;

-- Alert policies: declarative rules that watch state transitions and
-- decide when to open / close alert instances.
CREATE TABLE alert_policies (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name                VARCHAR(255) NOT NULL,
    description         TEXT,
    enabled             BOOLEAN NOT NULL DEFAULT TRUE,
    -- Selector: which components / apps this policy applies to.
    -- Shape: { "app_id": "uuid"? , "component_id": "uuid"?, "tags": {"k":"v"}? }
    -- Empty selector ({}) = applies to every component in the org.
    selector            JSONB NOT NULL DEFAULT '{}'::JSONB,
    -- ComponentState names that trigger this policy. Common: ['FAILED','UNREACHABLE'].
    trigger_states      TEXT[] NOT NULL,
    -- Component must hold a trigger_state for this long before the
    -- policy actually opens an instance. 0 = fire immediately.
    -- Implementation note: sustain enforcement uses
    -- `state_transitions` history at policy-eval time; no scheduler needed.
    sustain_seconds     INTEGER NOT NULL DEFAULT 0
        CHECK (sustain_seconds >= 0),
    severity            VARCHAR(16) NOT NULL DEFAULT 'warning'
        CHECK (severity IN ('info', 'warning', 'critical')),
    -- Anti-flap: after firing, the same (policy, component) fingerprint
    -- is suppressed for cooldown_seconds.
    cooldown_seconds    INTEGER NOT NULL DEFAULT 300
        CHECK (cooldown_seconds >= 0),
    -- Channels this policy dispatches to. UUIDs reference notification_channels.
    -- Empty array = policy still opens instances but sends no notifications.
    channel_ids         UUID[] NOT NULL DEFAULT '{}',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, name)
);

CREATE INDEX idx_alert_policies_org_enabled ON alert_policies (org_id, enabled);

-- Alert instances: one row per (policy, component) that has fired.
-- Lifecycle: firing → optionally acknowledged → eventually resolved.
-- Status updates are intentional (not append-only); the action_log
-- captures who did what.
CREATE TABLE alert_instances (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    policy_id           UUID NOT NULL REFERENCES alert_policies(id) ON DELETE CASCADE,
    component_id        UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    -- Stable identifier for dedup: SHA-256 of (policy_id, component_id).
    -- Combined with status='firing' in a unique index → at most one open
    -- alert per (policy, component) at a time.
    fingerprint         VARCHAR(64) NOT NULL,
    severity            VARCHAR(16) NOT NULL
        CHECK (severity IN ('info', 'warning', 'critical')),
    status              VARCHAR(16) NOT NULL DEFAULT 'firing'
        CHECK (status IN ('firing', 'acknowledged', 'resolved')),
    -- State that tripped the alert (e.g. 'FAILED').
    triggered_state     TEXT NOT NULL,
    -- Optional human-readable summary computed at fire time.
    summary             TEXT,
    fired_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged_at     TIMESTAMPTZ,
    acknowledged_by     UUID REFERENCES users(id),
    resolved_at         TIMESTAMPTZ,
    -- Per-channel dispatch log: [{"channel_id":"...","at":"...","ok":true,"error":null}].
    notifications_sent  JSONB NOT NULL DEFAULT '[]'::JSONB
);

-- Only one firing instance per (policy, component) at a time.
CREATE UNIQUE INDEX idx_alert_instances_open
    ON alert_instances (fingerprint)
    WHERE status IN ('firing', 'acknowledged');

CREATE INDEX idx_alert_instances_org_status_fired
    ON alert_instances (org_id, status, fired_at DESC);

CREATE INDEX idx_alert_instances_component
    ON alert_instances (component_id, fired_at DESC);
