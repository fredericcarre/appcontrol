-- V036: Application suspension support (SQLite)
-- Allows pausing health checks for an entire application without deleting it

ALTER TABLE applications ADD COLUMN is_suspended INTEGER NOT NULL DEFAULT 0;
ALTER TABLE applications ADD COLUMN suspended_at TEXT;
ALTER TABLE applications ADD COLUMN suspended_by TEXT REFERENCES users(id);

-- Index for filtering active applications efficiently
CREATE INDEX IF NOT EXISTS idx_applications_suspended ON applications (organization_id);
