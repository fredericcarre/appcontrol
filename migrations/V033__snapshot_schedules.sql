-- Scheduled snapshots for discovery comparison over time

-- Snapshot schedules: defines when to capture discovery snapshots
CREATE TABLE snapshot_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    agent_ids UUID[] NOT NULL DEFAULT '{}',
    frequency VARCHAR(20) NOT NULL DEFAULT 'daily', -- hourly, daily, weekly, monthly
    cron_expression VARCHAR(100), -- optional, for custom schedules
    enabled BOOLEAN NOT NULL DEFAULT true,
    retention_days INTEGER NOT NULL DEFAULT 30,
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID REFERENCES users(id)
);

CREATE INDEX idx_snapshot_schedules_org ON snapshot_schedules(organization_id);
CREATE INDEX idx_snapshot_schedules_enabled ON snapshot_schedules(enabled) WHERE enabled = true;
CREATE INDEX idx_snapshot_schedules_next_run ON snapshot_schedules(next_run_at) WHERE enabled = true;

-- Scheduled snapshots: captures from scheduled runs
CREATE TABLE scheduled_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id UUID NOT NULL REFERENCES snapshot_schedules(id) ON DELETE CASCADE,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    agent_ids UUID[] NOT NULL DEFAULT '{}',
    report_ids UUID[] NOT NULL DEFAULT '{}', -- references to discovery_reports
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ, -- for auto-cleanup based on retention_days
    correlation_result JSONB -- cached correlation result for quick comparison
);

CREATE INDEX idx_scheduled_snapshots_schedule ON scheduled_snapshots(schedule_id);
CREATE INDEX idx_scheduled_snapshots_org ON scheduled_snapshots(organization_id);
CREATE INDEX idx_scheduled_snapshots_captured ON scheduled_snapshots(captured_at DESC);
CREATE INDEX idx_scheduled_snapshots_expires ON scheduled_snapshots(expires_at) WHERE expires_at IS NOT NULL;
