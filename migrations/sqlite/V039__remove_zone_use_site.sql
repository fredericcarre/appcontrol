-- V039: Migration from zone to site_id (SQLite)
--
-- This migration replaces the gateway "zone" concept with proper site_id relationships.
-- Gateways will be grouped by site_id for failover, not by zone string.
--
-- Changes:
-- 1. Create sites for each existing zone that doesn't have a site
-- 2. Assign gateways without site_id to their corresponding site (by zone code)
-- 3. Make zone column nullable (deprecated)
-- 4. Add site_id to enrollment_tokens

-- ============================================================================
-- Step 1: Create sites for each zone that doesn't have a corresponding site
-- ============================================================================

-- For each gateway zone, create a site if one doesn't exist with that code
-- Note: SQLite uses app-generated UUIDs, so we use subselect pattern
INSERT INTO sites (id, organization_id, name, code, site_type, is_active, created_at, updated_at)
SELECT DISTINCT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    g.organization_id,
    g.zone,           -- name = zone
    g.zone,           -- code = zone
    'primary',
    1,
    datetime('now'),
    datetime('now')
FROM gateways g
WHERE g.site_id IS NULL
  AND g.zone IS NOT NULL
  AND g.zone != ''
  AND NOT EXISTS (
    SELECT 1 FROM sites s
    WHERE s.organization_id = g.organization_id AND s.code = g.zone
  );

-- ============================================================================
-- Step 2: Assign gateways without site_id to their corresponding site
-- ============================================================================

UPDATE gateways
SET site_id = (
    SELECT s.id FROM sites s
    WHERE s.organization_id = gateways.organization_id
      AND s.code = gateways.zone
)
WHERE site_id IS NULL
  AND zone IS NOT NULL
  AND zone != '';

-- ============================================================================
-- Step 3: Zone column is already nullable in SQLite (no action needed)
-- ============================================================================

-- ============================================================================
-- Step 4: Add site_id to enrollment_tokens
-- ============================================================================

-- Add site_id column to enrollment_tokens (replaces zone-based scoping)
ALTER TABLE enrollment_tokens ADD COLUMN site_id TEXT REFERENCES sites(id);

-- Create index for site-scoped token lookup
CREATE INDEX IF NOT EXISTS idx_enrollment_tokens_site
  ON enrollment_tokens (organization_id, site_id);
