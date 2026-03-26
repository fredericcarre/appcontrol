-- V014: Command Execution Tracking (SQLite)

CREATE TABLE command_executions (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    component_id TEXT NOT NULL,
    agent_id TEXT,
    command_type TEXT NOT NULL DEFAULT 'custom',
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    duration_ms INTEGER,
    status TEXT NOT NULL DEFAULT 'dispatched',
    dispatched_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_command_executions_component ON command_executions(component_id, created_at);
CREATE INDEX idx_command_executions_request ON command_executions(request_id);
CREATE INDEX idx_command_executions_status ON command_executions(status) WHERE status = 'dispatched';
