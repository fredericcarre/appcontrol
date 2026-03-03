-- V027: Agent metrics time-series table for CPU/memory/disk monitoring
-- Stores heartbeat data for historical graphing

CREATE TABLE agent_metrics (
    id BIGINT GENERATED ALWAYS AS IDENTITY,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    cpu_pct REAL NOT NULL,
    memory_pct REAL NOT NULL,
    disk_used_pct REAL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_id, created_at)
) PARTITION BY RANGE (created_at);

-- Create partitions for current and next months
CREATE TABLE agent_metrics_y2026m03 PARTITION OF agent_metrics
    FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');
CREATE TABLE agent_metrics_y2026m04 PARTITION OF agent_metrics
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');
CREATE TABLE agent_metrics_y2026m05 PARTITION OF agent_metrics
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');

-- Index for efficient time-range queries per agent
CREATE INDEX idx_agent_metrics_agent_time ON agent_metrics (agent_id, created_at DESC);

-- Retention: auto-delete metrics older than 7 days (run via cron or backend job)
COMMENT ON TABLE agent_metrics IS 'Time-series heartbeat metrics for agent monitoring graphs. Partitioned by month, retain 7 days.';
