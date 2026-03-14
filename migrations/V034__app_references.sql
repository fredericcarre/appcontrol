-- Track references between applications
-- When app A has a component that references app B, we record it here
-- This enables: deletion warnings, cascade state updates, dependency visualization

CREATE TABLE app_references (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The app that contains the referencing component
    source_app_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    -- The app being referenced (the "synthetic" component)
    target_app_id UUID NOT NULL REFERENCES applications(id) ON DELETE RESTRICT,
    -- The component in source_app that holds the reference
    component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES users(id),

    -- Prevent duplicate references
    UNIQUE (source_app_id, target_app_id, component_id)
);

-- Index for finding all apps that reference a given app (for deletion check)
CREATE INDEX idx_app_references_target ON app_references(target_app_id);

-- Index for finding all references from a given app
CREATE INDEX idx_app_references_source ON app_references(source_app_id);

-- Add referenced_app_id to components for direct lookup
ALTER TABLE components ADD COLUMN referenced_app_id UUID REFERENCES applications(id) ON DELETE SET NULL;

-- Index for finding components that reference apps
CREATE INDEX idx_components_referenced_app ON components(referenced_app_id) WHERE referenced_app_id IS NOT NULL;

-- Comment
COMMENT ON TABLE app_references IS 'Tracks which applications reference other applications as synthetic components';
COMMENT ON COLUMN app_references.target_app_id IS 'ON DELETE RESTRICT: cannot delete an app that is referenced by another';
COMMENT ON COLUMN components.referenced_app_id IS 'If set, this component represents an aggregate view of another application';
