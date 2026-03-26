-- V006: Teams, Permissions, Sharing (SQLite)

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, name)
);

CREATE TABLE teams (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, name)
);

CREATE TABLE team_members (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member'
        CHECK (role IN ('lead', 'member')),
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(team_id, user_id)
);

CREATE TABLE app_permissions_users (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_level TEXT NOT NULL
        CHECK (permission_level IN ('view','operate','edit','manage','owner')),
    granted_by TEXT NOT NULL REFERENCES users(id),
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(application_id, user_id)
);

CREATE TABLE app_permissions_teams (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    permission_level TEXT NOT NULL
        CHECK (permission_level IN ('view','operate','edit','manage','owner')),
    granted_by TEXT NOT NULL REFERENCES users(id),
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(application_id, team_id)
);

CREATE TABLE app_share_links (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    permission_level TEXT NOT NULL
        CHECK (permission_level IN ('view','operate','edit')),
    created_by TEXT NOT NULL REFERENCES users(id),
    expires_at TEXT,
    max_uses INTEGER,
    use_count INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE user_favorites (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, application_id)
);

CREATE TABLE saved_views (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    view_config TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_teams_org ON teams (organization_id);
CREATE INDEX idx_team_members_team ON team_members (team_id);
CREATE INDEX idx_team_members_user ON team_members (user_id);
CREATE INDEX idx_app_perms_users_app ON app_permissions_users (application_id);
CREATE INDEX idx_app_perms_users_user ON app_permissions_users (user_id);
CREATE INDEX idx_app_perms_teams_app ON app_permissions_teams (application_id);
CREATE INDEX idx_app_perms_teams_team ON app_permissions_teams (team_id);
CREATE INDEX idx_share_links_app ON app_share_links (application_id);
CREATE INDEX idx_share_links_token ON app_share_links (token);
CREATE INDEX idx_user_favorites_user ON user_favorites (user_id);
CREATE INDEX idx_saved_views_user ON saved_views (user_id, application_id);
