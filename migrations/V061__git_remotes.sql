-- V061: Git remotes & per-application sync settings.
--
-- Materialises the GitOps story described in the methodology
-- (Phase 3 §4.5: "La map est dans un repository Git") and the
-- vision document. Each organisation can declare one or more Git
-- remotes; applications opt in to remote sync individually.
--
-- Token handling: we do NOT store the actual credential in the DB.
-- Instead we record the *name of the environment variable* that the
-- backend will read at push time. This keeps secrets out of database
-- backups and audit dumps, and lets operators rotate via standard
-- envvar deploys (Helm secret, systemd unit, etc.).
--
-- Provider is an enum-ish VARCHAR for forward compatibility: the
-- initial implementation ships GitHub Contents API; GitLab and Gitea
-- are stubbed as future providers and will share the same row shape.

CREATE TABLE git_remotes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(200) NOT NULL,
    provider VARCHAR(40) NOT NULL,              -- 'github' | 'gitlab' | 'gitea' | 'shell'
    base_url TEXT NOT NULL DEFAULT 'https://api.github.com',
    repo VARCHAR(300) NOT NULL,                 -- e.g. 'fredericcarre/appcontrol-maps'
    branch VARCHAR(120) NOT NULL DEFAULT 'main',
    token_env_var VARCHAR(120) NOT NULL,        -- e.g. 'GIT_TOKEN_BILLING_MAPS'
    default_path_template TEXT NOT NULL DEFAULT 'apps/{app_id}/map.json',
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    last_push_at TIMESTAMPTZ,
    last_push_sha VARCHAR(80),
    last_push_status VARCHAR(20),                -- 'ok' | 'error'
    last_push_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, name)
);

CREATE INDEX idx_git_remotes_org ON git_remotes (organization_id);

-- Per-application binding to a Git remote (optional path override).
CREATE TABLE application_git_settings (
    application_id UUID PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
    git_remote_id UUID NOT NULL REFERENCES git_remotes(id) ON DELETE RESTRICT,
    path_override TEXT,                          -- NULL = use remote's default_path_template
    auto_push_on_change BOOLEAN NOT NULL DEFAULT false,
    last_push_at TIMESTAMPTZ,
    last_push_sha VARCHAR(80),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_application_git_settings_remote ON application_git_settings (git_remote_id);
