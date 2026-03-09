-- V032: Add metrics column to check_events for generic operational data
--
-- Check commands can return JSON to provide rich operational data:
-- - {"active_users": 12, "users": ["Alice", "Bob"]}
-- - {"queue_depth": 150, "consumers": 3}
-- - {"connections": 45, "replication_lag_ms": 10}
--
-- The frontend renders this generically without interpreting the schema.

ALTER TABLE check_events ADD COLUMN IF NOT EXISTS metrics JSONB;

-- Index for querying components with metrics
CREATE INDEX IF NOT EXISTS idx_check_events_metrics
    ON check_events (component_id, created_at DESC)
    WHERE metrics IS NOT NULL;

COMMENT ON COLUMN check_events.metrics IS 'Generic JSON metrics from check command stdout';
