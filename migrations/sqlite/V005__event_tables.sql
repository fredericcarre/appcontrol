-- V005: Event Tables (APPEND-ONLY — NO UPDATE, NO DELETE) (SQLite)
-- check_events, state_transitions, action_log, switchover_log, config_versions
--
-- SQLite Adaptations:
-- - No partitioning (single table with index on created_at for efficient queries)
-- - BIGINT IDENTITY -> INTEGER PRIMARY KEY AUTOINCREMENT
-- - Data retention handled by simple DELETE instead of partition drop

-- check_events: No partitioning in SQLite, use index on created_at
CREATE TABLE check_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    component_id TEXT NOT NULL,
    check_type TEXT NOT NULL DEFAULT 'health'
        CHECK (check_type IN ('health', 'integrity', 'post_start', 'infrastructure')),
    exit_code INTEGER NOT NULL,
    stdout TEXT,
    duration_ms INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_check_events_component ON check_events (component_id, created_at);
CREATE INDEX idx_check_events_type ON check_events (check_type, created_at);
CREATE INDEX idx_check_events_created ON check_events (created_at);

-- state_transitions: APPEND-ONLY
CREATE TABLE state_transitions (
    id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL,
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    trigger TEXT NOT NULL DEFAULT 'check',
    details TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_state_transitions_component ON state_transitions (component_id, created_at);
CREATE INDEX idx_state_transitions_state ON state_transitions (to_state, created_at);

-- action_log: APPEND-ONLY (DORA audit trail)
CREATE TABLE action_log (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    details TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_action_log_user ON action_log (user_id, created_at);
CREATE INDEX idx_action_log_resource ON action_log (resource_id, created_at);
CREATE INDEX idx_action_log_action ON action_log (action, created_at);

-- switchover_log: APPEND-ONLY
CREATE TABLE switchover_log (
    id TEXT PRIMARY KEY,
    switchover_id TEXT NOT NULL,
    application_id TEXT NOT NULL,
    phase TEXT NOT NULL
        CHECK (phase IN ('PREPARE','VALIDATE','STOP_SOURCE','SYNC','START_TARGET','COMMIT','ROLLBACK')),
    status TEXT NOT NULL DEFAULT 'in_progress'
        CHECK (status IN ('in_progress','completed','failed','rolled_back')),
    details TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_switchover_log_app ON switchover_log (application_id, created_at);
CREATE INDEX idx_switchover_log_switchover ON switchover_log (switchover_id, created_at);

-- config_versions: APPEND-ONLY (snapshot before/after)
CREATE TABLE config_versions (
    id TEXT PRIMARY KEY,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    changed_by TEXT NOT NULL,
    before_snapshot TEXT,
    after_snapshot TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_config_versions_resource ON config_versions (resource_id, created_at);
