-- V013: Security & Resilience Tables
-- Implements: agent identity binding, approval workflows, break-glass,
--             credential vault, agent updates, certificate lifecycle

-- ============================================================================
-- Agent Identity & Certificate Tracking
-- ============================================================================

ALTER TABLE agents ADD COLUMN IF NOT EXISTS certificate_fingerprint VARCHAR(128);
ALTER TABLE agents ADD COLUMN IF NOT EXISTS certificate_cn VARCHAR(300);
ALTER TABLE agents ADD COLUMN IF NOT EXISTS identity_verified BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE agents ADD COLUMN IF NOT EXISTS last_version VARCHAR(50);

-- ============================================================================
-- Approval Workflows (4-Eyes Principle)
-- ============================================================================

CREATE TABLE IF NOT EXISTS approval_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    -- What operation is being requested
    operation_type VARCHAR(50) NOT NULL,  -- start, stop, switchover, rebuild, break_glass
    resource_type VARCHAR(50) NOT NULL,   -- application, component
    resource_id UUID NOT NULL,
    -- Risk classification
    risk_level VARCHAR(20) NOT NULL DEFAULT 'medium',  -- low, medium, high, critical
    -- Who requested
    requested_by UUID NOT NULL REFERENCES users(id),
    request_payload JSONB NOT NULL DEFAULT '{}',
    -- Approval status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending, approved, rejected, expired, cancelled
    required_approvals INTEGER NOT NULL DEFAULT 1,
    -- Timing
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    -- Execution tracking
    executed_at TIMESTAMPTZ,
    execution_result JSONB
);

CREATE INDEX idx_approval_requests_org ON approval_requests(organization_id);
CREATE INDEX idx_approval_requests_status ON approval_requests(status) WHERE status = 'pending';
CREATE INDEX idx_approval_requests_resource ON approval_requests(resource_type, resource_id);

CREATE TABLE IF NOT EXISTS approval_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    request_id UUID NOT NULL REFERENCES approval_requests(id),
    -- Who decided
    decided_by UUID NOT NULL REFERENCES users(id),
    decision VARCHAR(20) NOT NULL,  -- approved, rejected
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_approval_decisions_request ON approval_decisions(request_id);

-- Approval config per organization (which operations require approval)
CREATE TABLE IF NOT EXISTS approval_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    operation_type VARCHAR(50) NOT NULL,
    risk_level VARCHAR(20) NOT NULL DEFAULT 'high',
    required_approvals INTEGER NOT NULL DEFAULT 1,
    timeout_minutes INTEGER NOT NULL DEFAULT 15,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, operation_type)
);

-- ============================================================================
-- Break-Glass Emergency Access
-- ============================================================================

CREATE TABLE IF NOT EXISTS break_glass_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    username VARCHAR(100) NOT NULL,
    password_hash VARCHAR(256) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_rotated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, username)
);

-- Break-glass sessions are APPEND-ONLY (Critical Rule #2 extended)
CREATE TABLE IF NOT EXISTS break_glass_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES break_glass_accounts(id),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    activated_by_ip VARCHAR(45) NOT NULL,
    reason TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    actions_taken INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_break_glass_sessions_active ON break_glass_sessions(organization_id)
    WHERE ended_at IS NULL;

-- ============================================================================
-- Agent Update Tasks
-- ============================================================================

CREATE TABLE IF NOT EXISTS agent_update_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    agent_id UUID NOT NULL REFERENCES agents(id),
    target_version VARCHAR(50) NOT NULL,
    binary_url TEXT NOT NULL,
    checksum_sha256 VARCHAR(64) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending, downloading, installing, completed, failed, rolled_back
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    error_message TEXT
);

CREATE INDEX idx_agent_update_tasks_agent ON agent_update_tasks(agent_id);
CREATE INDEX idx_agent_update_tasks_status ON agent_update_tasks(status) WHERE status IN ('pending', 'downloading', 'installing');

-- ============================================================================
-- Credential Vault References
-- ============================================================================

-- Extend app_variables with vault integration fields
ALTER TABLE app_variables ADD COLUMN IF NOT EXISTS vault_path VARCHAR(500);
ALTER TABLE app_variables ADD COLUMN IF NOT EXISTS vault_backend VARCHAR(50) DEFAULT 'builtin';  -- builtin, hashicorp, aws_kms, azure_kv

-- ============================================================================
-- Rate Limiting Configuration (per organization)
-- ============================================================================

ALTER TABLE organizations ADD COLUMN IF NOT EXISTS rate_limit_auth INTEGER DEFAULT 10;        -- per IP per minute
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS rate_limit_operations INTEGER DEFAULT 5;    -- per user per minute
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS rate_limit_reads INTEGER DEFAULT 200;       -- per user per minute

-- ============================================================================
-- Certificate Tracking
-- ============================================================================

CREATE TABLE IF NOT EXISTS certificate_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID REFERENCES agents(id),
    gateway_id UUID,
    event_type VARCHAR(50) NOT NULL,  -- issued, renewed, expired, revoked, renewal_requested
    fingerprint VARCHAR(128),
    cn VARCHAR(300),
    issued_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_certificate_events_agent ON certificate_events(agent_id);
