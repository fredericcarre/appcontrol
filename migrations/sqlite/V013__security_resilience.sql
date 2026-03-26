-- V013: Security & Resilience Tables (SQLite)

-- Agent Identity & Certificate Tracking
ALTER TABLE agents ADD COLUMN certificate_fingerprint TEXT;
ALTER TABLE agents ADD COLUMN certificate_cn TEXT;
ALTER TABLE agents ADD COLUMN identity_verified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agents ADD COLUMN last_version TEXT;

-- Approval Workflows
CREATE TABLE approval_requests (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    operation_type TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    risk_level TEXT NOT NULL DEFAULT 'medium',
    requested_by TEXT NOT NULL REFERENCES users(id),
    request_payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    required_approvals INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    resolved_at TEXT,
    executed_at TEXT,
    execution_result TEXT
);

CREATE INDEX idx_approval_requests_org ON approval_requests(organization_id);
CREATE INDEX idx_approval_requests_status ON approval_requests(status) WHERE status = 'pending';
CREATE INDEX idx_approval_requests_resource ON approval_requests(resource_type, resource_id);

CREATE TABLE approval_decisions (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL REFERENCES approval_requests(id),
    decided_by TEXT NOT NULL REFERENCES users(id),
    decision TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_approval_decisions_request ON approval_decisions(request_id);

CREATE TABLE approval_policies (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    operation_type TEXT NOT NULL,
    risk_level TEXT NOT NULL DEFAULT 'high',
    required_approvals INTEGER NOT NULL DEFAULT 1,
    timeout_minutes INTEGER NOT NULL DEFAULT 15,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, operation_type)
);

-- Break-Glass Emergency Access
CREATE TABLE break_glass_accounts (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_rotated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(organization_id, username)
);

CREATE TABLE break_glass_sessions (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES break_glass_accounts(id),
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    activated_by_ip TEXT NOT NULL,
    reason TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    ended_at TEXT,
    actions_taken INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_break_glass_sessions_active ON break_glass_sessions(organization_id)
    WHERE ended_at IS NULL;

-- Agent Update Tasks
CREATE TABLE agent_update_tasks (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    agent_id TEXT NOT NULL REFERENCES agents(id),
    target_version TEXT NOT NULL,
    binary_url TEXT NOT NULL,
    checksum_sha256 TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    error_message TEXT
);

CREATE INDEX idx_agent_update_tasks_agent ON agent_update_tasks(agent_id);
CREATE INDEX idx_agent_update_tasks_status ON agent_update_tasks(status) WHERE status IN ('pending', 'downloading', 'installing');

-- Credential Vault References
ALTER TABLE app_variables ADD COLUMN vault_path TEXT;
ALTER TABLE app_variables ADD COLUMN vault_backend TEXT DEFAULT 'builtin';

-- Rate Limiting Configuration
ALTER TABLE organizations ADD COLUMN rate_limit_auth INTEGER DEFAULT 10;
ALTER TABLE organizations ADD COLUMN rate_limit_operations INTEGER DEFAULT 5;
ALTER TABLE organizations ADD COLUMN rate_limit_reads INTEGER DEFAULT 200;

-- Certificate Tracking
CREATE TABLE certificate_events (
    id TEXT PRIMARY KEY,
    agent_id TEXT REFERENCES agents(id),
    gateway_id TEXT,
    event_type TEXT NOT NULL,
    fingerprint TEXT,
    cn TEXT,
    issued_at TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_certificate_events_agent ON certificate_events(agent_id);
