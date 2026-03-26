-- V022: Platform Admin, Gateway-Site Binding, Certificate Revocation (SQLite)
-- Adds super-admin role, links gateways to sites, and enables cert revocation.

-- ============================================================================
-- Platform-level super-admin
-- ============================================================================
-- Distinguishes platform super-admins (who can create orgs) from org admins.
-- NULL = regular user (role field determines org-level access).
-- 'super_admin' = platform administrator who can create and manage organizations.

ALTER TABLE users ADD COLUMN platform_role TEXT
    CHECK (platform_role IS NULL OR platform_role IN ('super_admin'));

-- ============================================================================
-- Gateway-Site binding
-- ============================================================================
-- Each gateway belongs to a site (datacenter/DR/staging).

ALTER TABLE gateways ADD COLUMN site_id TEXT REFERENCES sites(id);
ALTER TABLE gateways ADD COLUMN certificate_fingerprint TEXT;
ALTER TABLE gateways ADD COLUMN certificate_cn TEXT;

CREATE INDEX IF NOT EXISTS idx_gateways_site ON gateways(site_id);

-- ============================================================================
-- Revoked Certificates
-- ============================================================================
-- When an agent or gateway is compromised, its cert is revoked here.
-- Gateways and backend check this table on every mTLS connection.
-- APPEND-ONLY (Critical Rule #2 extended).

CREATE TABLE IF NOT EXISTS revoked_certificates (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    fingerprint TEXT NOT NULL,
    cn TEXT,
    -- What was revoked
    agent_id TEXT REFERENCES agents(id),
    gateway_id TEXT,
    -- Why and who
    reason TEXT NOT NULL,
    revoked_by TEXT NOT NULL REFERENCES users(id),
    revoked_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_revoked_certs_org ON revoked_certificates(organization_id);
CREATE INDEX IF NOT EXISTS idx_revoked_certs_fingerprint ON revoked_certificates(fingerprint);

-- ============================================================================
-- Password hash for local users (created by org admin)
-- ============================================================================
-- Users created via OIDC/SAML have NULL password_hash.
-- Users created locally by org admin have a bcrypt hash.
-- Note: V018 may have added these columns already

-- ALTER TABLE users ADD COLUMN password_hash TEXT;
-- ALTER TABLE users ADD COLUMN auth_provider TEXT NOT NULL DEFAULT 'external'
--     CHECK (auth_provider IN ('local', 'oidc', 'saml', 'external'));
