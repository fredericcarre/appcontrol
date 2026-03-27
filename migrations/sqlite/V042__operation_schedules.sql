-- V042: Operation Schedules for automated start/stop/restart (SQLite)
-- Allows scheduling start/stop/restart operations on applications or individual components

CREATE TABLE operation_schedules (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,

    -- Target: either application OR component (not both)
    application_id TEXT REFERENCES applications(id) ON DELETE CASCADE,
    component_id TEXT REFERENCES components(id) ON DELETE CASCADE,

    -- Schedule definition
    name TEXT NOT NULL,
    description TEXT,
    operation TEXT NOT NULL CHECK (operation IN ('start', 'stop', 'restart')),
    cron_expression TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'Europe/Paris',

    -- State
    is_enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    next_run_at TEXT,
    last_run_status TEXT CHECK (last_run_status IS NULL OR last_run_status IN ('success', 'failed', 'skipped')),
    last_run_message TEXT,
    last_action_log_id TEXT REFERENCES action_log(id),

    -- Audit
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_operation_schedules_next_run ON operation_schedules (next_run_at);
CREATE INDEX idx_operation_schedules_app ON operation_schedules (application_id);
CREATE INDEX idx_operation_schedules_component ON operation_schedules (component_id);
CREATE INDEX idx_operation_schedules_org ON operation_schedules (organization_id);

-- Execution history (APPEND-ONLY)
CREATE TABLE operation_schedule_executions (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES operation_schedules(id) ON DELETE CASCADE,
    action_log_id TEXT REFERENCES action_log(id),
    executed_at TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL CHECK (status IN ('success', 'failed', 'skipped')),
    message TEXT,
    duration_ms INTEGER
);

CREATE INDEX idx_operation_schedule_executions_schedule ON operation_schedule_executions (schedule_id, executed_at);
