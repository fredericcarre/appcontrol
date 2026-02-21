-- V009: SAML/OIDC Authentication Support
--
-- Adds SSO identity columns to users and creates the
-- saml_group_mappings table for group→team→permission mapping.

-- Add SSO identity columns to users table
ALTER TABLE users ADD COLUMN oidc_sub VARCHAR(500);
ALTER TABLE users ADD COLUMN saml_name_id VARCHAR(500);

CREATE INDEX idx_users_oidc_sub ON users (oidc_sub) WHERE oidc_sub IS NOT NULL;
CREATE INDEX idx_users_saml_name_id ON users (saml_name_id) WHERE saml_name_id IS NOT NULL;

-- SAML group → AppControl team mapping table
-- Each row maps an external IdP group (e.g., AD "CN=APP_PAYMENTS_OPERATORS,OU=Groups,DC=corp,DC=com")
-- to an AppControl team with a default role.
--
-- On SAML login:
--   1. Extract group claims from the SAML assertion
--   2. For each group, look up this table to find the target team
--   3. Add user to team (if not already member)
--   4. Remove user from teams whose SAML group is no longer in the assertion
--   5. The team's app_permissions_teams grant drives app access
--
-- Example:
--   AD group "APP_PAYMENTS_OPERATORS"  → team "Payments-Ops"   → operate on "Paiements-SEPA"
--   AD group "APP_PAYMENTS_ADMINS"     → team "Payments-Admin"  → manage on "Paiements-SEPA"
--   AD group "APPCONTROL_ADMINS"       → role=admin (org-wide)
CREATE TABLE saml_group_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saml_group VARCHAR(1000) NOT NULL,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    default_role VARCHAR(20) NOT NULL DEFAULT 'viewer'
        CHECK (default_role IN ('admin', 'operator', 'editor', 'viewer')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(saml_group, team_id)
);

CREATE INDEX idx_saml_group_mappings_group ON saml_group_mappings (saml_group);
CREATE INDEX idx_saml_group_mappings_team ON saml_group_mappings (team_id);
