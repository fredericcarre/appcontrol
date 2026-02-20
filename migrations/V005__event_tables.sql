-- V005: Event Tables (APPEND-ONLY — NO UPDATE, NO DELETE)
-- check_events, state_transitions, action_log, switchover_log, config_versions

-- check_events: PARTITIONED by month
CREATE TABLE check_events (
    id BIGINT GENERATED ALWAYS AS IDENTITY,
    component_id UUID NOT NULL,
    check_type VARCHAR(20) NOT NULL DEFAULT 'health'
        CHECK (check_type IN ('health', 'integrity', 'post_start', 'infrastructure')),
    exit_code SMALLINT NOT NULL,
    stdout TEXT,
    duration_ms INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
) PARTITION BY RANGE (created_at);

-- Create partitions for 2026
CREATE TABLE check_events_2026_01 PARTITION OF check_events FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
CREATE TABLE check_events_2026_02 PARTITION OF check_events FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');
CREATE TABLE check_events_2026_03 PARTITION OF check_events FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');
CREATE TABLE check_events_2026_04 PARTITION OF check_events FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');
CREATE TABLE check_events_2026_05 PARTITION OF check_events FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');
CREATE TABLE check_events_2026_06 PARTITION OF check_events FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');
CREATE TABLE check_events_2026_07 PARTITION OF check_events FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');
CREATE TABLE check_events_2026_08 PARTITION OF check_events FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');
CREATE TABLE check_events_2026_09 PARTITION OF check_events FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');
CREATE TABLE check_events_2026_10 PARTITION OF check_events FOR VALUES FROM ('2026-10-01') TO ('2026-11-01');
CREATE TABLE check_events_2026_11 PARTITION OF check_events FOR VALUES FROM ('2026-11-01') TO ('2026-12-01');
CREATE TABLE check_events_2026_12 PARTITION OF check_events FOR VALUES FROM ('2026-12-01') TO ('2027-01-01');

CREATE INDEX idx_check_events_component ON check_events (component_id, created_at);
CREATE INDEX idx_check_events_type ON check_events (check_type, created_at);

-- state_transitions: APPEND-ONLY
CREATE TABLE state_transitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    component_id UUID NOT NULL,
    from_state VARCHAR(20) NOT NULL,
    to_state VARCHAR(20) NOT NULL,
    trigger VARCHAR(50) NOT NULL DEFAULT 'check',
    details JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_state_transitions_component ON state_transitions (component_id, created_at);
CREATE INDEX idx_state_transitions_state ON state_transitions (to_state, created_at);

-- action_log: APPEND-ONLY (DORA audit trail)
CREATE TABLE action_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    action VARCHAR(100) NOT NULL,
    resource_type VARCHAR(50) NOT NULL,
    resource_id UUID NOT NULL,
    details JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_action_log_user ON action_log (user_id, created_at);
CREATE INDEX idx_action_log_resource ON action_log (resource_id, created_at);
CREATE INDEX idx_action_log_action ON action_log (action, created_at);

-- switchover_log: APPEND-ONLY
CREATE TABLE switchover_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    switchover_id UUID NOT NULL,
    application_id UUID NOT NULL,
    phase VARCHAR(20) NOT NULL
        CHECK (phase IN ('PREPARE','VALIDATE','STOP_SOURCE','SYNC','START_TARGET','COMMIT','ROLLBACK')),
    status VARCHAR(20) NOT NULL DEFAULT 'in_progress'
        CHECK (status IN ('in_progress','completed','failed','rolled_back')),
    details JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_switchover_log_app ON switchover_log (application_id, created_at);
CREATE INDEX idx_switchover_log_switchover ON switchover_log (switchover_id, created_at);

-- config_versions: APPEND-ONLY (snapshot before/after)
CREATE TABLE config_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_type VARCHAR(50) NOT NULL,
    resource_id UUID NOT NULL,
    changed_by UUID NOT NULL,
    before_snapshot JSONB,
    after_snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_config_versions_resource ON config_versions (resource_id, created_at);
