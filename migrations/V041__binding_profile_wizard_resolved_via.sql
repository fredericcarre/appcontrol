-- V041: Add 'wizard' to binding_profile_mappings.resolved_via constraint
-- The import wizard uses 'wizard' as resolved_via value

ALTER TABLE binding_profile_mappings DROP CONSTRAINT IF EXISTS binding_profile_mappings_resolved_via_check;

ALTER TABLE binding_profile_mappings ADD CONSTRAINT binding_profile_mappings_resolved_via_check
  CHECK (resolved_via IN ('exact_hostname', 'fqdn_suffix', 'ip', 'manual', 'pattern', 'wizard'));
