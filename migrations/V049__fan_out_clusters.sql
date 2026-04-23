-- V049: Fan-out cluster support
--
-- Introduces a `fan_out` cluster mode where each member is a first-class,
-- independently monitored and operable entity (own agent, own commands, own FSM).
-- The existing `cluster_size` / `cluster_nodes` fields (V035) remain in use for
-- the `aggregate` mode (single FSM, external aggregation via the check_cmd).

-- Component-level cluster configuration
ALTER TABLE components
    ADD COLUMN cluster_mode VARCHAR(20) NOT NULL DEFAULT 'aggregate'
        CHECK (cluster_mode IN ('aggregate', 'fan_out')),
    ADD COLUMN cluster_health_policy VARCHAR(20) NOT NULL DEFAULT 'all_healthy'
        CHECK (cluster_health_policy IN ('all_healthy', 'any_healthy', 'quorum', 'threshold_pct')),
    ADD COLUMN cluster_min_healthy_pct SMALLINT NOT NULL DEFAULT 100
        CHECK (cluster_min_healthy_pct BETWEEN 1 AND 100);

COMMENT ON COLUMN components.cluster_mode IS 'aggregate = external aggregation (current behavior, cluster_size/cluster_nodes cosmetic). fan_out = each cluster_members row is a first-class monitored entity';
COMMENT ON COLUMN components.cluster_health_policy IS 'How the component FSM is derived from member states in fan_out mode';
COMMENT ON COLUMN components.cluster_min_healthy_pct IS 'For threshold_pct policy, minimum percentage of members that must be RUNNING for the component to be RUNNING';

-- First-class cluster members for fan_out mode
CREATE TABLE cluster_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id UUID NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    hostname VARCHAR(255) NOT NULL,
    agent_id UUID NOT NULL REFERENCES agents(id),
    site_id UUID REFERENCES sites(id),
    -- Per-member command overrides (NULL = inherit from component)
    check_cmd_override TEXT,
    start_cmd_override TEXT,
    stop_cmd_override TEXT,
    install_path TEXT,
    env_vars_override JSONB,
    -- Display & operations
    member_order INTEGER NOT NULL DEFAULT 0,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    tags JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(component_id, hostname, site_id)
);

CREATE INDEX idx_cluster_members_component ON cluster_members(component_id);
CREATE INDEX idx_cluster_members_agent ON cluster_members(agent_id);
CREATE INDEX idx_cluster_members_site ON cluster_members(site_id);

-- Per-member state cache (mirrors components.current_state for fan-out members)
CREATE TABLE cluster_member_state (
    cluster_member_id UUID PRIMARY KEY REFERENCES cluster_members(id) ON DELETE CASCADE,
    current_state VARCHAR(20) NOT NULL DEFAULT 'UNKNOWN',
    last_check_at TIMESTAMPTZ,
    last_check_exit_code SMALLINT,
    last_check_duration_ms INTEGER,
    last_stdout TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_cluster_member_state_state ON cluster_member_state(current_state);

-- Event tables gain an optional cluster_member_id for per-member attribution.
-- Nullable because non-fan_out components still emit events with NULL.
-- No FK: we preserve audit rows even if a member is deleted (APPEND-ONLY rule).
ALTER TABLE check_events      ADD COLUMN cluster_member_id UUID;
ALTER TABLE state_transitions ADD COLUMN cluster_member_id UUID;
ALTER TABLE action_log        ADD COLUMN cluster_member_id UUID;

CREATE INDEX idx_check_events_member ON check_events(cluster_member_id, created_at);
CREATE INDEX idx_state_transitions_member ON state_transitions(cluster_member_id, created_at);
CREATE INDEX idx_action_log_member ON action_log(cluster_member_id, created_at);
