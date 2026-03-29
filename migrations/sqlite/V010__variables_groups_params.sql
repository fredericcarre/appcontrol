-- V010: Application Variables, Component Groups, Command Input Parameters (SQLite)

-- 1. Application Variables
CREATE TABLE app_variables (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    description TEXT,
    is_secret INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(application_id, name)
);

CREATE INDEX idx_app_variables_app ON app_variables (application_id);

-- 2. Component Groups
CREATE TABLE component_groups (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT DEFAULT '#6366F1',
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(application_id, name)
);

CREATE INDEX idx_component_groups_app ON component_groups (application_id);

-- Add group_id to components
ALTER TABLE components ADD COLUMN group_id TEXT REFERENCES component_groups(id);

-- 3. Display Enhancements for Components
ALTER TABLE components ADD COLUMN display_name TEXT;
ALTER TABLE components ADD COLUMN icon TEXT DEFAULT 'box';
ALTER TABLE components ADD COLUMN description TEXT;

-- 4. Hypertext Links (Resources)
CREATE TABLE component_links (
    id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    url TEXT NOT NULL,
    link_type TEXT NOT NULL DEFAULT 'documentation'
        CHECK (link_type IN ('documentation', 'cmdb', 'monitoring', 'log', 'runbook', 'other')),
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(component_id, label)
);

CREATE INDEX idx_component_links_component ON component_links (component_id);

-- 5. Command Input Parameters
CREATE TABLE command_input_params (
    id TEXT PRIMARY KEY,
    command_id TEXT NOT NULL REFERENCES component_commands(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    default_value TEXT,
    validation_regex TEXT,
    required INTEGER NOT NULL DEFAULT 1,
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(command_id, name)
);

CREATE INDEX idx_command_input_params_cmd ON command_input_params (command_id);
