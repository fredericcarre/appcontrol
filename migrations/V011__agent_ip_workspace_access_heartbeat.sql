-- V011: Agent IP addresses, workspace-site access control, heartbeat timeout config
--
-- Three features:
-- 1. Agents can report their IP addresses (FQDN + IP support, e.g. Azure VMs without proper DNS)
-- 2. Workspace-site binding for zone/gateway access control
-- 3. Heartbeat timeout configuration per organization

-- 1. Agent IP addresses
ALTER TABLE agents ADD COLUMN ip_addresses JSONB DEFAULT '[]';

-- 2. Workspace-site access control
-- Links workspaces to specific sites — users in a workspace can only see/operate
-- on components whose agents are on allowed sites.
CREATE TABLE workspace_sites (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, site_id)
);

CREATE TABLE workspace_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member'
        CHECK (role IN ('admin', 'member')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Either user_id or team_id must be set, not both
    CONSTRAINT workspace_member_type CHECK (
        (user_id IS NOT NULL AND team_id IS NULL)
        OR (user_id IS NULL AND team_id IS NOT NULL)
    ),
    -- Unique per user or team in workspace
    UNIQUE(workspace_id, user_id),
    UNIQUE(workspace_id, team_id)
);

CREATE INDEX idx_workspace_sites_workspace ON workspace_sites (workspace_id);
CREATE INDEX idx_workspace_sites_site ON workspace_sites (site_id);
CREATE INDEX idx_workspace_members_workspace ON workspace_members (workspace_id);
CREATE INDEX idx_workspace_members_user ON workspace_members (user_id);
CREATE INDEX idx_workspace_members_team ON workspace_members (team_id);

-- 3. Heartbeat timeout configuration (per organization, default 180s = 3 heartbeats)
ALTER TABLE organizations ADD COLUMN heartbeat_timeout_seconds INTEGER NOT NULL DEFAULT 180;
