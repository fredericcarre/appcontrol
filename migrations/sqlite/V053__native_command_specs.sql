-- V053 (SQLite): native command specs.
-- See migrations/V053__native_command_specs.sql for the rationale. SQLite has
-- no JSONB so we store the JSON payload as TEXT — DbJson + serde_json on the
-- backend handle (de)serialisation.

ALTER TABLE components ADD COLUMN check_native TEXT;
ALTER TABLE components ADD COLUMN start_native TEXT;
ALTER TABLE components ADD COLUMN stop_native  TEXT;
