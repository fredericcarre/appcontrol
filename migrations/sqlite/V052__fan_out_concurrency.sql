-- V052 (SQLite): Fan-out start/stop concurrency policy.
-- See migrations/V052__fan_out_concurrency.sql for the rationale.
-- SQLite has no IF NOT EXISTS on ALTER TABLE ADD COLUMN, hence the simpler form.

ALTER TABLE components ADD COLUMN cluster_concurrency_mode TEXT NOT NULL DEFAULT 'parallel';
ALTER TABLE components ADD COLUMN cluster_batch_size INTEGER;
