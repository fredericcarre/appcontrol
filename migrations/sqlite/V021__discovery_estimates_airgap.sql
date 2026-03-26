-- V021: Discovery, Operation Estimates, Air-Gap Agent Updates (SQLite)
-- Features:
--   1. Passive topology discovery (agent → backend reports, inferred DAGs)
--   2. Operation time estimation (regular table, refreshed periodically)
--   3. Air-gap agent binary management (admin uploads, chunked push via WS)

-- ---------------------------------------------------------------------------
-- 1. Discovery: agent scan reports
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS discovery_reports (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    hostname TEXT NOT NULL,
    report TEXT NOT NULL,  -- JSON: full DiscoveryReport (processes, listeners, connections, services)
    scanned_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_discovery_reports_agent ON discovery_reports(agent_id);
CREATE INDEX IF NOT EXISTS idx_discovery_reports_created ON discovery_reports(created_at);

-- ---------------------------------------------------------------------------
-- 2. Discovery: inferred application drafts
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS discovery_drafts (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, applied, dismissed
    inferred_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_app_id TEXT REFERENCES applications(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS discovery_draft_components (
    id TEXT PRIMARY KEY,
    draft_id TEXT NOT NULL REFERENCES discovery_drafts(id) ON DELETE CASCADE,
    suggested_name TEXT NOT NULL,
    process_name TEXT,
    host TEXT,
    agent_id TEXT REFERENCES agents(id),
    listening_ports TEXT,  -- JSON array of integers
    component_type TEXT NOT NULL DEFAULT 'service',
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_draft_components_draft ON discovery_draft_components(draft_id);

CREATE TABLE IF NOT EXISTS discovery_draft_dependencies (
    id TEXT PRIMARY KEY,
    draft_id TEXT NOT NULL REFERENCES discovery_drafts(id) ON DELETE CASCADE,
    from_component TEXT NOT NULL REFERENCES discovery_draft_components(id) ON DELETE CASCADE,
    to_component TEXT NOT NULL REFERENCES discovery_draft_components(id) ON DELETE CASCADE,
    inferred_via TEXT NOT NULL DEFAULT 'tcp_connection'  -- tcp_connection, port_match, manual
);

CREATE INDEX IF NOT EXISTS idx_draft_deps_draft ON discovery_draft_dependencies(draft_id);

-- ---------------------------------------------------------------------------
-- 3. Operation time estimation: regular table (not materialized view)
-- ---------------------------------------------------------------------------

-- Compute per-component, per-operation-type statistics from successful executions.
-- Refreshed periodically by the backend.
CREATE TABLE IF NOT EXISTS component_operation_stats (
    component_id TEXT NOT NULL,
    command_type TEXT NOT NULL,
    sample_count INTEGER NOT NULL,
    avg_ms INTEGER,
    p50_ms INTEGER,
    p95_ms INTEGER,
    min_ms INTEGER,
    max_ms INTEGER,
    PRIMARY KEY (component_id, command_type)
);

-- ---------------------------------------------------------------------------
-- 4. Air-gap agent binary management
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS agent_binaries (
    id TEXT PRIMARY KEY,
    version TEXT NOT NULL UNIQUE,
    platform TEXT NOT NULL DEFAULT 'linux-amd64',
    checksum_sha256 TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    binary_data BLOB NOT NULL,
    uploaded_by TEXT REFERENCES users(id),
    uploaded_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_update_tasks (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    target_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, in_progress, complete, failed
    chunks_sent INTEGER NOT NULL DEFAULT 0,
    total_chunks INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_update_tasks_agent ON agent_update_tasks(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_update_tasks_status ON agent_update_tasks(status);
