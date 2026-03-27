-- V043: Component Log Sources for diagnostics and monitoring (SQLite)

-- Log sources table
CREATE TABLE component_log_sources (
    id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    organization_id TEXT NOT NULL REFERENCES organizations(id),

    name TEXT NOT NULL,
    source_type TEXT NOT NULL CHECK (source_type IN ('file', 'event_log', 'command')),

    file_path TEXT,

    event_log_name TEXT,
    event_log_source TEXT,
    event_log_level TEXT,

    command TEXT,
    command_timeout_seconds INTEGER DEFAULT 30,

    max_lines INTEGER DEFAULT 1000,
    max_age_hours INTEGER DEFAULT 24,
    is_sensitive INTEGER DEFAULT 0,

    display_order INTEGER DEFAULT 0,
    description TEXT,

    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_log_sources_component ON component_log_sources(component_id);
CREATE INDEX idx_log_sources_org ON component_log_sources(organization_id);

-- Add process capture settings to components
ALTER TABLE components ADD COLUMN log_capture_enabled INTEGER DEFAULT 1;
ALTER TABLE components ADD COLUMN log_buffer_lines INTEGER DEFAULT 10000;

-- Log access audit table
CREATE TABLE log_access_audit (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    user_id TEXT REFERENCES users(id),
    component_id TEXT REFERENCES components(id),
    log_source_id TEXT REFERENCES component_log_sources(id),

    source_type TEXT NOT NULL,
    source_name TEXT,

    lines_requested INTEGER,
    filter_applied TEXT,
    time_range_hours INTEGER,

    accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
    ip_address TEXT,
    user_agent TEXT
);

CREATE INDEX idx_log_audit_component ON log_access_audit(component_id);
CREATE INDEX idx_log_audit_user ON log_access_audit(user_id);
CREATE INDEX idx_log_audit_time ON log_access_audit(accessed_at);
