-- V014: Command Execution Tracking (APPEND-ONLY)
-- Records every command dispatched to agents and their results.
-- Provides full audit trail for start/stop/custom commands.

CREATE TABLE IF NOT EXISTS command_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    request_id UUID NOT NULL UNIQUE,
    component_id UUID NOT NULL,
    agent_id UUID,
    command_type VARCHAR(20) NOT NULL DEFAULT 'custom',  -- start, stop, custom
    exit_code SMALLINT,
    stdout TEXT,
    stderr TEXT,
    duration_ms INTEGER,
    status VARCHAR(20) NOT NULL DEFAULT 'dispatched',  -- dispatched, completed, failed, timeout
    dispatched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_command_executions_component ON command_executions(component_id, created_at);
CREATE INDEX idx_command_executions_request ON command_executions(request_id);
CREATE INDEX idx_command_executions_status ON command_executions(status) WHERE status = 'dispatched';
