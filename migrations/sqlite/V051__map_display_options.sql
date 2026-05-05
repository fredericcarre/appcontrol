-- V051 (SQLite): Per-map display options
--
-- See migrations/V051__map_display_options.sql for the rationale. SQLite has
-- no JSONB so we store the same payload as TEXT — DbJson + serde_json on the
-- backend handle (de)serialisation.

ALTER TABLE applications
    ADD COLUMN map_display_options TEXT NOT NULL DEFAULT '{}';
