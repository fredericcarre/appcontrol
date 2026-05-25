-- V057: Incidents ingested from external ITSM tools.
--
-- AppControl stores incidents pulled from ServiceNow, Jira Service
-- Management or any other ITSM to power the learning loop described in
-- Phase 5 of the methodology: each incident references the components it
-- impacted, lets us correlate co-occurrences, and ultimately drives the
-- pattern library.
--
-- This is not the FSM transition log (state_transitions) — it captures
-- the *organisational* signal (a ticket was opened, a root cause was
-- documented), not the technical state of a component at a point in time.

CREATE TABLE incidents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    application_id UUID REFERENCES applications(id) ON DELETE SET NULL,
    external_id VARCHAR(200) NOT NULL,
    source VARCHAR(50) NOT NULL,           -- 'servicenow', 'jira-sm', 'pagerduty', ...
    title TEXT NOT NULL,
    description TEXT,
    severity VARCHAR(20),                  -- 'P1', 'P2', 'P3', 'P4'
    status VARCHAR(50),                    -- 'open', 'in_progress', 'resolved', 'closed'
    opened_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    root_cause TEXT,
    impacted_components JSONB DEFAULT '[]', -- array of component IDs
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, external_id)
);

CREATE INDEX idx_incidents_org ON incidents (organization_id);
CREATE INDEX idx_incidents_app ON incidents (application_id);
CREATE INDEX idx_incidents_opened ON incidents (opened_at DESC);
CREATE INDEX idx_incidents_severity ON incidents (severity);
