-- V048: Component Catalog (SQLite version)
-- Dynamic component type definitions per organization.

CREATE TABLE IF NOT EXISTS component_catalog (
    id                  TEXT PRIMARY KEY,
    org_id              TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    type_key            TEXT NOT NULL,
    label               TEXT NOT NULL,
    description         TEXT,
    icon                TEXT NOT NULL DEFAULT 'box',
    color               TEXT NOT NULL DEFAULT '#455A64',
    category            TEXT,
    default_check_cmd   TEXT,
    default_start_cmd   TEXT,
    default_stop_cmd    TEXT,
    default_env_vars    TEXT,
    display_order       INTEGER NOT NULL DEFAULT 0,
    is_builtin          INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(org_id, type_key)
);

CREATE INDEX IF NOT EXISTS idx_component_catalog_org ON component_catalog(org_id);
CREATE INDEX IF NOT EXISTS idx_component_catalog_category ON component_catalog(org_id, category);
