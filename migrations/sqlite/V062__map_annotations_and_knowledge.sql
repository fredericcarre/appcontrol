-- V062: Map annotations + knowledge progress (SQLite mirror).
-- See migrations/V062 for the full rationale.

CREATE TABLE map_annotations (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    target_type TEXT NOT NULL
        CHECK (target_type IN ('application', 'component', 'dependency')),
    target_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'note'
        CHECK (kind IN ('note', 'review', 'todo', 'warning')),
    body TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    author_id TEXT,
    resolved_at TIMESTAMP,
    resolved_by TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_map_annotations_target ON map_annotations (target_type, target_id);
CREATE INDEX idx_map_annotations_org ON map_annotations (organization_id);

ALTER TABLE components ADD COLUMN confidence_score REAL NOT NULL DEFAULT 0.5;
ALTER TABLE components ADD COLUMN knowledge_status TEXT NOT NULL DEFAULT 'draft'
    CHECK (knowledge_status IN ('candidate', 'draft', 'reviewed', 'validated', 'deprecated'));

ALTER TABLE dependencies ADD COLUMN confidence_score REAL NOT NULL DEFAULT 0.5;
ALTER TABLE dependencies ADD COLUMN knowledge_status TEXT NOT NULL DEFAULT 'draft'
    CHECK (knowledge_status IN ('candidate', 'draft', 'reviewed', 'validated', 'deprecated'));

CREATE INDEX idx_components_knowledge_status ON components (knowledge_status);
CREATE INDEX idx_dependencies_knowledge_status ON dependencies (knowledge_status);
