-- V017: Move Redis-backed features to PostgreSQL
--
-- 1. Token revocation blacklist (was Redis SET with TTL)
-- 2. Rate limiting counters (was Redis INCR with EXPIRE)
--
-- This eliminates Redis as a deployment dependency.

-- Token revocation: stores fingerprints of revoked JWT tokens.
-- Entries auto-expire via a periodic cleanup task (TTL column).
CREATE TABLE IF NOT EXISTS revoked_tokens (
    fingerprint TEXT PRIMARY KEY,
    revoked_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);

-- Index for cleanup query (delete expired entries)
CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expires ON revoked_tokens (expires_at);

-- Rate limiting: sliding window counters.
-- Key format: "auth:<ip>", "ops:<user_id>", "read:<user_id>"
CREATE TABLE IF NOT EXISTS rate_limit_counters (
    key         TEXT PRIMARY KEY,
    count       INTEGER NOT NULL DEFAULT 1,
    window_start TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for cleanup
CREATE INDEX IF NOT EXISTS idx_rate_limit_window ON rate_limit_counters (window_start);
