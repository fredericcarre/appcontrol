-- V026: Add system info columns to agents table
-- These columns store static system information collected when the agent registers

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS os_name TEXT,
    ADD COLUMN IF NOT EXISTS os_version TEXT,
    ADD COLUMN IF NOT EXISTS cpu_arch TEXT,
    ADD COLUMN IF NOT EXISTS cpu_cores INTEGER,
    ADD COLUMN IF NOT EXISTS total_memory_mb BIGINT,
    ADD COLUMN IF NOT EXISTS disk_total_gb BIGINT;

-- Add comments for documentation
COMMENT ON COLUMN agents.os_name IS 'Operating system name (e.g., macOS, Linux, Windows)';
COMMENT ON COLUMN agents.os_version IS 'Operating system version';
COMMENT ON COLUMN agents.cpu_arch IS 'CPU architecture (e.g., x86_64, aarch64)';
COMMENT ON COLUMN agents.cpu_cores IS 'Number of CPU cores';
COMMENT ON COLUMN agents.total_memory_mb IS 'Total system memory in megabytes';
COMMENT ON COLUMN agents.disk_total_gb IS 'Total disk space in gigabytes (largest partition)';
