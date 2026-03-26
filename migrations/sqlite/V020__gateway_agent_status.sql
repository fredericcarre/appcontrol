-- V020: Gateway and Agent status management (SQLite)
-- Adds explicit status column for suspend/activate/delete operations

-- Add status column to gateways
-- status: 'active' (normal), 'suspended' (temporarily disabled), 'deleted' (soft deleted)
ALTER TABLE gateways ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
  CHECK (status IN ('active', 'suspended', 'deleted'));

-- Add status column to agents
ALTER TABLE agents ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
  CHECK (status IN ('active', 'suspended', 'deleted'));

-- Create indexes for status filtering
CREATE INDEX idx_gateways_status ON gateways (organization_id, status);
CREATE INDEX idx_agents_status ON agents (organization_id, status);

-- Migrate existing is_active data
UPDATE gateways SET status = CASE WHEN is_active THEN 'active' ELSE 'deleted' END;
UPDATE agents SET status = CASE WHEN is_active THEN 'active' ELSE 'deleted' END;
