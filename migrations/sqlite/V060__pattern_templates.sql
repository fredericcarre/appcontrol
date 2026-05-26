-- V060: Pattern templates library (SQLite mirror).
-- See migrations/V060 for the full rationale.

CREATE TABLE pattern_templates (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    technology TEXT NOT NULL,
    description TEXT,
    check_cmd_template TEXT,
    integrity_check_cmd_template TEXT,
    infra_check_cmd_template TEXT,
    start_cmd_template TEXT,
    stop_cmd_template TEXT,
    rebuild_cmd_template TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    created_from_incident_id TEXT,
    is_enabled BOOLEAN NOT NULL DEFAULT 1,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (organization_id, name)
);

CREATE INDEX idx_pattern_templates_org ON pattern_templates (organization_id);
CREATE INDEX idx_pattern_templates_tech ON pattern_templates (technology);
CREATE INDEX idx_pattern_templates_incident ON pattern_templates (created_from_incident_id);
