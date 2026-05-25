-- V057: Alert policies, channels, and instances (SQLite mirror).
-- See migrations/V057__alert_channels_and_policies.sql for the full rationale.
--
-- SQLite differences from the PostgreSQL version:
--   * UUIDs stored as TEXT (DbUuid handles binding/decoding)
--   * JSONB → TEXT (parsed application-side; storage uses JSON1 functions
--     where needed via json_extract())
--   * trigger_states TEXT[] → TEXT (JSON array stored verbatim,
--     application splits it)
--   * channel_ids UUID[] → TEXT (JSON array of UUID strings)
--   * TIMESTAMPTZ → TEXT (RFC3339 strings)
--   * gen_random_uuid() → app-side; the column has no DEFAULT and the
--     repository inserts a freshly generated UUID.

CREATE TABLE notification_channels (
    id              TEXT PRIMARY KEY,
    org_id          TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL
        CHECK (kind IN ('webhook', 'slack')),
    config          TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (org_id, name)
);

CREATE INDEX idx_notification_channels_org ON notification_channels (org_id) WHERE enabled = 1;

CREATE TABLE alert_policies (
    id                  TEXT PRIMARY KEY,
    org_id              TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    description         TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1,
    selector            TEXT NOT NULL DEFAULT '{}',
    trigger_states      TEXT NOT NULL,
    sustain_seconds     INTEGER NOT NULL DEFAULT 0
        CHECK (sustain_seconds >= 0),
    severity            TEXT NOT NULL DEFAULT 'warning'
        CHECK (severity IN ('info', 'warning', 'critical')),
    cooldown_seconds    INTEGER NOT NULL DEFAULT 300
        CHECK (cooldown_seconds >= 0),
    channel_ids         TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (org_id, name)
);

CREATE INDEX idx_alert_policies_org_enabled ON alert_policies (org_id, enabled);

CREATE TABLE alert_instances (
    id                  TEXT PRIMARY KEY,
    org_id              TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    policy_id           TEXT NOT NULL REFERENCES alert_policies(id) ON DELETE CASCADE,
    component_id        TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    fingerprint         TEXT NOT NULL,
    severity            TEXT NOT NULL
        CHECK (severity IN ('info', 'warning', 'critical')),
    status              TEXT NOT NULL DEFAULT 'firing'
        CHECK (status IN ('firing', 'acknowledged', 'resolved')),
    triggered_state     TEXT NOT NULL,
    summary             TEXT,
    fired_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    acknowledged_at     TEXT,
    acknowledged_by     TEXT REFERENCES users(id),
    resolved_at         TEXT,
    notifications_sent  TEXT NOT NULL DEFAULT '[]'
);

CREATE UNIQUE INDEX idx_alert_instances_open
    ON alert_instances (fingerprint)
    WHERE status IN ('firing', 'acknowledged');

CREATE INDEX idx_alert_instances_org_status_fired
    ON alert_instances (org_id, status, fired_at DESC);

CREATE INDEX idx_alert_instances_component
    ON alert_instances (component_id, fired_at DESC);
