-- V017: Move Redis-backed features to SQLite

-- Token revocation blacklist
CREATE TABLE revoked_tokens (
    fingerprint TEXT PRIMARY KEY,
    revoked_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX idx_revoked_tokens_expires ON revoked_tokens (expires_at);

-- Rate limiting counters
CREATE TABLE rate_limit_counters (
    key TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 1,
    window_start TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_rate_limit_window ON rate_limit_counters (window_start);
