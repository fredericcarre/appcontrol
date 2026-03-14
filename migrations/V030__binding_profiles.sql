-- V030: Binding Profiles for Map Import with Gateway Resolution and DR Support

-- 1. BINDING PROFILES TABLE
CREATE TABLE IF NOT EXISTS binding_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    profile_type VARCHAR(20) NOT NULL DEFAULT 'custom'
        CHECK (profile_type IN ('primary', 'dr', 'custom')),
    is_active BOOLEAN NOT NULL DEFAULT false,
    gateway_ids UUID[] NOT NULL DEFAULT '{}',
    auto_failover BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES users(id),
    UNIQUE(application_id, name)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_binding_profiles_active
    ON binding_profiles (application_id) WHERE is_active = true;

CREATE INDEX IF NOT EXISTS idx_binding_profiles_app ON binding_profiles (application_id);
CREATE INDEX IF NOT EXISTS idx_binding_profiles_type ON binding_profiles (profile_type);

-- 2. BINDING PROFILE MAPPINGS TABLE
CREATE TABLE IF NOT EXISTS binding_profile_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id UUID NOT NULL REFERENCES binding_profiles(id) ON DELETE CASCADE,
    component_name VARCHAR(200) NOT NULL,
    host VARCHAR(300) NOT NULL,
    agent_id UUID NOT NULL REFERENCES agents(id),
    resolved_via VARCHAR(50) NOT NULL DEFAULT 'manual'
        CHECK (resolved_via IN ('exact_hostname', 'fqdn_suffix', 'ip', 'manual', 'pattern')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(profile_id, component_name)
);

CREATE INDEX IF NOT EXISTS idx_binding_profile_mappings_profile ON binding_profile_mappings (profile_id);
CREATE INDEX IF NOT EXISTS idx_binding_profile_mappings_agent ON binding_profile_mappings (agent_id);

-- 3. DR PATTERN RULES TABLE
CREATE TABLE IF NOT EXISTS dr_pattern_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    search_pattern VARCHAR(200) NOT NULL,
    replace_pattern VARCHAR(200) NOT NULL,
    priority INT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_dr_pattern_rules_org ON dr_pattern_rules (organization_id);

-- 4. AUTO-FAILOVER TRACKING TABLE
CREATE TABLE IF NOT EXISTS failover_health_status (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id UUID NOT NULL REFERENCES binding_profiles(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id),
    is_reachable BOOLEAN NOT NULL DEFAULT true,
    last_check_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    unreachable_since TIMESTAMPTZ,
    UNIQUE(profile_id, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_failover_health_profile ON failover_health_status (profile_id);
CREATE INDEX IF NOT EXISTS idx_failover_health_unreachable ON failover_health_status (profile_id, is_reachable);

-- 5. COMMENTS
COMMENT ON TABLE binding_profiles IS 'Named binding profiles for applications (prod, dr, bench). One active per app.';
COMMENT ON TABLE binding_profile_mappings IS 'Maps component names to agents for each binding profile';
COMMENT ON TABLE dr_pattern_rules IS 'Organization-wide rules for auto-suggesting DR agent mappings';
COMMENT ON TABLE failover_health_status IS 'Tracks agent reachability for auto-failover decisions';
