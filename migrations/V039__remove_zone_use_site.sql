-- V039: Migration from zone to site_id
--
-- This migration replaces the gateway "zone" concept with proper site_id relationships.
-- Gateways will be grouped by site_id for failover, not by zone string.
--
-- Changes:
-- 1. Create sites for each existing zone that doesn't have a site
-- 2. Assign gateways without site_id to their corresponding site (by zone code)
-- 3. Recreate unique index for primary per site (not zone)
-- 4. Make zone column nullable (deprecated)
-- 5. Add site_id to enrollment_tokens

-- ============================================================================
-- Step 1: Create sites for each zone that doesn't have a corresponding site
-- ============================================================================

-- For each gateway zone, create a site if one doesn't exist with that code
INSERT INTO sites (id, organization_id, name, code, site_type, is_active)
SELECT DISTINCT
    gen_random_uuid(),
    g.organization_id,
    g.zone,           -- name = zone
    g.zone,           -- code = zone
    'primary',
    true
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

UPDATE gateways g
SET site_id = s.id
FROM sites s
WHERE g.site_id IS NULL
  AND g.zone IS NOT NULL
  AND g.zone != ''
  AND s.organization_id = g.organization_id
  AND s.code = g.zone;

-- ============================================================================
-- Step 3: Recreate unique index for one primary per site (not per zone)
-- ============================================================================

-- Drop the old zone-based unique index
DROP INDEX IF EXISTS idx_gateways_one_primary_per_zone;

-- Create new site-based unique index
-- Only one primary gateway allowed per site
CREATE UNIQUE INDEX IF NOT EXISTS idx_gateways_one_primary_per_site
  ON gateways (organization_id, site_id)
  WHERE is_primary = true AND is_active = true AND site_id IS NOT NULL;

-- ============================================================================
-- Step 4: Make zone column nullable (deprecated, kept for backward compat)
-- ============================================================================

ALTER TABLE gateways ALTER COLUMN zone DROP NOT NULL;

-- ============================================================================
-- Step 5: Add site_id to enrollment_tokens
-- ============================================================================

-- Add site_id column to enrollment_tokens (replaces zone-based scoping)
ALTER TABLE enrollment_tokens ADD COLUMN IF NOT EXISTS site_id UUID REFERENCES sites(id);

-- Create index for site-scoped token lookup
CREATE INDEX IF NOT EXISTS idx_enrollment_tokens_site
  ON enrollment_tokens (organization_id, site_id)
  WHERE revoked_at IS NULL;

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON COLUMN gateways.zone IS 'DEPRECATED: Legacy zone string. Use site_id instead. Kept for backward compatibility with older gateways.';
COMMENT ON COLUMN enrollment_tokens.site_id IS 'If set, this token is only valid for gateways assigned to this site. NULL means valid everywhere.';
