-- V027: Agent metrics time-series table for CPU/memory/disk monitoring (SQLite)
-- Stores heartbeat data for historical graphing

-- SQLite does not support table partitioning, use regular table with indexes
CREATE TABLE agent_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    cpu_pct REAL NOT NULL,
    memory_pct REAL NOT NULL,
    disk_used_pct REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for efficient time-range queries per agent
CREATE INDEX idx_agent_metrics_agent_time ON agent_metrics (agent_id, created_at);

-- Retention: auto-delete metrics older than 7 days (run via backend job)
