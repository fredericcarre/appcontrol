-- V058: Add vendor channel adapters (SQLite mirror).
-- See migrations/V058__alert_vendor_channels.sql for the rationale.
--
-- SQLite cannot DROP or ALTER a CHECK constraint in place — the only
-- supported path is a table rebuild. To keep this migration cheap (the
-- table is small and freshly introduced in V057), we rebuild with the
-- widened CHECK list.

CREATE TABLE notification_channels_new (
    id              TEXT PRIMARY KEY,
    org_id          TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL
        CHECK (kind IN ('webhook', 'slack', 'email', 'pagerduty', 'teams')),
    config          TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (org_id, name)
);

INSERT INTO notification_channels_new
    SELECT id, org_id, name, kind, config, enabled, created_at, updated_at
      FROM notification_channels;

DROP TABLE notification_channels;
ALTER TABLE notification_channels_new RENAME TO notification_channels;

CREATE INDEX idx_notification_channels_org ON notification_channels (org_id) WHERE enabled = 1;
