-- Persistent operation locks with heartbeat for reliable stuck detection
-- Replaces in-memory advisory locks with database-backed locks

CREATE TABLE operation_locks (
    app_id UUID PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
    operation VARCHAR(50) NOT NULL,  -- start, stop, restart, rebuild, switchover, etc.
    user_id UUID NOT NULL REFERENCES users(id),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(20) NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'cancelling', 'cancelled')),
    backend_instance VARCHAR(100),  -- hostname/pod identifier for debugging
    details JSONB DEFAULT '{}'
);

-- Index for finding stale locks
CREATE INDEX idx_operation_locks_heartbeat ON operation_locks (last_heartbeat);

-- Index for status queries
CREATE INDEX idx_operation_locks_status ON operation_locks (status) WHERE status != 'cancelled';

COMMENT ON TABLE operation_locks IS 'Tracks in-flight operations per application with heartbeat for stuck detection';
COMMENT ON COLUMN operation_locks.last_heartbeat IS 'Updated every 5s by the running operation. Stale if > 30s old.';
COMMENT ON COLUMN operation_locks.status IS 'running=active, cancelling=cancel requested, cancelled=acknowledged';
COMMENT ON COLUMN operation_locks.backend_instance IS 'Identifies which backend instance holds the lock (for debugging)';
