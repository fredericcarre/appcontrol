-- V050: Fix log_access_audit FKs to cascade on delete
--
-- Before this migration, log_access_audit.component_id and
-- log_access_audit.log_source_id had no ON DELETE policy (defaulted to NO
-- ACTION), so cascading deletes from applications → components → component
-- log sources would fail with "FOREIGN KEY constraint failed" as soon as
-- any log access had been recorded for that component.
--
-- Switching to ON DELETE SET NULL preserves the audit trail (DORA rule:
-- append-only audit) while allowing the parent component/log_source to
-- be deleted.

ALTER TABLE log_access_audit
    DROP CONSTRAINT IF EXISTS log_access_audit_component_id_fkey,
    DROP CONSTRAINT IF EXISTS log_access_audit_log_source_id_fkey;

ALTER TABLE log_access_audit
    ADD CONSTRAINT log_access_audit_component_id_fkey
        FOREIGN KEY (component_id) REFERENCES components(id) ON DELETE SET NULL,
    ADD CONSTRAINT log_access_audit_log_source_id_fkey
        FOREIGN KEY (log_source_id) REFERENCES component_log_sources(id) ON DELETE SET NULL;
