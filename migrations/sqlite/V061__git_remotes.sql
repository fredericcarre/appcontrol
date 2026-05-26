-- V061: Git remotes & per-application sync settings (SQLite mirror).
-- See migrations/V061 for the full rationale.

CREATE TABLE git_remotes (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT NOT NULL DEFAULT 'https://api.github.com',
    repo TEXT NOT NULL,
    branch TEXT NOT NULL DEFAULT 'main',
    token_env_var TEXT NOT NULL,
    default_path_template TEXT NOT NULL DEFAULT 'apps/{app_id}/map.json',
    is_enabled BOOLEAN NOT NULL DEFAULT 1,
    last_push_at TIMESTAMP,
    last_push_sha TEXT,
    last_push_status TEXT,
    last_push_error TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (organization_id, name)
);

CREATE INDEX idx_git_remotes_org ON git_remotes (organization_id);

CREATE TABLE application_git_settings (
    application_id TEXT PRIMARY KEY,
    git_remote_id TEXT NOT NULL,
    path_override TEXT,
    auto_push_on_change BOOLEAN NOT NULL DEFAULT 0,
    last_push_at TIMESTAMP,
    last_push_sha TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_application_git_settings_remote ON application_git_settings (git_remote_id);
