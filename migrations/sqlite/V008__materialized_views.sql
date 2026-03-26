-- V008: Component Daily Stats Table (SQLite)
-- SQLite doesn't support materialized views, so we use a regular table
-- that gets refreshed periodically by the backend.

CREATE TABLE component_daily_stats (
    component_id TEXT NOT NULL,
    date TEXT NOT NULL,
    running_transitions INTEGER NOT NULL DEFAULT 0,
    failed_transitions INTEGER NOT NULL DEFAULT 0,
    stopped_transitions INTEGER NOT NULL DEFAULT 0,
    running_seconds INTEGER NOT NULL DEFAULT 0,
    total_seconds INTEGER NOT NULL DEFAULT 86400,
    total_transitions INTEGER NOT NULL DEFAULT 0,
    refreshed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (component_id, date)
);

CREATE INDEX idx_component_daily_stats_date ON component_daily_stats (date);
