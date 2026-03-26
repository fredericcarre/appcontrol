-- V012: Add host field to components (SQLite)
-- Already included in base components table for SQLite

-- Index for fast host→agent resolution
CREATE INDEX IF NOT EXISTS idx_components_host ON components (host);
CREATE INDEX IF NOT EXISTS idx_agents_hostname ON agents (hostname);
