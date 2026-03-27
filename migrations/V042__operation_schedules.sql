-- V042: Operation Schedules for automated start/stop/restart
-- Allows scheduling start/stop/restart operations on applications or individual components
-- Use case: stop an app at night, schedule automatic restart every morning

CREATE TABLE operation_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,

    -- Target: either application OR component (not both)
    application_id UUID REFERENCES applications(id) ON DELETE CASCADE,
    component_id UUID REFERENCES components(id) ON DELETE CASCADE,

    -- Schedule definition
    name VARCHAR(200) NOT NULL,
    description TEXT,
    operation VARCHAR(20) NOT NULL CHECK (operation IN ('start', 'stop', 'restart')),
    cron_expression VARCHAR(100) NOT NULL,  -- e.g., '0 7 * * 1-5' = 7h Mon-Fri
    timezone VARCHAR(50) NOT NULL DEFAULT 'Europe/Paris',

    -- State
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ,
    last_run_status VARCHAR(20) CHECK (last_run_status IS NULL OR last_run_status IN ('success', 'failed', 'skipped')),
    last_run_message TEXT,
    last_action_log_id UUID REFERENCES action_log(id),

    -- Audit
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Constraint: must target app OR component, not both, and at least one
    CONSTRAINT operation_schedule_target_check CHECK (
        (application_id IS NOT NULL AND component_id IS NULL) OR
        (application_id IS NULL AND component_id IS NOT NULL)
    )
);

-- Index for the scheduler background job query: find enabled schedules due for execution
CREATE INDEX idx_operation_schedules_next_run ON operation_schedules (next_run_at)
    WHERE is_enabled = true AND next_run_at IS NOT NULL;

-- Indexes for listing schedules by target
CREATE INDEX idx_operation_schedules_app ON operation_schedules (application_id)
    WHERE application_id IS NOT NULL;
CREATE INDEX idx_operation_schedules_component ON operation_schedules (component_id)
    WHERE component_id IS NOT NULL;

-- Index for org-level queries
CREATE INDEX idx_operation_schedules_org ON operation_schedules (organization_id);

-- Execution history (APPEND-ONLY per CLAUDE.md rules)
CREATE TABLE operation_schedule_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id UUID NOT NULL REFERENCES operation_schedules(id) ON DELETE CASCADE,
    action_log_id UUID REFERENCES action_log(id),
    executed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    status VARCHAR(20) NOT NULL CHECK (status IN ('success', 'failed', 'skipped')),
    message TEXT,
    duration_ms INTEGER
);

CREATE INDEX idx_operation_schedule_executions_schedule ON operation_schedule_executions (schedule_id, executed_at DESC);
