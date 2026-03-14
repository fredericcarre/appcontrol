-- Add cluster fields to components table
-- cluster_size: number of nodes in cluster (NULL = not a cluster)
-- cluster_nodes: JSON array of node hostnames/IPs

ALTER TABLE components
ADD COLUMN cluster_size INTEGER DEFAULT NULL,
ADD COLUMN cluster_nodes JSONB DEFAULT NULL;

COMMENT ON COLUMN components.cluster_size IS 'Number of nodes in cluster (NULL = not a cluster)';
COMMENT ON COLUMN components.cluster_nodes IS 'JSON array of node hostnames/IPs';

-- Add constraint to ensure cluster_size is at least 2 when set
ALTER TABLE components
ADD CONSTRAINT chk_cluster_size CHECK (cluster_size IS NULL OR cluster_size >= 2);
