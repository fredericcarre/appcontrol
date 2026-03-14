-- Application suspension support
-- Allows pausing health checks for an entire application without deleting it

ALTER TABLE applications ADD COLUMN is_suspended BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE applications ADD COLUMN suspended_at TIMESTAMPTZ;
ALTER TABLE applications ADD COLUMN suspended_by UUID REFERENCES users(id);

-- Index for filtering active applications efficiently
CREATE INDEX idx_applications_suspended ON applications (organization_id) WHERE is_suspended = false;

-- Add comment explaining the feature
COMMENT ON COLUMN applications.is_suspended IS 'When true, agent stops health checks for all components in this application';
COMMENT ON COLUMN applications.suspended_at IS 'Timestamp when the application was suspended';
COMMENT ON COLUMN applications.suspended_by IS 'User who suspended the application';
