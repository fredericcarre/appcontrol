-- Component Catalog: dynamic component type definitions per organization.
-- Allows clients to populate AppControl with their own component types,
-- each with icon, color, category, and optional default commands.

CREATE TABLE component_catalog (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    type_key            VARCHAR(50) NOT NULL,
    label               VARCHAR(200) NOT NULL,
    description         TEXT,
    icon                VARCHAR(50) NOT NULL DEFAULT 'box',
    color               VARCHAR(7) NOT NULL DEFAULT '#455A64',
    category            VARCHAR(50),
    default_check_cmd   TEXT,
    default_start_cmd   TEXT,
    default_stop_cmd    TEXT,
    default_env_vars    JSONB,
    display_order       INTEGER NOT NULL DEFAULT 0,
    is_builtin          BOOLEAN NOT NULL DEFAULT false,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(org_id, type_key)
);

CREATE INDEX idx_component_catalog_org ON component_catalog(org_id);
CREATE INDEX idx_component_catalog_category ON component_catalog(org_id, category);
