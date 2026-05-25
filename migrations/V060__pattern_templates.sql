-- V060: Pattern templates library — transversal capitalisation.
--
-- Phase 5 of the methodology ("apprentissage par les incidents") and
-- the vision document promise a shared library of patterns that
-- accumulates over time: each post-incident PR can tag the lessons it
-- captures, and other applications running the same technology can
-- pick them up via a propagation flow.
--
-- A pattern is a reusable bundle that describes "how this technology
-- typically misbehaves and what checks/commands cover it":
--
--   * `technology`  — coarse tag (`spring-boot`, `postgres`, `kafka`...)
--   * `check_cmd_template` and friends — command snippets parametrised
--     with `{hostname}`, `{port}`, `{install_path}` placeholders that
--     map onto the same templater the agent uses for cluster members.
--   * `created_from_incident_id` — optional back-reference to the
--     incident that motivated the pattern (Phase 5 loop visibility).
--
-- The library is scoped to the organisation so customers can curate
-- their own patterns without polluting the global catalogue.

CREATE TABLE pattern_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    technology VARCHAR(100) NOT NULL,            -- spring-boot, postgres, kafka, ...
    description TEXT,
    check_cmd_template TEXT,
    integrity_check_cmd_template TEXT,
    infra_check_cmd_template TEXT,
    start_cmd_template TEXT,
    stop_cmd_template TEXT,
    rebuild_cmd_template TEXT,
    tags JSONB NOT NULL DEFAULT '[]',
    created_from_incident_id UUID REFERENCES incidents(id) ON DELETE SET NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    usage_count INTEGER NOT NULL DEFAULT 0,      -- bumped when a component picks the pattern up
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, name)
);

CREATE INDEX idx_pattern_templates_org ON pattern_templates (organization_id);
CREATE INDEX idx_pattern_templates_tech ON pattern_templates (technology);
CREATE INDEX idx_pattern_templates_incident ON pattern_templates (created_from_incident_id);
