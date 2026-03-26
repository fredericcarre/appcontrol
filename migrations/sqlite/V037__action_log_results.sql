-- V037: Add operation result tracking to action_log (SQLite)
-- Allows tracking whether operations succeeded or failed, with error details

-- Add status and error columns
ALTER TABLE action_log ADD COLUMN status TEXT DEFAULT 'pending';
ALTER TABLE action_log ADD COLUMN error_message TEXT;
ALTER TABLE action_log ADD COLUMN completed_at TEXT;

-- Note: SQLite CHECK constraint via ALTER TABLE is stored but not enforced
-- Application code should validate status IN ('pending', 'in_progress', 'success', 'failed', 'cancelled')

-- Index for filtering by status
CREATE INDEX idx_action_log_status ON action_log(status, created_at);

-- Update existing rows to 'success' (legacy data assumed successful)
UPDATE action_log SET status = 'success', completed_at = created_at WHERE status = 'pending';
