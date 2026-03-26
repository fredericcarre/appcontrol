-- V018: Local authentication support (SQLite)

-- Add password_hash column
ALTER TABLE users ADD COLUMN password_hash TEXT;

-- Add auth_provider column
ALTER TABLE users ADD COLUMN auth_provider TEXT NOT NULL DEFAULT 'local';

-- Index for email lookups during login
CREATE INDEX idx_users_email_auth ON users (email, auth_provider);
