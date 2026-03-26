-- V001: Organizations and Users (SQLite)
-- Adaptations:
-- - UUID stored as TEXT (36 chars)
-- - TIMESTAMPTZ stored as TEXT (ISO8601)
-- - JSONB stored as TEXT (JSON)
-- - No gen_random_uuid() - app must provide UUIDs
-- - BOOLEAN stored as INTEGER (0/1)

CREATE TABLE organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    settings TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    external_id TEXT NOT NULL,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'viewer'
        CHECK (role IN ('admin', 'operator', 'editor', 'viewer')),
    is_active INTEGER NOT NULL DEFAULT 1,
    last_login_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, external_id)
);

CREATE INDEX idx_users_org ON users (organization_id);
CREATE INDEX idx_users_email ON users (email);
