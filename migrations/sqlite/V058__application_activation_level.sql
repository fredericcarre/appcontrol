-- V056: Application activation level (SQLite mirror).
-- See migrations/V056 for the full rationale.

ALTER TABLE applications ADD COLUMN activation_level INTEGER NOT NULL DEFAULT 4
    CHECK (activation_level BETWEEN 0 AND 4);

CREATE INDEX IF NOT EXISTS idx_applications_activation_level
    ON applications(activation_level);
