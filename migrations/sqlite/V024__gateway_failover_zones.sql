-- V024: Gateway Failover Groups and Zone-scoped Enrollment (SQLite)
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
ALTER TABLE gateways ADD COLUMN is_primary INTEGER NOT NULL DEFAULT 0;
ALTER TABLE gateways ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;

-- Track when gateway last reported healthy (for failover detection)
ALTER TABLE gateways ADD COLUMN last_heartbeat_at TEXT;

-- Note: SQLite partial unique indexes have limited support
-- We'll enforce one primary per zone in application code

-- Index for efficient zone lookups
CREATE INDEX IF NOT EXISTS idx_gateways_zone_priority
  ON gateways (organization_id, zone, priority, is_active);

-- ============================================================================
-- Zone-scoped enrollment tokens
-- ============================================================================

-- Add zone restriction to enrollment tokens
-- NULL = valid on all zones (super-admin token)
-- 'europe-west' = only valid on gateways in that zone
ALTER TABLE enrollment_tokens ADD COLUMN zone TEXT;

-- Index for zone-scoped token lookup during enrollment
CREATE INDEX IF NOT EXISTS idx_enrollment_tokens_zone
  ON enrollment_tokens (organization_id, zone);

-- ============================================================================
-- Gateway status events (audit trail for failover)
-- ============================================================================

CREATE TABLE IF NOT EXISTS gateway_status_events (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    gateway_id TEXT NOT NULL REFERENCES gateways(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL, -- 'connected', 'disconnected', 'failover_activated', 'failover_deactivated', 'promoted_to_primary'
    previous_state TEXT,  -- JSON
    new_state TEXT,       -- JSON
    triggered_by TEXT,    -- 'heartbeat_timeout', 'manual', 'auto_election'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_gateway_status_events_gateway
  ON gateway_status_events (gateway_id, created_at);

CREATE INDEX IF NOT EXISTS idx_gateway_status_events_org_time
  ON gateway_status_events (organization_id, created_at);
