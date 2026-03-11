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

-- Update trigger for snapshot_schedules
CREATE OR REPLACE FUNCTION update_snapshot_schedule_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER snapshot_schedule_updated
    BEFORE UPDATE ON snapshot_schedules
    FOR EACH ROW
    EXECUTE FUNCTION update_snapshot_schedule_timestamp();

-- Function to calculate next run time based on frequency
CREATE OR REPLACE FUNCTION calculate_next_run(frequency VARCHAR, last_run TIMESTAMPTZ DEFAULT NOW())
RETURNS TIMESTAMPTZ AS $$
BEGIN
    RETURN CASE frequency
        WHEN 'hourly' THEN date_trunc('hour', last_run) + INTERVAL '1 hour'
        WHEN 'daily' THEN date_trunc('day', last_run) + INTERVAL '1 day'
        WHEN 'weekly' THEN date_trunc('week', last_run) + INTERVAL '1 week'
        WHEN 'monthly' THEN date_trunc('month', last_run) + INTERVAL '1 month'
        ELSE date_trunc('day', last_run) + INTERVAL '1 day'
    END;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Set next_run_at on insert
CREATE OR REPLACE FUNCTION set_initial_next_run()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.enabled AND NEW.next_run_at IS NULL THEN
        NEW.next_run_at = calculate_next_run(NEW.frequency, NOW());
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER snapshot_schedule_set_next_run
    BEFORE INSERT ON snapshot_schedules
    FOR EACH ROW
    EXECUTE FUNCTION set_initial_next_run();
