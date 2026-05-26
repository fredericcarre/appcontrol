-- V057: Incidents ingested from external ITSM tools (SQLite mirror).
-- See migrations/V057 for the full rationale.

CREATE TABLE incidents (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    application_id TEXT,
    external_id TEXT NOT NULL,
    source TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    severity TEXT,
    status TEXT,
    opened_at TIMESTAMP NOT NULL,
    resolved_at TIMESTAMP,
    root_cause TEXT,
    impacted_components TEXT NOT NULL DEFAULT '[]',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (source, external_id)
);

CREATE INDEX idx_incidents_org ON incidents (organization_id);
CREATE INDEX idx_incidents_app ON incidents (application_id);
CREATE INDEX idx_incidents_opened ON incidents (opened_at DESC);
CREATE INDEX idx_incidents_severity ON incidents (severity);
