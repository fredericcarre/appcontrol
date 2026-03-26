-- V002: Agents and Gateways (SQLite)

CREATE TABLE gateways (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    zone TEXT NOT NULL,
    hostname TEXT,
    port INTEGER DEFAULT 443,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_heartbeat_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    hostname TEXT NOT NULL,
    gateway_id TEXT REFERENCES gateways(id),
    labels TEXT DEFAULT '{}',
    version TEXT,
    last_heartbeat_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_agents_org ON agents (organization_id);
CREATE INDEX idx_agents_gateway ON agents (gateway_id);
CREATE INDEX idx_gateways_org ON gateways (organization_id);
