-- V056: Strong vs weak dependencies (SQLite mirror).
-- See migrations/V056__dependency_type.sql for the full rationale.

ALTER TABLE dependencies
    ADD COLUMN dependency_type TEXT NOT NULL DEFAULT 'strong'
        CHECK (dependency_type IN ('strong', 'weak'));

CREATE INDEX idx_dependencies_type ON dependencies (dependency_type);
