-- V055: Per-member native command overrides (SQLite mirror).
-- See migrations/V055 for the full rationale.

ALTER TABLE cluster_members ADD COLUMN check_native_override TEXT;
ALTER TABLE cluster_members ADD COLUMN start_native_override TEXT;
ALTER TABLE cluster_members ADD COLUMN stop_native_override TEXT;
