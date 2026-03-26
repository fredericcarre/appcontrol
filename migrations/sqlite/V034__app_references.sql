-- V034: Track references between applications (SQLite)
-- When app A has a component that references app B, we record it here
-- This enables: deletion warnings, cascade state updates, dependency visualization

CREATE TABLE app_references (
    id TEXT PRIMARY KEY,
    -- The app that contains the referencing component
    source_app_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    -- The app being referenced (the "synthetic" component)
    target_app_id TEXT NOT NULL REFERENCES applications(id) ON DELETE RESTRICT,
    -- The component in source_app that holds the reference
    component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT REFERENCES users(id),

    -- Prevent duplicate references
    UNIQUE (source_app_id, target_app_id, component_id)
);

-- Index for finding all apps that reference a given app (for deletion check)
CREATE INDEX idx_app_references_target ON app_references(target_app_id);

-- Index for finding all references from a given app
CREATE INDEX idx_app_references_source ON app_references(source_app_id);

-- Add referenced_app_id to components for direct lookup
ALTER TABLE components ADD COLUMN referenced_app_id TEXT REFERENCES applications(id) ON DELETE SET NULL;

-- Index for finding components that reference apps
CREATE INDEX idx_components_referenced_app ON components(referenced_app_id);
