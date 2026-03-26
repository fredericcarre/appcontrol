-- V033: Scheduled snapshots for discovery comparison over time (SQLite)

-- Snapshot schedules: defines when to capture discovery snapshots
CREATE TABLE snapshot_schedules (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    agent_ids TEXT NOT NULL DEFAULT '[]',  -- JSON array of UUIDs
    frequency TEXT NOT NULL DEFAULT 'daily', -- hourly, daily, weekly, monthly
    cron_expression TEXT, -- optional, for custom schedules
    enabled INTEGER NOT NULL DEFAULT 1,
    retention_days INTEGER NOT NULL DEFAULT 30,
    last_run_at TEXT,
    next_run_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT REFERENCES users(id)
);

CREATE INDEX idx_snapshot_schedules_org ON snapshot_schedules(organization_id);
CREATE INDEX idx_snapshot_schedules_enabled ON snapshot_schedules(enabled);
CREATE INDEX idx_snapshot_schedules_next_run ON snapshot_schedules(next_run_at);

-- Scheduled snapshots: captures from scheduled runs
CREATE TABLE scheduled_snapshots (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES snapshot_schedules(id) ON DELETE CASCADE,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    agent_ids TEXT NOT NULL DEFAULT '[]',   -- JSON array of UUIDs
    report_ids TEXT NOT NULL DEFAULT '[]',  -- references to discovery_reports
    captured_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT, -- for auto-cleanup based on retention_days
    correlation_result TEXT -- cached correlation result for quick comparison (JSON)
);

CREATE INDEX idx_scheduled_snapshots_schedule ON scheduled_snapshots(schedule_id);
CREATE INDEX idx_scheduled_snapshots_org ON scheduled_snapshots(organization_id);
CREATE INDEX idx_scheduled_snapshots_captured ON scheduled_snapshots(captured_at);
CREATE INDEX idx_scheduled_snapshots_expires ON scheduled_snapshots(expires_at);
