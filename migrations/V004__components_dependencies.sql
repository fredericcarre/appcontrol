-- V004: Components, Dependencies, Site Overrides, Component Commands

CREATE TABLE components (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    component_type VARCHAR(50) NOT NULL
        CHECK (component_type IN ('database','middleware','appserver','webfront','service','batch','custom')),
    agent_id UUID REFERENCES agents(id),
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
    rebuild_agent_id UUID REFERENCES agents(id),
    rebuild_protected BOOLEAN NOT NULL DEFAULT false,
    -- Configuration
    check_interval_seconds INTEGER NOT NULL DEFAULT 30,
    start_timeout_seconds INTEGER NOT NULL DEFAULT 120,
    stop_timeout_seconds INTEGER NOT NULL DEFAULT 60,
    is_optional BOOLEAN NOT NULL DEFAULT false,
    -- Visual position (React Flow)
    position_x REAL DEFAULT 0,
    position_y REAL DEFAULT 0,
    -- Metadata
    env_vars JSONB DEFAULT '{}',
    tags JSONB DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(application_id, name)
);

CREATE TABLE dependencies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    from_component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    to_component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(from_component_id, to_component_id)
);

CREATE TABLE site_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES sites(id),
    agent_id_override UUID REFERENCES agents(id),
    check_cmd_override TEXT,
    start_cmd_override TEXT,
    stop_cmd_override TEXT,
    rebuild_cmd_override TEXT,
    env_vars_override JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(component_id, site_id)
);

CREATE TABLE component_commands (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    command TEXT NOT NULL,
    description TEXT,
    requires_confirmation BOOLEAN NOT NULL DEFAULT false,
    min_permission_level VARCHAR(20) NOT NULL DEFAULT 'operate'
        CHECK (min_permission_level IN ('operate','edit','manage','owner')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(component_id, name)
);

CREATE INDEX idx_components_app ON components (application_id);
CREATE INDEX idx_components_agent ON components (agent_id);
CREATE INDEX idx_dependencies_app ON dependencies (application_id);
CREATE INDEX idx_dependencies_from ON dependencies (from_component_id);
CREATE INDEX idx_dependencies_to ON dependencies (to_component_id);
CREATE INDEX idx_site_overrides_component ON site_overrides (component_id);
CREATE INDEX idx_component_commands_component ON component_commands (component_id);
