-- V062: Map annotations + knowledge progress tracking.
--
-- Two related but distinct concepts surfaced in the methodology:
--
--   * **Annotations** — free-form human commentary attached to a
--     component, a dependency or an application. Used during the
--     human review phase (§4.4), incident post-mortems, and
--     architecture discussions. NOT operational (does not drive any
--     FSM) — purely informational.
--
--   * **Knowledge status** — how validated each component / dependency
--     is. The captation pipeline produces *candidate* rows (best-effort
--     from sources); the human review promotes them through *draft*
--     and *reviewed* and eventually *validated*. A confidence score
--     (0..1) is carried alongside for downstream sorting and risk
--     weighting.

CREATE TABLE map_annotations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    target_type VARCHAR(20) NOT NULL
        CHECK (target_type IN ('application', 'component', 'dependency')),
    target_id UUID NOT NULL,
    kind VARCHAR(20) NOT NULL DEFAULT 'note'
        CHECK (kind IN ('note', 'review', 'todo', 'warning')),
    body TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    author_id UUID REFERENCES users(id) ON DELETE SET NULL,
    resolved_at TIMESTAMPTZ,
    resolved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_map_annotations_target ON map_annotations (target_type, target_id);
CREATE INDEX idx_map_annotations_org ON map_annotations (organization_id);
CREATE INDEX idx_map_annotations_open ON map_annotations (target_type, target_id) WHERE resolved_at IS NULL;

-- Knowledge progress columns on components and dependencies.
ALTER TABLE components
    ADD COLUMN confidence_score REAL NOT NULL DEFAULT 0.5,
    ADD COLUMN knowledge_status VARCHAR(20) NOT NULL DEFAULT 'draft'
        CHECK (knowledge_status IN ('candidate', 'draft', 'reviewed', 'validated', 'deprecated'));

ALTER TABLE dependencies
    ADD COLUMN confidence_score REAL NOT NULL DEFAULT 0.5,
    ADD COLUMN knowledge_status VARCHAR(20) NOT NULL DEFAULT 'draft'
        CHECK (knowledge_status IN ('candidate', 'draft', 'reviewed', 'validated', 'deprecated'));

CREATE INDEX idx_components_knowledge_status ON components (knowledge_status);
CREATE INDEX idx_dependencies_knowledge_status ON dependencies (knowledge_status);
