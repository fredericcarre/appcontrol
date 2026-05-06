-- V054: Manual task components
--
-- Some operations are inherently outside AppControl's reach: "connect to F5
-- and disable the VIP", "ask the DBA to run the schema migration", "page
-- the on-call to confirm the DR drill is starting". A manual_task component
-- represents one of these — it pauses the DAG until an operator validates
-- it, and the validation (with a comment and timing) lands in the audit log.
--
-- We don't introduce a new component_type CHECK because V031 already
-- removed the constraint on component_type. The convention is that the
-- frontend sets `component_type = 'manual_task'`, which the sequencer
-- detects at runtime.
--
-- Storage:
--   * components.manual_description — markdown, what the operator should do.
--     Image and file links inside the markdown carry the screenshots /
--     attachments without us needing dedicated tables for v1.18.0.
--   * manual_task_validations — append-only history of validation attempts.
--     One pending row per "is the parent app currently being started" cycle;
--     past validations stick around for the audit log + DORA reports.

ALTER TABLE components
    ADD COLUMN IF NOT EXISTS manual_description TEXT;

CREATE TABLE IF NOT EXISTS manual_task_validations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id    UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    application_id  UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_by      UUID REFERENCES users(id),
    validated_at    TIMESTAMPTZ,
    validated_by    UUID REFERENCES users(id),
    -- 'pending' until an operator clicks Validate or Skip; then it stays
    -- in its terminal state and a new row is created on the next start.
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'validated', 'skipped', 'failed')),
    comment         TEXT,
    -- duration_seconds: validated_at - started_at, denormalised so DORA
    -- reports don't have to re-derive it.
    duration_seconds INTEGER
);

CREATE INDEX IF NOT EXISTS idx_manual_task_validations_component
    ON manual_task_validations (component_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_manual_task_validations_pending
    ON manual_task_validations (component_id) WHERE status = 'pending';
