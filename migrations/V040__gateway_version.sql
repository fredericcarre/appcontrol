-- V040: Add version column to gateways table
-- Gateways report their version when they register with the backend

ALTER TABLE gateways ADD COLUMN IF NOT EXISTS version VARCHAR(50);

COMMENT ON COLUMN gateways.version IS 'Gateway software version, reported during registration';
