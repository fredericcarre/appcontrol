-- V004: Components, Dependencies, Site Overrides, Component Commands (SQLite)

CREATE TABLE components (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    component_type TEXT NOT NULL
        CHECK (component_type IN ('database','middleware','appserver','webfront','service','batch','custom','application')),
    agent_id TEXT REFERENCES agents(id),
    -- Core commands
    check_cmd TEXT,
    start_cmd TEXT,
    stop_cmd TEXT,
    -- Advanced checks (v4)
    integrity_check_cmd TEXT,
    post_start_check_cmd TEXT,
    -- Infrastructure check (v4.2)
    infra_check_cmd TEXT,
    -- Rebuild commands (v4.2)
    rebuild_cmd TEXT,
    rebuild_infra_cmd TEXT,
    rebuild_agent_id TEXT REFERENCES agents(id),
    rebuild_protected INTEGER NOT NULL DEFAULT 0,
    -- Configuration
    check_interval_seconds INTEGER NOT NULL DEFAULT 30,
    start_timeout_seconds INTEGER NOT NULL DEFAULT 120,
    stop_timeout_seconds INTEGER NOT NULL DEFAULT 60,
    is_optional INTEGER NOT NULL DEFAULT 0,
    -- Visual position (React Flow)
    position_x REAL DEFAULT 0,
    position_y REAL DEFAULT 0,
    -- Metadata
    env_vars TEXT DEFAULT '{}',
    tags TEXT DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(application_id, name)
);

CREATE TABLE dependencies (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    from_component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    to_component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(from_component_id, to_component_id)
);

CREATE TABLE site_overrides (
    id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    site_id TEXT NOT NULL REFERENCES sites(id),
    agent_id_override TEXT REFERENCES agents(id),
    check_cmd_override TEXT,
    start_cmd_override TEXT,
    stop_cmd_override TEXT,
    rebuild_cmd_override TEXT,
    env_vars_override TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(component_id, site_id)
);

CREATE TABLE component_commands (
    id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    description TEXT,
    requires_confirmation INTEGER NOT NULL DEFAULT 0,
    min_permission_level TEXT NOT NULL DEFAULT 'operate'
        CHECK (min_permission_level IN ('operate','edit','manage','owner')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(component_id, name)
);

CREATE INDEX idx_components_app ON components (application_id);
CREATE INDEX idx_components_agent ON components (agent_id);
CREATE INDEX idx_dependencies_app ON dependencies (application_id);
CREATE INDEX idx_dependencies_from ON dependencies (from_component_id);
CREATE INDEX idx_dependencies_to ON dependencies (to_component_id);
CREATE INDEX idx_site_overrides_component ON site_overrides (component_id);
CREATE INDEX idx_component_commands_component ON component_commands (component_id);
