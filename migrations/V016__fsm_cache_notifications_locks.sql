-- V016: FSM state cache, webhook notification endpoints, distributed lock support

-- Add current_state column to components for fast FSM reads
-- (replaces ORDER BY created_at DESC LIMIT 1 on state_transitions)
ALTER TABLE components ADD COLUMN current_state VARCHAR(20) NOT NULL DEFAULT 'UNKNOWN';
CREATE INDEX idx_components_state ON components (current_state);

-- Backfill current_state from state_transitions for existing data
UPDATE components c
SET current_state = COALESCE(
    (SELECT st.to_state FROM state_transitions st WHERE st.component_id = c.id ORDER BY st.created_at DESC LIMIT 1),
    'UNKNOWN'
);

-- Webhook endpoints for notification delivery (org-level or app-level)
CREATE TABLE webhook_endpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    application_id UUID REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    url TEXT NOT NULL,
    secret VARCHAR(256),
    event_types JSONB NOT NULL DEFAULT '["state_change","switchover","operation","failure"]',
    headers JSONB DEFAULT '{}',
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    retry_count INTEGER NOT NULL DEFAULT 3,
    last_triggered_at TIMESTAMPTZ,
    last_status_code INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_webhook_endpoints_org ON webhook_endpoints (organization_id);
CREATE INDEX idx_webhook_endpoints_app ON webhook_endpoints (application_id);

-- Webhook delivery log (append-only for debugging)
CREATE TABLE webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id UUID NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
    event_type VARCHAR(50) NOT NULL,
    payload JSONB NOT NULL,
    status_code INTEGER,
    response_body TEXT,
    attempt INTEGER NOT NULL DEFAULT 1,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id, delivered_at);
