-- V021: Enriched discovery — operational commands, config/log files, service matching
--
-- Adds operational columns to discovery_draft_components so that drafts carry
-- check/start/stop commands, detected config files, log files, and matched
-- system services all the way through to apply_draft → real components.

ALTER TABLE discovery_draft_components
    ADD COLUMN IF NOT EXISTS check_cmd TEXT,
    ADD COLUMN IF NOT EXISTS start_cmd TEXT,
    ADD COLUMN IF NOT EXISTS stop_cmd TEXT,
    ADD COLUMN IF NOT EXISTS restart_cmd TEXT,
    ADD COLUMN IF NOT EXISTS command_confidence VARCHAR(10) DEFAULT 'low',
    ADD COLUMN IF NOT EXISTS command_source VARCHAR(30),
    ADD COLUMN IF NOT EXISTS config_files JSONB DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS log_files JSONB DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS matched_service VARCHAR(200);
