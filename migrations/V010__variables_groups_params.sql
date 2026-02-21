-- V010: Application Variables, Component Groups, Command Input Parameters, Display Enhancements
--
-- Features from old AppControl (LYNX-PRD) that were missing in v4:
-- 1. Application-level variables with $(var) interpolation in commands
-- 2. Component groups/categories for visual organization
-- 3. Custom command input parameters with regex validation
-- 4. Display enhancements: icons, display_name, hypertext links

-- ============================================================
-- 1. Application Variables
-- ============================================================
-- Variables are defined at the application level and interpolated
-- into component commands using $(variable_name) syntax.
-- Example: check_cmd = "curl http://$(APP_HOST):$(APP_PORT)/health"

CREATE TABLE app_variables (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    description TEXT,
    is_secret BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(application_id, name)
);

CREATE INDEX idx_app_variables_app ON app_variables (application_id);

-- ============================================================
-- 2. Component Groups
-- ============================================================
-- Groups allow visual organization of components on the map.
-- In the old version: "Bases de données", "Middlewares", "Fronts web", etc.

CREATE TABLE component_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    description TEXT,
    color VARCHAR(7) DEFAULT '#6366F1',  -- hex color for group border/background
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(application_id, name)
);

CREATE INDEX idx_component_groups_app ON component_groups (application_id);

-- Add group_id to components
ALTER TABLE components ADD COLUMN group_id UUID REFERENCES component_groups(id) ON DELETE SET NULL;

-- ============================================================
-- 3. Display Enhancements for Components
-- ============================================================
-- display_name: friendly label shown on the map (vs technical 'name')
-- icon: icon identifier (e.g., "database", "server", "globe", "shield", "cloud")
-- description: component description text

ALTER TABLE components ADD COLUMN display_name VARCHAR(200);
ALTER TABLE components ADD COLUMN icon VARCHAR(50) DEFAULT 'box';
ALTER TABLE components ADD COLUMN description TEXT;

-- ============================================================
-- 4. Hypertext Links (Resources)
-- ============================================================
-- Attach documentation, CMDB, monitoring URLs to components.
-- In the old version: "Page documentation", "url CMDB", "Log Splunk"

CREATE TABLE component_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    label VARCHAR(200) NOT NULL,
    url TEXT NOT NULL,
    link_type VARCHAR(50) NOT NULL DEFAULT 'documentation'
        CHECK (link_type IN ('documentation', 'cmdb', 'monitoring', 'log', 'runbook', 'other')),
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(component_id, label)
);

CREATE INDEX idx_component_links_component ON component_links (component_id);

-- ============================================================
-- 5. Command Input Parameters
-- ============================================================
-- Custom commands can define input parameters that the user must
-- fill in before execution. Each parameter has a name, description,
-- default value, and optional regex validation.
-- Example: "Purge logs" command with parameter "retention_days" (default: 30, regex: ^\d+$)

CREATE TABLE command_input_params (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    command_id UUID NOT NULL REFERENCES component_commands(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    default_value TEXT,
    validation_regex TEXT,
    required BOOLEAN NOT NULL DEFAULT true,
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(command_id, name)
);

CREATE INDEX idx_command_input_params_cmd ON command_input_params (command_id);
