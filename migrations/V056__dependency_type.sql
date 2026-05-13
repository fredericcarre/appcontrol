-- V056: Strong vs weak dependencies.
--
-- Until now every row in `dependencies` was treated as a strong
-- dependency: the DAG sequencer would not start a downstream component
-- before every upstream had reached RUNNING. A weak dependency is a
-- diagrammatic relationship: it shows up in the map and in reports
-- (useful for explaining the topology, e.g. "the batch reads from the
-- DB but starting the batch must not block on the DB"), but it does
-- NOT gate sequencing.
--
-- This migration adds a `dependency_type` column with a CHECK
-- constraint. Existing rows default to 'strong' so behaviour is
-- unchanged. The backend's DAG builder skips edges of type 'weak'.

ALTER TABLE dependencies
    ADD COLUMN dependency_type VARCHAR(10) NOT NULL DEFAULT 'strong'
        CHECK (dependency_type IN ('strong', 'weak'));

-- Index on the type so the DAG builder can filter cheaply when the
-- table grows. Most queries will continue to pull every edge for an
-- application, but reports that split strong vs weak benefit.
CREATE INDEX idx_dependencies_type ON dependencies (dependency_type);
