-- V041: Add 'wizard' to binding_profile_mappings.resolved_via constraint (SQLite)
-- The import wizard uses 'wizard' as resolved_via value

-- SQLite doesn't support DROP CONSTRAINT / ADD CONSTRAINT on existing tables
-- We need to recreate the table to change the CHECK constraint
-- For now, rely on application validation since SQLite CHECK via ALTER is limited

-- The table was created in V030 with CHECK constraint, so we need to:
-- 1. Create a new temp table with updated CHECK
-- 2. Copy data
-- 3. Drop old table
-- 4. Rename temp table

CREATE TABLE binding_profile_mappings_new (
    id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL REFERENCES binding_profiles(id) ON DELETE CASCADE,
    component_name TEXT NOT NULL,
    host TEXT NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    resolved_via TEXT NOT NULL DEFAULT 'manual'
        CHECK (resolved_via IN ('exact_hostname', 'fqdn_suffix', 'ip', 'manual', 'pattern', 'wizard')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(profile_id, component_name)
);

INSERT INTO binding_profile_mappings_new
SELECT id, profile_id, component_name, host, agent_id, resolved_via, created_at
FROM binding_profile_mappings;

DROP TABLE binding_profile_mappings;

ALTER TABLE binding_profile_mappings_new RENAME TO binding_profile_mappings;

-- Recreate indexes
CREATE INDEX idx_binding_profile_mappings_profile ON binding_profile_mappings (profile_id);
CREATE INDEX idx_binding_profile_mappings_agent ON binding_profile_mappings (agent_id);
