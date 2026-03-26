-- V040: Add version column to gateways table (SQLite)
-- Gateways report their version when they register with the backend

ALTER TABLE gateways ADD COLUMN version TEXT;
