-- V056: Application activation level (graduated adoption ladder).
--
-- Materialises the 5-level adoption ladder described in the AppControl
-- methodology and vision documents. Each application carries an explicit
-- activation level that gates what operations the platform is allowed to
-- perform on it:
--
--   Level 0 — CAPTATION ONLY     Only reads from external referentials.
--                                No agent operations, no checks.
--   Level 1 — ADVISORY           Agents observe processes/ports/files and
--                                feed the map. No checks executed, no
--                                start/stop allowed.
--   Level 2 — ACTIVE DIAGNOSTIC  Health/integrity/infra checks run.
--                                FSM drives state. Still no start/stop.
--   Level 3 — OPS UNDER PR       start / stop / restart / rebuild allowed
--                                only when the calling client provides a
--                                merged-PR approval reference
--                                (X-PR-Approved-Sha header for REST,
--                                pr_approved_sha field for CLI/API).
--   Level 4 — DIRECT OPS         Operations allowed directly for users
--                                with the required RBAC permission.
--
-- Default is 4 for existing applications (no behaviour change for already
-- onboarded apps). New applications are created at level 1 by the
-- application-create handler so adoption starts on the safe side.

ALTER TABLE applications
    ADD COLUMN IF NOT EXISTS activation_level SMALLINT NOT NULL DEFAULT 4;

ALTER TABLE applications
    ADD CONSTRAINT applications_activation_level_range
    CHECK (activation_level BETWEEN 0 AND 4);

COMMENT ON COLUMN applications.activation_level IS
    '0=captation, 1=advisory, 2=diagnostic, 3=PR-only ops, 4=direct ops';

-- Index for filtering apps by activation level on dashboards.
CREATE INDEX IF NOT EXISTS idx_applications_activation_level
    ON applications(activation_level);
