-- V057: AI decisions — append-only audit trail for the AI layer (DORA).
--
-- Every AI interaction (copilot chat, RCA, architect map, command fix,
-- remediation) records HOW it was produced: which model, where it was routed
-- (local vs frontier), the data sensitivity, and a hash of the exact prompt so
-- any decision is reproducible WITHOUT storing secrets.
--
-- APPEND-ONLY: like action_log / state_transitions, this table is NEVER updated
-- or deleted. The application layer only ever INSERTs and SELECTs.

CREATE TABLE ai_decisions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    -- NULL when the action was taken autonomously (no human in the loop).
    actor_user_id   UUID REFERENCES users(id) ON DELETE SET NULL,
    kind            VARCHAR(40) NOT NULL,   -- 'chat','rca','architect','command_fix','remediation'
    model_provider  VARCHAR(40) NOT NULL,   -- 'mock','local','frontier'
    model_name      VARCHAR(120) NOT NULL,
    sensitivity     VARCHAR(20) NOT NULL,   -- 'public','internal','sensitive','secret'
    routed_to       VARCHAR(20) NOT NULL,   -- 'local' | 'frontier'
    prompt_hash     CHAR(64) NOT NULL,      -- SHA-256 of the exact prompt
    context_summary JSONB,
    confidence      REAL,
    outcome         VARCHAR(20) NOT NULL DEFAULT 'completed',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_ai_decisions_org ON ai_decisions(organization_id, created_at);
CREATE INDEX idx_ai_decisions_kind ON ai_decisions(organization_id, kind);
