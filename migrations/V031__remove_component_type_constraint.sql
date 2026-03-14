-- V031: Remove component_type CHECK constraint
-- Allow any string value for component_type to support flexible imports

ALTER TABLE components DROP CONSTRAINT IF EXISTS components_component_type_check;
