-- V044: Add missing columns discovered by schema audit (SQLite)
-- These columns exist in queries but were missing from SQLite migrations.

-- 1. check_events: add agent_id and stderr columns
ALTER TABLE check_events ADD COLUMN agent_id TEXT;
ALTER TABLE check_events ADD COLUMN stderr TEXT;

-- 2. agents: add os_info column (used by heartbeat update)
ALTER TABLE agents ADD COLUMN os_info TEXT;

-- 3. gateway_status_events: add agent_id column (used by agent status events)
ALTER TABLE gateway_status_events ADD COLUMN agent_id TEXT;

-- 4. revoked_certificates: add UNIQUE constraint on fingerprint (needed for ON CONFLICT)
CREATE UNIQUE INDEX IF NOT EXISTS idx_revoked_certificates_fingerprint_unique
    ON revoked_certificates (fingerprint);
