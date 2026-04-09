-- V046: Hostings - Grouping of sites by physical/logical hosting location (SQLite)

CREATE TABLE IF NOT EXISTS hostings (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_hostings_org ON hostings(organization_id);

-- SQLite does not support ADD COLUMN IF NOT EXISTS, use a safe approach
ALTER TABLE sites ADD COLUMN hosting_id TEXT REFERENCES hostings(id);

CREATE INDEX IF NOT EXISTS idx_sites_hosting ON sites(hosting_id);
