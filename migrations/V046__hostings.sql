-- V046: Hostings - Grouping of sites by physical/logical hosting location
-- A hosting represents a datacenter, cloud region, or hosting provider
-- that contains one or more sites.

-- 1. HOSTINGS TABLE
CREATE TABLE IF NOT EXISTS hostings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name VARCHAR(200) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_hostings_org ON hostings(organization_id);

-- 2. ADD hosting_id TO SITES (nullable for backward compat)
ALTER TABLE sites ADD COLUMN IF NOT EXISTS hosting_id UUID REFERENCES hostings(id);

CREATE INDEX IF NOT EXISTS idx_sites_hosting ON sites(hosting_id);

-- 3. COMMENTS
COMMENT ON TABLE hostings IS 'Logical grouping of sites by datacenter or hosting location. Sites within the same hosting share network proximity.';
COMMENT ON COLUMN sites.hosting_id IS 'Optional hosting this site belongs to. NULL means unassigned.';
