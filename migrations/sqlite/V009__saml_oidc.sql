-- V009: SAML/OIDC Authentication Support (SQLite)

-- Add SSO identity columns to users table
ALTER TABLE users ADD COLUMN oidc_sub TEXT;
ALTER TABLE users ADD COLUMN saml_name_id TEXT;

-- SQLite supports partial indexes
CREATE INDEX idx_users_oidc_sub ON users (oidc_sub) WHERE oidc_sub IS NOT NULL;
CREATE INDEX idx_users_saml_name_id ON users (saml_name_id) WHERE saml_name_id IS NOT NULL;

-- SAML group to team mapping
CREATE TABLE saml_group_mappings (
    id TEXT PRIMARY KEY,
    saml_group TEXT NOT NULL,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    default_role TEXT NOT NULL DEFAULT 'viewer'
        CHECK (default_role IN ('admin', 'operator', 'editor', 'viewer')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(saml_group, team_id)
);

CREATE INDEX idx_saml_group_mappings_group ON saml_group_mappings (saml_group);
CREATE INDEX idx_saml_group_mappings_team ON saml_group_mappings (team_id);
