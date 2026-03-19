-- Add operation result tracking to action_log
-- Allows tracking whether operations succeeded or failed, with error details

-- Add status and error columns
ALTER TABLE action_log
ADD COLUMN status VARCHAR(20) DEFAULT 'pending',
ADD COLUMN error_message TEXT,
ADD COLUMN completed_at TIMESTAMPTZ;

-- Add constraint for valid status values
ALTER TABLE action_log
ADD CONSTRAINT action_log_status_check
CHECK (status IN ('pending', 'in_progress', 'success', 'failed', 'cancelled'));

-- Index for filtering by status
CREATE INDEX idx_action_log_status ON action_log(status, created_at DESC);

-- Update existing rows to 'success' (legacy data assumed successful)
UPDATE action_log SET status = 'success', completed_at = created_at WHERE status = 'pending';

-- Comment
COMMENT ON COLUMN action_log.status IS 'Operation status: pending, in_progress, success, failed, cancelled';
COMMENT ON COLUMN action_log.error_message IS 'Error details when status is failed';
COMMENT ON COLUMN action_log.completed_at IS 'When the operation completed (success or failure)';
