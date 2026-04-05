-- V045: Actually remove component_type CHECK constraint (SQLite)
-- V031 was a no-op but the CHECK constraint from V004 is still active.
-- SQLite requires table recreation to remove CHECK constraints.
--
-- Strategy: rename old → create new → copy → drop old.
-- ALTER TABLE RENAME does not trigger FK cascades.

-- Step 1: Rename old table (no FK cascade triggered)
ALTER TABLE components RENAME TO components_old;

-- Step 2: Create new table without CHECK constraint on component_type
CREATE TABLE components (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    component_type TEXT NOT NULL,
    agent_id TEXT REFERENCES agents(id),
    check_cmd TEXT,
    start_cmd TEXT,
    stop_cmd TEXT,
    integrity_check_cmd TEXT,
    post_start_check_cmd TEXT,
    infra_check_cmd TEXT,
    rebuild_cmd TEXT,
    rebuild_infra_cmd TEXT,
    rebuild_agent_id TEXT REFERENCES agents(id),
    rebuild_protected INTEGER NOT NULL DEFAULT 0,
    check_interval_seconds INTEGER NOT NULL DEFAULT 30,
    start_timeout_seconds INTEGER NOT NULL DEFAULT 120,
    stop_timeout_seconds INTEGER NOT NULL DEFAULT 60,
    is_optional INTEGER NOT NULL DEFAULT 0,
    position_x REAL DEFAULT 0,
    position_y REAL DEFAULT 0,
    env_vars TEXT DEFAULT '{}',
    tags TEXT DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    host TEXT,
    current_state TEXT NOT NULL DEFAULT 'UNKNOWN',
    display_name TEXT,
    description TEXT,
    icon TEXT DEFAULT 'box',
    group_id TEXT,
    referenced_app_id TEXT REFERENCES applications(id) ON DELETE SET NULL,
    cluster_size INTEGER DEFAULT NULL,
    cluster_nodes TEXT DEFAULT NULL,
    log_capture_enabled INTEGER DEFAULT 1,
    log_buffer_lines INTEGER DEFAULT 10000,
    UNIQUE(application_id, name)
);

-- Step 3: Copy all data
INSERT INTO components SELECT
    id, application_id, name, component_type, agent_id,
    check_cmd, start_cmd, stop_cmd,
    integrity_check_cmd, post_start_check_cmd,
    infra_check_cmd,
    rebuild_cmd, rebuild_infra_cmd, rebuild_agent_id, rebuild_protected,
    check_interval_seconds, start_timeout_seconds, stop_timeout_seconds, is_optional,
    position_x, position_y,
    env_vars, tags, created_at, updated_at,
    host,
    current_state,
    display_name, description, icon,
    group_id,
    referenced_app_id,
    cluster_size, cluster_nodes,
    log_capture_enabled, log_buffer_lines
FROM components_old;

-- Step 4: Drop old table
DROP TABLE components_old;

-- Step 5: Recreate indexes
CREATE INDEX idx_components_app ON components (application_id);
CREATE INDEX idx_components_agent ON components (agent_id);
CREATE INDEX IF NOT EXISTS idx_components_host ON components (host);
CREATE INDEX IF NOT EXISTS idx_components_referenced_app ON components(referenced_app_id);
