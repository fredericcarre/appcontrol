-- V051: Per-map display options
--
-- Lets operators tailor what shows on each component node — host name,
-- metrics widget, site bindings, weather, cluster badge, links — on a
-- per-application basis. Stored as a JSONB blob so we can grow the option
-- catalogue without further migrations.
--
-- The default `{}` means "show everything" — the frontend treats absent
-- keys as enabled, so existing apps keep their current visual fidelity.
-- Operators flip flags off to declutter big maps.

ALTER TABLE applications
    ADD COLUMN IF NOT EXISTS map_display_options JSONB NOT NULL DEFAULT '{}'::jsonb;
