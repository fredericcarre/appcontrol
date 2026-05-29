-- V057 (SQLite): AI decisions — append-only audit trail for the AI layer (DORA).
-- See migrations/V057__ai_decisions.sql for the full rationale.
-- APPEND-ONLY: the application layer only ever INSERTs and SELECTs.

CREATE TABLE IF NOT EXISTS ai_decisions (
    id              TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    actor_user_id   TEXT REFERENCES users(id) ON DELETE SET NULL,
    kind            TEXT NOT NULL,
    model_provider  TEXT NOT NULL,
    model_name      TEXT NOT NULL,
    sensitivity     TEXT NOT NULL,
    routed_to       TEXT NOT NULL,
    prompt_hash     TEXT NOT NULL,
    context_summary TEXT,
    confidence      REAL,
    outcome         TEXT NOT NULL DEFAULT 'completed',
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_ai_decisions_org ON ai_decisions(organization_id, created_at);
CREATE INDEX IF NOT EXISTS idx_ai_decisions_kind ON ai_decisions(organization_id, kind);
