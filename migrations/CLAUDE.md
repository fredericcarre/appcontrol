# CLAUDE.md - migrations/

## Purpose
PostgreSQL 16 schema managed by sqlx migrations. Each file is numbered VXX__description.sql.

## CRITICAL RULES
1. **APPEND-ONLY tables:** `check_events`, `state_transitions`, `action_log`, `switchover_log`, `config_versions` — NEVER include UPDATE or DELETE statements for these tables.
2. **Partitioning:** `check_events` MUST be partitioned by month (PARTITION BY RANGE on created_at).
3. **All UUIDs:** use `gen_random_uuid()` as default.
4. **All timestamps:** use `TIMESTAMPTZ` with `DEFAULT now()`.
5. **Indexes:** every foreign key used in queries needs an index. Every (entity_id, created_at) pair for time-series queries.

## Migration Order
```
V001__organizations_users.sql      # organizations, users
V002__agents_gateways.sql          # agents, gateways (agents referenced by components)
V003__sites_applications.sql       # sites, applications (with FK to sites)
V004__components_dependencies.sql  # components (all fields incl rebuild), dependencies, site_overrides, component_commands
V005__event_tables.sql             # check_events (partitioned), state_transitions, action_log, switchover_log, config_versions
V006__teams_permissions.sql        # workspaces, teams, team_members, app_permissions_users/teams, app_share_links, user_favorites, saved_views
V007__api_keys_notifications.sql   # api_keys, notification_preferences
V008__materialized_views.sql       # component_daily_stats + refresh indexes
V009__saml_oidc.sql                # SAML/OIDC columns, saml_group_mappings
V010__variables_groups_params.sql  # app_variables, component_groups, component_links, command_input_params
V011__agent_ip_workspace_access_heartbeat.sql  # agents.ip_addresses, workspace_sites, workspace_members, orgs.heartbeat_timeout_seconds
V012__component_host_field.sql                 # Add host field to components table
V013__security_resilience.sql                  # Security resilience tables (threat tracking, incident response)
V014__command_executions.sql                   # Command executions tracking table
V015__enrollment_tokens.sql                    # Agent enrollment tokens for secure onboarding
V016__fsm_cache_notifications_locks.sql        # PostgreSQL-based FSM state cache, notification queue, advisory locks
V017__remove_redis_dependency.sql              # Remove Redis dependency: PostgreSQL-based rate limiting and token revocation
V018__local_auth.sql                           # Local authentication support
V019__typed_params_output_streaming.sql        # Typed parameters and output streaming
V020__gateway_agent_status.sql                 # Gateway and agent status tracking
V021__discovery_estimates_airgap.sql           # Discovery estimates and air-gapped mode
V022__platform_admin_sites_gateways.sql        # Platform admin, sites and gateways
V023__discovery_enriched.sql                   # Enriched discovery data
V024__gateway_failover_zones.sql               # Gateway failover zones
V025__certificate_rotation.sql                 # Certificate rotation support
V026__agent_system_info.sql                    # Agent system information
V027__agent_metrics.sql                        # Agent metrics tracking
V030__binding_profiles.sql                     # Binding profiles for import wizard
V031__remove_component_type_constraint.sql     # Remove component_type CHECK constraint (flexible types)
```

## Complete Schema Reference

### V001: organizations, users
```sql
CREATE TABLE organizations (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(200) NOT NULL UNIQUE,
  slug VARCHAR(100) NOT NULL UNIQUE,
  settings JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  organization_id UUID NOT NULL REFERENCES organizations(id),
  external_id VARCHAR(500) NOT NULL,
  email VARCHAR(300) NOT NULL,
  display_name VARCHAR(200) NOT NULL,
  role VARCHAR(20) NOT NULL DEFAULT 'viewer'
    CHECK (role IN ('admin', 'operator', 'editor', 'viewer')),
  is_active BOOLEAN NOT NULL DEFAULT true,
  last_login_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(organization_id, external_id)
);
```

### V004: components (FULL — includes all v4.2 fields)
Note: V031 removes the CHECK constraint on component_type to allow flexible types.
```sql
CREATE TABLE components (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
  name VARCHAR(200) NOT NULL,
  component_type VARCHAR(50) NOT NULL,  -- No CHECK constraint: allows any type value
  agent_id UUID REFERENCES agents(id),
  -- Core commands
  check_cmd TEXT,
  start_cmd TEXT,
  stop_cmd TEXT,
  -- Advanced checks (v4)
  integrity_check_cmd TEXT,
  post_start_check_cmd TEXT,
  -- Infrastructure check (v4.2)
  infra_check_cmd TEXT,
  -- Rebuild commands (v4.2)
  rebuild_cmd TEXT,
  rebuild_infra_cmd TEXT,
  rebuild_agent_id UUID REFERENCES agents(id),
  rebuild_protected BOOLEAN NOT NULL DEFAULT false,
  -- Configuration
  check_interval_seconds INTEGER NOT NULL DEFAULT 30,
  start_timeout_seconds INTEGER NOT NULL DEFAULT 120,
  stop_timeout_seconds INTEGER NOT NULL DEFAULT 60,
  is_optional BOOLEAN NOT NULL DEFAULT false,
  -- Visual position (React Flow)
  position_x REAL DEFAULT 0,
  position_y REAL DEFAULT 0,
  -- Metadata
  env_vars JSONB DEFAULT '{}',
  tags JSONB DEFAULT '[]',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(application_id, name)
);
```

### V005: check_events (PARTITIONED)
```sql
CREATE TABLE check_events (
  id BIGINT GENERATED ALWAYS AS IDENTITY,
  component_id UUID NOT NULL,
  check_type VARCHAR(20) NOT NULL DEFAULT 'health'
    CHECK (check_type IN ('health', 'integrity', 'post_start', 'infrastructure')),
  exit_code SMALLINT NOT NULL,
  stdout TEXT,
  duration_ms INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
) PARTITION BY RANGE (created_at);

-- Create initial partitions (backend should auto-create future partitions)
CREATE TABLE check_events_2026_01 PARTITION OF check_events FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
CREATE TABLE check_events_2026_02 PARTITION OF check_events FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');
CREATE TABLE check_events_2026_03 PARTITION OF check_events FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');

CREATE INDEX idx_check_events_component ON check_events (component_id, created_at);
```

### V006: permissions (see specs v4.1 for full SQL of all permission tables)
Key tables: `app_permissions_users`, `app_permissions_teams`, `app_share_links` — all with permission_level CHECK constraint.

## Validation
Run `sqlx migrate run` against a clean PostgreSQL 16 instance. All migrations must succeed in order.
