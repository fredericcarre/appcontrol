-- V011: Agent IP addresses, workspace-site access control, heartbeat timeout (SQLite)

-- 1. Agent IP addresses (already included in base agents table for SQLite)
-- ALTER TABLE agents ADD COLUMN ip_addresses TEXT DEFAULT '[]';

-- 2. Workspace-site access control
CREATE TABLE workspace_sites (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (workspace_id, site_id)
);

CREATE TABLE workspace_members (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    team_id TEXT REFERENCES teams(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member'
        CHECK (role IN ('admin', 'member')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(workspace_id, user_id),
    UNIQUE(workspace_id, team_id)
);

CREATE INDEX idx_workspace_sites_workspace ON workspace_sites (workspace_id);
CREATE INDEX idx_workspace_sites_site ON workspace_sites (site_id);
CREATE INDEX idx_workspace_members_workspace ON workspace_members (workspace_id);
CREATE INDEX idx_workspace_members_user ON workspace_members (user_id);
CREATE INDEX idx_workspace_members_team ON workspace_members (team_id);

-- 3. Heartbeat timeout configuration (already in base organizations table for SQLite)
-- ALTER TABLE organizations ADD COLUMN heartbeat_timeout_seconds INTEGER NOT NULL DEFAULT 180;
