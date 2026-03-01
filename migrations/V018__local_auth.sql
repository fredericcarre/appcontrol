-- V018: Local authentication support
-- Adds password_hash to users table for local auth mode (non-SSO deployments)

-- Add password_hash column (nullable - only used in local auth mode)
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash TEXT;

-- Add auth_provider column to track how user authenticates
-- Values: 'local', 'oidc', 'saml', 'demo'
ALTER TABLE users ADD COLUMN IF NOT EXISTS auth_provider TEXT NOT NULL DEFAULT 'local';

-- Index for email lookups during login
CREATE INDEX IF NOT EXISTS idx_users_email_auth ON users (email, auth_provider);

-- Note: Default organization and admin user are created by the backend seed process
-- (controlled by SEED_* environment variables), not by migrations.
-- This keeps the migration portable and the seed configurable.

-- Comment explaining the setup
COMMENT ON COLUMN users.password_hash IS 'Bcrypt hash of password. Only used when auth_provider=local. NULL for SSO users.';
COMMENT ON COLUMN users.auth_provider IS 'Authentication method: local (password), oidc, saml, or demo (dev only)';
