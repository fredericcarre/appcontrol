-- V018: Local authentication support
-- Adds password_hash to users table for local auth mode (non-SSO deployments)

-- Add password_hash column (nullable - only used in local auth mode)
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash TEXT;

-- Add auth_provider column to track how user authenticates
-- Values: 'local', 'oidc', 'saml', 'demo'
ALTER TABLE users ADD COLUMN IF NOT EXISTS auth_provider TEXT NOT NULL DEFAULT 'local';

-- Index for email lookups during login
CREATE INDEX IF NOT EXISTS idx_users_email_auth ON users (email, auth_provider);

-- Create default demo organization and admin user
-- Password: 'admin' (bcrypt hash)
-- This seed runs on every migration but uses ON CONFLICT DO NOTHING
INSERT INTO organizations (id, name, slug)
VALUES ('00000000-0000-0000-0000-000000000001', 'Default Organization', 'default')
ON CONFLICT (id) DO NOTHING;

-- Default admin user with password 'admin'
-- bcrypt hash of 'admin' with cost 12
INSERT INTO users (id, organization_id, external_id, email, display_name, role, auth_provider, password_hash)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'admin',
    'admin@local',
    'Administrator',
    'admin',
    'local',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/X4.V4ferKlQF4.Kpu'
)
ON CONFLICT (id) DO NOTHING;

-- Comment explaining the setup
COMMENT ON COLUMN users.password_hash IS 'Bcrypt hash of password. Only used when auth_provider=local. NULL for SSO users.';
COMMENT ON COLUMN users.auth_provider IS 'Authentication method: local (password), oidc, saml, or demo (dev only)';
