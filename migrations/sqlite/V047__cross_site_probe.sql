-- V047: Cross-site probe — detect components running on the wrong site (SQLite)

ALTER TABLE components ADD COLUMN detected_site_id TEXT REFERENCES sites(id);
ALTER TABLE components ADD COLUMN passive_check_at TEXT;
ALTER TABLE components ADD COLUMN passive_site_status TEXT CHECK (passive_site_status IS NULL OR passive_site_status IN ('active', 'inactive'));

CREATE INDEX IF NOT EXISTS idx_components_detected_site ON components(detected_site_id);
CREATE INDEX IF NOT EXISTS idx_components_passive_status ON components(passive_site_status);
