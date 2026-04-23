-- V049: Fan-out cluster support (SQLite)
--
-- Mirrors the PostgreSQL V049 migration. Adds fan_out cluster mode where each
-- member is first-class (own agent, own commands, own FSM state), while the
-- existing cluster_size/cluster_nodes (V035) remain for aggregate mode.

-- Component-level cluster configuration
-- SQLite: CHECK constraints on new ALTER TABLE ADD COLUMN are stored but not enforced.
-- Application code validates values.
ALTER TABLE components ADD COLUMN cluster_mode TEXT NOT NULL DEFAULT 'aggregate';
ALTER TABLE components ADD COLUMN cluster_health_policy TEXT NOT NULL DEFAULT 'all_healthy';
ALTER TABLE components ADD COLUMN cluster_min_healthy_pct INTEGER NOT NULL DEFAULT 100;

-- First-class cluster members for fan_out mode
CREATE TABLE cluster_members (
    id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    hostname TEXT NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    site_id TEXT REFERENCES sites(id),
    check_cmd_override TEXT,
    start_cmd_override TEXT,
    stop_cmd_override TEXT,
    install_path TEXT,
    env_vars_override TEXT,
    member_order INTEGER NOT NULL DEFAULT 0,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(component_id, hostname, site_id)
);

CREATE INDEX idx_cluster_members_component ON cluster_members(component_id);
CREATE INDEX idx_cluster_members_agent ON cluster_members(agent_id);
CREATE INDEX idx_cluster_members_site ON cluster_members(site_id);

-- Per-member state cache
CREATE TABLE cluster_member_state (
    cluster_member_id TEXT PRIMARY KEY REFERENCES cluster_members(id) ON DELETE CASCADE,
    current_state TEXT NOT NULL DEFAULT 'UNKNOWN',
    last_check_at TEXT,
    last_check_exit_code INTEGER,
    last_check_duration_ms INTEGER,
    last_stdout TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_cluster_member_state_state ON cluster_member_state(current_state);

-- Event tables: optional cluster_member_id (no FK — APPEND-ONLY preservation)
ALTER TABLE check_events ADD COLUMN cluster_member_id TEXT;
ALTER TABLE state_transitions ADD COLUMN cluster_member_id TEXT;
ALTER TABLE action_log ADD COLUMN cluster_member_id TEXT;

CREATE INDEX idx_check_events_member ON check_events(cluster_member_id, created_at);
CREATE INDEX idx_state_transitions_member ON state_transitions(cluster_member_id, created_at);
CREATE INDEX idx_action_log_member ON action_log(cluster_member_id, created_at);
