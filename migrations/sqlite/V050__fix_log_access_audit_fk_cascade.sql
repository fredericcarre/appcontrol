-- V050: Fix log_access_audit FKs to cascade on delete (SQLite)
--
-- SQLite doesn't support ALTER TABLE to change FK constraints, so we
-- recreate the table with proper ON DELETE SET NULL. Data is preserved.
--
-- Before: component_id/log_source_id FK with no ON DELETE policy (NO ACTION)
--   → cascading delete from applications fails as soon as any log access
--     has been recorded.
-- After: ON DELETE SET NULL on both FKs — the audit trail survives (DORA
--   append-only rule) and the parent can be deleted.

-- 1. Rename the old table
ALTER TABLE log_access_audit RENAME TO log_access_audit_old;

-- 2. Create the new table with proper FK policies
CREATE TABLE log_access_audit (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    user_id TEXT REFERENCES users(id),
    component_id TEXT REFERENCES components(id) ON DELETE SET NULL,
    log_source_id TEXT REFERENCES component_log_sources(id) ON DELETE SET NULL,

    source_type TEXT NOT NULL,
    source_name TEXT,

    lines_requested INTEGER,
    filter_applied TEXT,
    time_range_hours INTEGER,

    accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
    ip_address TEXT,
    user_agent TEXT
);

-- 3. Copy data
INSERT INTO log_access_audit
    (id, organization_id, user_id, component_id, log_source_id,
     source_type, source_name, lines_requested, filter_applied,
     time_range_hours, accessed_at, ip_address, user_agent)
SELECT
     id, organization_id, user_id, component_id, log_source_id,
     source_type, source_name, lines_requested, filter_applied,
     time_range_hours, accessed_at, ip_address, user_agent
FROM log_access_audit_old;

-- 4. Drop the old table
DROP TABLE log_access_audit_old;

-- 5. Recreate the index
CREATE INDEX IF NOT EXISTS idx_log_audit_component ON log_access_audit(component_id);
