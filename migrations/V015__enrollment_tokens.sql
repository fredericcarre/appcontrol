-- V015: Enrollment Tokens & CA Storage
-- Provides token-based agent enrollment and organization CA management.

-- ============================================================================
-- Organization CA Storage
-- ============================================================================
-- Each organization has its own CA for issuing agent/gateway certificates.
-- The CA is generated once (via `appctl pki init` or the UI) and stored here.

ALTER TABLE organizations ADD COLUMN IF NOT EXISTS ca_cert_pem TEXT;
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS ca_key_pem TEXT;

-- ============================================================================
-- Enrollment Tokens
-- ============================================================================
-- One-time or multi-use tokens for agent enrollment.
-- Created via CLI (`appctl pki create-token`), API, or the UI.

CREATE TABLE IF NOT EXISTS enrollment_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    -- Token value (hashed with SHA-256 for storage, never stored in clear)
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    -- Token prefix for identification (first 8 chars of token, e.g. "ac_enrol")
    token_prefix VARCHAR(20) NOT NULL,
    -- Human-readable label
    name VARCHAR(200) NOT NULL,
    -- Usage limits
    max_uses INTEGER,           -- NULL = unlimited
    current_uses INTEGER NOT NULL DEFAULT 0,
    -- Validity
    expires_at TIMESTAMPTZ NOT NULL,
    -- Scope: what kind of cert can this token issue?
    scope VARCHAR(20) NOT NULL DEFAULT 'agent',  -- agent, gateway
    -- Who created it
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Revocation
    revoked_at TIMESTAMPTZ,
    revoked_by UUID REFERENCES users(id)
);

CREATE INDEX idx_enrollment_tokens_org ON enrollment_tokens(organization_id);
CREATE INDEX idx_enrollment_tokens_hash ON enrollment_tokens(token_hash);
CREATE INDEX idx_enrollment_tokens_active ON enrollment_tokens(organization_id)
    WHERE revoked_at IS NULL AND (max_uses IS NULL OR current_uses < max_uses);

-- ============================================================================
-- Enrollment Audit Log
-- ============================================================================
-- Every enrollment attempt is logged (success or failure).
-- APPEND-ONLY (Critical Rule #2 extended).

CREATE TABLE IF NOT EXISTS enrollment_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    token_id UUID REFERENCES enrollment_tokens(id),
    -- What happened
    event_type VARCHAR(20) NOT NULL,  -- success, token_expired, token_revoked, token_exhausted, invalid_token
    -- Who/what enrolled
    hostname VARCHAR(300),
    ip_address VARCHAR(45),
    agent_id UUID,
    -- Certificate issued (if success)
    cert_fingerprint VARCHAR(128),
    cert_cn VARCHAR(300),
    -- Timing
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_enrollment_events_org ON enrollment_events(organization_id, created_at);
CREATE INDEX idx_enrollment_events_token ON enrollment_events(token_id);
