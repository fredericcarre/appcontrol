-- V052: Fan-out start/stop concurrency policy
--
-- Until now, `start_fan_out_component` / `stop_fan_out_component` dispatched
-- the per-member commands to every enabled member at once. That's fine for
-- a 6-node demo but stomps on downstream services (DB, auth, LB) when the
-- tier has 100-200 members coming up simultaneously.
--
-- Add two columns next to `cluster_mode` on `components`:
--
--   cluster_concurrency_mode TEXT
--     'parallel' (default) | 'batched'
--
--   cluster_batch_size INT
--     batch size when concurrency_mode = 'batched'. Ignored when 'parallel'.
--     NULL means "use the default of 10" — keeping it nullable lets the
--     backend pick a sensible default and lets us tune it later without a
--     schema change.

ALTER TABLE components
    ADD COLUMN IF NOT EXISTS cluster_concurrency_mode TEXT
        NOT NULL DEFAULT 'parallel'
        CHECK (cluster_concurrency_mode IN ('parallel', 'batched'));

ALTER TABLE components
    ADD COLUMN IF NOT EXISTS cluster_batch_size INT
        CHECK (cluster_batch_size IS NULL OR cluster_batch_size >= 1);
