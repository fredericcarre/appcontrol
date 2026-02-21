-- V012: Add host field to components for user-facing host resolution
--
-- The user enters a host (FQDN or IP) when creating a component in the map.
-- The backend resolves this host to an agent_id by matching against
-- agents.hostname or agents.ip_addresses.
--
-- This decouples the user experience (they know hostnames/IPs) from the
-- internal agent UUID system. No multicast: 1 component → 1 host → 1 agent.

-- Add host field to components (what the user types: FQDN or IP)
ALTER TABLE components ADD COLUMN host VARCHAR(300);

-- Populate host from existing agent hostname where agent_id is set
UPDATE components c
SET host = a.hostname
FROM agents a
WHERE c.agent_id = a.id AND c.host IS NULL;

-- Index for fast host→agent resolution
CREATE INDEX idx_components_host ON components (host);
CREATE INDEX idx_agents_hostname ON agents (hostname);
