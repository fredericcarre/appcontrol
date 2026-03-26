-- V016: FSM state cache, webhook notifications (SQLite)

-- Add current_state column to components (already in base schema for SQLite)
-- ALTER TABLE components ADD COLUMN current_state TEXT NOT NULL DEFAULT 'UNKNOWN';
CREATE INDEX IF NOT EXISTS idx_components_state ON components (current_state);

-- Webhook endpoints
CREATE TABLE webhook_endpoints (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    application_id TEXT REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    secret TEXT,
    event_types TEXT NOT NULL DEFAULT '["state_change","switchover","operation","failure"]',
    headers TEXT DEFAULT '{}',
    is_enabled INTEGER NOT NULL DEFAULT 1,
    retry_count INTEGER NOT NULL DEFAULT 3,
    last_triggered_at TEXT,
    last_status_code INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_webhook_endpoints_org ON webhook_endpoints (organization_id);
CREATE INDEX idx_webhook_endpoints_app ON webhook_endpoints (application_id);

-- Webhook delivery log
CREATE TABLE webhook_deliveries (
    id TEXT PRIMARY KEY,
    webhook_id TEXT NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status_code INTEGER,
    response_body TEXT,
    attempt INTEGER NOT NULL DEFAULT 1,
    delivered_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id, delivered_at);
