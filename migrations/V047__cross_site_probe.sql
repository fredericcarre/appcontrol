-- V047: Cross-site probe — detect components running on the wrong site
-- When a DR binding profile exists, the backend can periodically check
-- the passive site to detect if a component is running there unexpectedly.

-- Track the site where a component is actually detected as running.
-- NULL means no cross-site detection has occurred yet.
ALTER TABLE components ADD COLUMN IF NOT EXISTS detected_site_id UUID REFERENCES sites(id);

-- Track when the last passive check was performed.
ALTER TABLE components ADD COLUMN IF NOT EXISTS passive_check_at TIMESTAMPTZ;

-- Track the result of the last passive check.
-- 'inactive' = not running on passive site (normal)
-- 'active' = running on passive site (warning!)
-- NULL = never checked
ALTER TABLE components ADD COLUMN IF NOT EXISTS passive_site_status VARCHAR(20)
    CHECK (passive_site_status IS NULL OR passive_site_status IN ('active', 'inactive'));

CREATE INDEX IF NOT EXISTS idx_components_detected_site ON components(detected_site_id) WHERE detected_site_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_components_passive_status ON components(passive_site_status) WHERE passive_site_status = 'active';

COMMENT ON COLUMN components.detected_site_id IS 'Site where the component was last detected as running (via cross-site probe). May differ from expected site.';
COMMENT ON COLUMN components.passive_site_status IS 'Result of last passive site check: active (running on wrong site), inactive (normal), NULL (never checked).';
