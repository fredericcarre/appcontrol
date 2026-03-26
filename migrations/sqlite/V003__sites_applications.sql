-- V003: Sites and Applications (SQLite)

CREATE TABLE sites (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    code TEXT NOT NULL,
    site_type TEXT NOT NULL DEFAULT 'primary'
        CHECK (site_type IN ('primary', 'dr', 'staging', 'development')),
    location TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, code)
);

CREATE TABLE applications (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    site_id TEXT NOT NULL REFERENCES sites(id),
    tags TEXT DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, name)
);

CREATE INDEX idx_sites_org ON sites (organization_id);
CREATE INDEX idx_applications_org ON applications (organization_id);
CREATE INDEX idx_applications_site ON applications (site_id);
