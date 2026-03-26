-- V015: Enrollment Tokens & CA Storage (SQLite)

-- Organization CA Storage
ALTER TABLE organizations ADD COLUMN ca_cert_pem TEXT;
ALTER TABLE organizations ADD COLUMN ca_key_pem TEXT;

-- Enrollment Tokens
CREATE TABLE enrollment_tokens (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    name TEXT NOT NULL,
    max_uses INTEGER,
    current_uses INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'agent',
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at TEXT,
    revoked_by TEXT REFERENCES users(id)
);

CREATE INDEX idx_enrollment_tokens_org ON enrollment_tokens(organization_id);
CREATE INDEX idx_enrollment_tokens_hash ON enrollment_tokens(token_hash);
CREATE INDEX idx_enrollment_tokens_active ON enrollment_tokens(organization_id)
    WHERE revoked_at IS NULL AND (max_uses IS NULL OR current_uses < max_uses);

-- Enrollment Events (APPEND-ONLY)
CREATE TABLE enrollment_events (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    token_id TEXT REFERENCES enrollment_tokens(id),
    event_type TEXT NOT NULL,
    hostname TEXT,
    ip_address TEXT,
    agent_id TEXT,
    cert_fingerprint TEXT,
    cert_cn TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_enrollment_events_org ON enrollment_events(organization_id, created_at);
CREATE INDEX idx_enrollment_events_token ON enrollment_events(token_id);
