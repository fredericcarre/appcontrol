-- V054 (SQLite): Manual task components.
-- See migrations/V054__manual_task_components.sql for the rationale.
-- SQLite differences:
--  * No DEFAULT gen_random_uuid() — DbUuid::new_v4() is bound on insert.
--  * TIMESTAMPTZ → TEXT (ISO-8601 via datetime('now')).
--  * Partial-index WHERE clause is supported on SQLite ≥ 3.8 (we ship newer).

ALTER TABLE components ADD COLUMN manual_description TEXT;

CREATE TABLE IF NOT EXISTS manual_task_validations (
    id              TEXT PRIMARY KEY,
    component_id    TEXT NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    application_id  TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    started_at      TEXT NOT NULL DEFAULT (datetime('now')),
    started_by      TEXT REFERENCES users(id),
    validated_at    TEXT,
    validated_by    TEXT REFERENCES users(id),
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'validated', 'skipped', 'failed')),
    comment         TEXT,
    duration_seconds INTEGER
);

CREATE INDEX IF NOT EXISTS idx_manual_task_validations_component
    ON manual_task_validations (component_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_manual_task_validations_pending
    ON manual_task_validations (component_id) WHERE status = 'pending';
