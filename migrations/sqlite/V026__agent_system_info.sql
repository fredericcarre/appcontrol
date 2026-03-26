-- V026: Add system info columns to agents table (SQLite)
-- These columns store static system information collected when the agent registers

ALTER TABLE agents ADD COLUMN os_name TEXT;
ALTER TABLE agents ADD COLUMN os_version TEXT;
ALTER TABLE agents ADD COLUMN cpu_arch TEXT;
ALTER TABLE agents ADD COLUMN cpu_cores INTEGER;
ALTER TABLE agents ADD COLUMN total_memory_mb INTEGER;
ALTER TABLE agents ADD COLUMN disk_total_gb INTEGER;
