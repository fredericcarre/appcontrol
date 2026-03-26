-- V038: Persistent operation locks with heartbeat for reliable stuck detection (SQLite)
-- Replaces in-memory advisory locks with database-backed locks

CREATE TABLE operation_locks (
    app_id TEXT PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
    operation TEXT NOT NULL,  -- start, stop, restart, rebuild, switchover, etc.
    user_id TEXT NOT NULL REFERENCES users(id),
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_heartbeat TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'cancelling', 'cancelled')),
    backend_instance TEXT,  -- hostname/pod identifier for debugging
    details TEXT DEFAULT '{}'  -- JSON
);

-- Index for finding stale locks
CREATE INDEX idx_operation_locks_heartbeat ON operation_locks (last_heartbeat);

-- Index for status queries
CREATE INDEX idx_operation_locks_status ON operation_locks (status);
