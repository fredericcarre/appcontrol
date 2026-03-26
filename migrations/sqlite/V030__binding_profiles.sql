-- V030: Binding Profiles for Map Import with Gateway Resolution and DR Support (SQLite)

-- 1. BINDING PROFILES TABLE
CREATE TABLE IF NOT EXISTS binding_profiles (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    profile_type TEXT NOT NULL DEFAULT 'custom'
        CHECK (profile_type IN ('primary', 'dr', 'custom')),
    is_active INTEGER NOT NULL DEFAULT 0,
    gateway_ids TEXT NOT NULL DEFAULT '[]',  -- JSON array of UUIDs
    auto_failover INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT REFERENCES users(id),
    UNIQUE(application_id, name)
);

CREATE INDEX IF NOT EXISTS idx_binding_profiles_app ON binding_profiles (application_id);
CREATE INDEX IF NOT EXISTS idx_binding_profiles_type ON binding_profiles (profile_type);

-- 2. BINDING PROFILE MAPPINGS TABLE
CREATE TABLE IF NOT EXISTS binding_profile_mappings (
    id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL REFERENCES binding_profiles(id) ON DELETE CASCADE,
    component_name TEXT NOT NULL,
    host TEXT NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    resolved_via TEXT NOT NULL DEFAULT 'manual'
        CHECK (resolved_via IN ('exact_hostname', 'fqdn_suffix', 'ip', 'manual', 'pattern')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(profile_id, component_name)
);

CREATE INDEX IF NOT EXISTS idx_binding_profile_mappings_profile ON binding_profile_mappings (profile_id);
CREATE INDEX IF NOT EXISTS idx_binding_profile_mappings_agent ON binding_profile_mappings (agent_id);

-- 3. DR PATTERN RULES TABLE
CREATE TABLE IF NOT EXISTS dr_pattern_rules (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    search_pattern TEXT NOT NULL,
    replace_pattern TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, name)
);

CREATE INDEX IF NOT EXISTS idx_dr_pattern_rules_org ON dr_pattern_rules (organization_id);

-- 4. AUTO-FAILOVER TRACKING TABLE
CREATE TABLE IF NOT EXISTS failover_health_status (
    id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL REFERENCES binding_profiles(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    is_reachable INTEGER NOT NULL DEFAULT 1,
    last_check_at TEXT NOT NULL DEFAULT (datetime('now')),
    unreachable_since TEXT,
    UNIQUE(profile_id, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_failover_health_profile ON failover_health_status (profile_id);
CREATE INDEX IF NOT EXISTS idx_failover_health_unreachable ON failover_health_status (profile_id, is_reachable);
