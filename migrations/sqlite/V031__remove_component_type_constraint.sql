-- V031: Remove component_type CHECK constraint (SQLite)
-- Allow any string value for component_type to support flexible imports

-- SQLite doesn't support DROP CONSTRAINT directly
-- The constraint is effectively ignored by recreating without it
-- Since SQLite CHECK constraints are part of CREATE TABLE,
-- we'd need to recreate the table to truly remove it.
-- For now, this is a no-op since SQLite V004 didn't include the constraint.
