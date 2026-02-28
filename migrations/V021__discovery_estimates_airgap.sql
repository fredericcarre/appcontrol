-- V021: Discovery, Operation Estimates, Air-Gap Agent Updates
-- Features:
--   1. Passive topology discovery (agent → backend reports, inferred DAGs)
--   2. Operation time estimation (materialized stats from command_executions)
--   3. Air-gap agent binary management (admin uploads, chunked push via WS)

-- ---------------------------------------------------------------------------
-- 1. Discovery: agent scan reports
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS discovery_reports (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id     UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    hostname     TEXT NOT NULL,
    report       JSONB NOT NULL,  -- full DiscoveryReport (processes, listeners, connections, services)
    scanned_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_discovery_reports_agent ON discovery_reports(agent_id);
CREATE INDEX IF NOT EXISTS idx_discovery_reports_created ON discovery_reports(created_at DESC);

-- ---------------------------------------------------------------------------
-- 2. Discovery: inferred application drafts
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS discovery_drafts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending, applied, dismissed
    inferred_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    applied_app_id  UUID REFERENCES applications(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS discovery_draft_components (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id        UUID NOT NULL REFERENCES discovery_drafts(id) ON DELETE CASCADE,
    suggested_name  TEXT NOT NULL,
    process_name    TEXT,
    host            TEXT,
    agent_id        UUID REFERENCES agents(id),
    listening_ports INTEGER[],
    component_type  TEXT NOT NULL DEFAULT 'service',
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_draft_components_draft ON discovery_draft_components(draft_id);

CREATE TABLE IF NOT EXISTS discovery_draft_dependencies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id        UUID NOT NULL REFERENCES discovery_drafts(id) ON DELETE CASCADE,
    from_component  UUID NOT NULL REFERENCES discovery_draft_components(id) ON DELETE CASCADE,
    to_component    UUID NOT NULL REFERENCES discovery_draft_components(id) ON DELETE CASCADE,
    inferred_via    TEXT NOT NULL DEFAULT 'tcp_connection'  -- tcp_connection, port_match, manual
);

CREATE INDEX IF NOT EXISTS idx_draft_deps_draft ON discovery_draft_dependencies(draft_id);

-- ---------------------------------------------------------------------------
-- 3. Operation time estimation: materialized view over command_executions
-- ---------------------------------------------------------------------------

-- Compute per-component, per-operation-type statistics from successful executions.
-- Refreshed periodically by the backend (or via pg_cron).
CREATE MATERIALIZED VIEW IF NOT EXISTS component_operation_stats AS
SELECT
    ce.component_id,
    ce.command_type,
    COUNT(*)::INTEGER                                                    AS sample_count,
    AVG(ce.duration_ms)::INTEGER                                         AS avg_ms,
    PERCENTILE_CONT(0.5)  WITHIN GROUP (ORDER BY ce.duration_ms)::INTEGER AS p50_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY ce.duration_ms)::INTEGER AS p95_ms,
    MIN(ce.duration_ms)::INTEGER                                         AS min_ms,
    MAX(ce.duration_ms)::INTEGER                                         AS max_ms
FROM command_executions ce
WHERE ce.exit_code = 0
  AND ce.duration_ms IS NOT NULL
  AND ce.completed_at > now() - INTERVAL '90 days'
GROUP BY ce.component_id, ce.command_type;

CREATE UNIQUE INDEX IF NOT EXISTS idx_cos_component_type
    ON component_operation_stats(component_id, command_type);

-- ---------------------------------------------------------------------------
-- 4. Air-gap agent binary management
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS agent_binaries (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version         TEXT NOT NULL UNIQUE,
    platform        TEXT NOT NULL DEFAULT 'linux-amd64',
    checksum_sha256 TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL,
    binary_data     BYTEA NOT NULL,
    uploaded_by     UUID REFERENCES users(id),
    uploaded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS agent_update_tasks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id        UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    target_version  TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending, in_progress, complete, failed
    chunks_sent     INTEGER NOT NULL DEFAULT 0,
    total_chunks    INTEGER NOT NULL DEFAULT 0,
    error           TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_agent_update_tasks_agent ON agent_update_tasks(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_update_tasks_status ON agent_update_tasks(status);
