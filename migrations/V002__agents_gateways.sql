-- V002: Agents and Gateways

CREATE TABLE gateways (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name VARCHAR(200) NOT NULL,
    zone VARCHAR(50) NOT NULL,
    hostname VARCHAR(300),
    port INTEGER DEFAULT 443,
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_heartbeat_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    hostname VARCHAR(300) NOT NULL,
    gateway_id UUID REFERENCES gateways(id),
    labels JSONB DEFAULT '{}',
    version VARCHAR(50),
    last_heartbeat_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_agents_org ON agents (organization_id);
CREATE INDEX idx_agents_gateway ON agents (gateway_id);
CREATE INDEX idx_gateways_org ON gateways (organization_id);
