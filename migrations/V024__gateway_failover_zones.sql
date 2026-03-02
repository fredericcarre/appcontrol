-- V024: Gateway Failover Groups and Zone-scoped Enrollment
--
-- This migration adds:
-- 1. Primary/standby designation for gateways within a zone
-- 2. Priority ordering for automatic failover
-- 3. Zone-scoped enrollment tokens for security
-- 4. Gateway heartbeat tracking for failover detection

-- ============================================================================
-- Gateway failover support
-- ============================================================================

-- Add primary/standby and priority for failover ordering
ALTER TABLE gateways ADD COLUMN IF NOT EXISTS is_primary BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE gateways ADD COLUMN IF NOT EXISTS priority INT NOT NULL DEFAULT 0;

-- Track when gateway last reported healthy (for failover detection)
-- Note: last_heartbeat_at may already exist from V011, this is idempotent
ALTER TABLE gateways ADD COLUMN IF NOT EXISTS last_heartbeat_at TIMESTAMPTZ;

-- Ensure only one primary per zone per organization
-- This allows multiple standby gateways in the same zone
CREATE UNIQUE INDEX IF NOT EXISTS idx_gateways_one_primary_per_zone
  ON gateways (organization_id, zone)
  WHERE is_primary = true AND is_active = true;

-- Index for efficient zone lookups
CREATE INDEX IF NOT EXISTS idx_gateways_zone_priority
  ON gateways (organization_id, zone, priority, is_active);

-- ============================================================================
-- Zone-scoped enrollment tokens
-- ============================================================================

-- Add zone restriction to enrollment tokens
-- NULL = valid on all zones (super-admin token)
-- 'europe-west' = only valid on gateways in that zone
ALTER TABLE enrollment_tokens ADD COLUMN IF NOT EXISTS zone VARCHAR(100);

-- Index for zone-scoped token lookup during enrollment
CREATE INDEX IF NOT EXISTS idx_enrollment_tokens_zone
  ON enrollment_tokens (organization_id, zone)
  WHERE revoked_at IS NULL;

-- ============================================================================
-- Gateway status events (audit trail for failover)
-- ============================================================================

CREATE TABLE IF NOT EXISTS gateway_status_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    gateway_id UUID NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
    event_type VARCHAR(50) NOT NULL, -- 'connected', 'disconnected', 'failover_activated', 'failover_deactivated', 'promoted_to_primary'
    previous_state JSONB,
    new_state JSONB,
    triggered_by VARCHAR(100), -- 'heartbeat_timeout', 'manual', 'auto_election'
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_gateway_status_events_gateway
  ON gateway_status_events (gateway_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_gateway_status_events_org_time
  ON gateway_status_events (organization_id, created_at DESC);

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON COLUMN gateways.is_primary IS 'True if this gateway is the primary for its zone. Only one primary allowed per zone.';
COMMENT ON COLUMN gateways.priority IS 'Failover priority (0 = highest). Lower priority gateways become active first if primary fails.';
COMMENT ON COLUMN enrollment_tokens.zone IS 'If set, this token is only valid for enrollment via gateways in this zone. NULL means valid everywhere.';
COMMENT ON TABLE gateway_status_events IS 'Audit trail for gateway status changes, especially failover events.';
