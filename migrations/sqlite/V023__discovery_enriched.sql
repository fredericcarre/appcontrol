-- V023: Enriched discovery — operational commands, config/log files, service matching (SQLite)
--
-- Adds operational columns to discovery_draft_components so that drafts carry
-- check/start/stop commands, detected config files, log files, and matched
-- system services all the way through to apply_draft → real components.

ALTER TABLE discovery_draft_components ADD COLUMN check_cmd TEXT;
ALTER TABLE discovery_draft_components ADD COLUMN start_cmd TEXT;
ALTER TABLE discovery_draft_components ADD COLUMN stop_cmd TEXT;
ALTER TABLE discovery_draft_components ADD COLUMN restart_cmd TEXT;
ALTER TABLE discovery_draft_components ADD COLUMN command_confidence TEXT DEFAULT 'low';
ALTER TABLE discovery_draft_components ADD COLUMN command_source TEXT;
ALTER TABLE discovery_draft_components ADD COLUMN config_files TEXT DEFAULT '[]';
ALTER TABLE discovery_draft_components ADD COLUMN log_files TEXT DEFAULT '[]';
ALTER TABLE discovery_draft_components ADD COLUMN matched_service TEXT;
