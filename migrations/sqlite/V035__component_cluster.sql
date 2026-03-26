-- V035: Add cluster fields to components table (SQLite)
-- cluster_size: number of nodes in cluster (NULL = not a cluster)
-- cluster_nodes: JSON array of node hostnames/IPs

ALTER TABLE components ADD COLUMN cluster_size INTEGER DEFAULT NULL;
ALTER TABLE components ADD COLUMN cluster_nodes TEXT DEFAULT NULL;

-- Note: SQLite CHECK constraints added via ALTER TABLE are stored but
-- not enforced. For new tables, constraints work. Application code
-- should validate cluster_size >= 2 when set.
