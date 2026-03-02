-- V025: Certificate rotation support
-- Adds infrastructure for seamless CA rotation during PKI migration.

-- Track pending CA rotation state per organization.
-- When a new CA is imported for rotation, it's stored here until all
-- agents/gateways have migrated, then it becomes the primary CA.
ALTER TABLE organizations ADD COLUMN pending_ca_cert_pem TEXT;
ALTER TABLE organizations ADD COLUMN pending_ca_key_pem TEXT;
ALTER TABLE organizations ADD COLUMN rotation_started_at TIMESTAMPTZ;

-- Track certificate rotation progress (APPEND-ONLY for audit).
-- Records when each agent/gateway successfully rotates to the new CA.
CREATE TABLE certificate_rotations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    rotation_id UUID NOT NULL,
    -- One of agent_id or gateway_id will be set
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    gateway_id UUID REFERENCES gateways(id) ON DELETE SET NULL,
    -- Certificate fingerprints before and after rotation
    old_fingerprint VARCHAR(128),
    new_fingerprint VARCHAR(128),
    -- Rotation lifecycle
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    rotated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Additional context
    hostname VARCHAR(300),
    error_message TEXT,
    CONSTRAINT chk_entity CHECK (
        (agent_id IS NOT NULL AND gateway_id IS NULL) OR
        (agent_id IS NULL AND gateway_id IS NOT NULL)
    )
);

-- Index for looking up rotation progress
CREATE INDEX idx_cert_rotations_org_rotation ON certificate_rotations(organization_id, rotation_id);
CREATE INDEX idx_cert_rotations_agent ON certificate_rotations(agent_id) WHERE agent_id IS NOT NULL;
CREATE INDEX idx_cert_rotations_gateway ON certificate_rotations(gateway_id) WHERE gateway_id IS NOT NULL;

-- Track overall rotation status per organization
CREATE TABLE rotation_progress (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    rotation_id UUID NOT NULL UNIQUE,
    -- Counts
    total_agents INTEGER NOT NULL DEFAULT 0,
    total_gateways INTEGER NOT NULL DEFAULT 0,
    migrated_agents INTEGER NOT NULL DEFAULT 0,
    migrated_gateways INTEGER NOT NULL DEFAULT 0,
    failed_agents INTEGER NOT NULL DEFAULT 0,
    failed_gateways INTEGER NOT NULL DEFAULT 0,
    -- Lifecycle
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    finalized_at TIMESTAMPTZ,
    status VARCHAR(32) NOT NULL DEFAULT 'in_progress',
    -- Metadata
    initiated_by UUID REFERENCES users(id),
    grace_period_secs INTEGER NOT NULL DEFAULT 3600
);

CREATE INDEX idx_rotation_progress_org ON rotation_progress(organization_id);
