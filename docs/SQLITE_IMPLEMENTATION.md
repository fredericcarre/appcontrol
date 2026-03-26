# SQLite Dual-Mode Backend Implementation

## Status: Phase 2 Complete, Phase 3 In Progress

This document tracks the implementation of SQLite support for AppControl's portable Windows deployment.

## Completed

### Phase 1: Infrastructure (Complete)

#### 1. Cargo.toml Updates
- Added feature flags: `postgres` (default), `sqlite`, `full`
- SQLx configured with `any` driver for runtime database selection
- Conditional compilation via features

#### 2. Database Type Configuration (`config.rs`)
- Added `DatabaseType` enum (Postgres, Sqlite)
- `DATABASE_TYPE` environment variable support
- `SQLITE_PATH` for SQLite database file location
- Automatic URL generation for SQLite mode

#### 3. Database Abstraction Layer (`db.rs`)
- `DbPool` type alias resolves to `sqlx::PgPool` or `sqlx::SqlitePool` based on feature
- `create_pool()` handles both database types
- SQLite-specific configuration: WAL mode, busy timeout, foreign keys
- `sql::` module with dialect-specific helpers:
  - `now()`, `gen_uuid()`, `json_extract()`, `array_contains()`
  - `count_filter()`, `ilike()`, `bool_true()`, `bool_false()`
  - `add_interval()`, `sub_interval()`, `in_clause()`
- `UuidArray` wrapper type for cross-database array handling
- `IntArray` wrapper type for integer arrays
- Helper functions: `delete_by_ids()`, `update_by_ids()`, `select_ids_by_ids()`

#### 4. AppState Updates (`lib.rs`)
- Changed `db: sqlx::PgPool` to `db: crate::db::DbPool`

#### 5. Main.rs Updates
- Conditional partition creation (PostgreSQL only)
- Conditional partition maintenance task
- Database-specific migration paths (`migrations/postgres/` or `migrations/sqlite/`)
- Database-specific `_migrations` table schema
- Separate data retention implementations for each database

#### 6. PgPool References Updated
- Replaced all 33 files using `sqlx::PgPool` with `crate::db::DbPool`

### Phase 2: SQLite Migrations (Complete)

Created all 31 SQLite migrations in `migrations/sqlite/`:

| Migration | Description |
|-----------|-------------|
| V001 | organizations, users |
| V002 | agents, gateways |
| V003 | sites, applications |
| V004 | components, dependencies, site_overrides, component_commands |
| V005 | check_events, state_transitions, action_log, switchover_log, config_versions |
| V006 | workspaces, teams, team_members, permissions, share_links, favorites, views |
| V007 | api_keys, notification_preferences |
| V008 | component_daily_stats (regular table, not materialized view) |
| V009 | SAML/OIDC columns, saml_group_mappings |
| V010 | app_variables, component_groups, component_links, command_input_params |
| V011 | agent IP addresses, workspace_sites, workspace_members, heartbeat timeout |
| V012 | components.host field |
| V013 | Security resilience tables |
| V014 | Command executions tracking |
| V015 | Enrollment tokens, CA storage |
| V016 | FSM state cache, webhook notifications |
| V017 | Token revocation, rate limiting (Redis removal) |
| V018 | Local authentication support |
| V019 | Typed parameters, output streaming |
| V020 | Gateway/agent status management |
| V021 | Discovery, operation estimates, air-gap updates |
| V022 | Platform admin, gateway-site binding, certificate revocation |
| V023 | Enriched discovery |
| V024 | Gateway failover zones |
| V025 | Certificate rotation support |
| V026 | Agent system info |
| V027 | Agent metrics time-series |
| V030 | Binding profiles |
| V031 | Remove component_type constraint |
| V032 | Check event metrics |
| V033 | Snapshot schedules |
| V034 | Application references |
| V035 | Component cluster support |
| V036 | Application suspension |
| V037 | Action log results |
| V038 | Operation locks |
| V039 | Zone to site migration |
| V040 | Gateway version |
| V041 | Binding profile wizard resolved_via |

### SQLite Schema Adaptations

| PostgreSQL | SQLite |
|------------|--------|
| `UUID` | `TEXT` (36 chars) |
| `TIMESTAMPTZ` | `TEXT` (ISO8601) |
| `JSONB` | `TEXT` (JSON string) |
| `BOOLEAN` | `INTEGER` (0/1) |
| `gen_random_uuid()` | App-generated UUID |
| `BIGINT IDENTITY` | `INTEGER PRIMARY KEY AUTOINCREMENT` |
| Table partitioning | Single table with indexes |
| `now()` | `datetime('now')` |
| `UUID[]` | `TEXT` (JSON array) |
| `INTEGER[]` | `TEXT` (JSON array) |
| `MATERIALIZED VIEW` | Regular table with refresh |
| `COMMENT ON` | Not supported (omitted) |

### Phase 3: Query Adaptations (Complete)

#### Files Fixed
- `api/profiles.rs` - UuidArray for gateway_ids
- `api/agents.rs` - Bulk delete with conditional queries
- `api/apps.rs` - Referenced app status queries
- `api/import_wizard.rs` - UuidArray for profile gateway_ids
- `core/diagnostic.rs` - Latest check queries
- `core/heartbeat_batcher.rs` - Batch update queries
- `core/fsm.rs` - Component state queries
- `websocket/mod.rs` - Agent lookup queries
- `api/discovery.rs` - UuidArray/IntArray for schedule/snapshot arrays, helper functions
- `api/history.rs` - Helper functions for state transitions, component actions, command executions
- `core/snapshot_scheduler.rs` - UuidArray for agent/report arrays, helper functions
- `core/resolution.rs` - Helper functions for agent resolution queries

#### Query Patterns Addressed

1. **Array binding** (`Vec<Uuid>`, `Vec<i32>`):
   - Use `UuidArray` and `IntArray` wrapper types in FromRow structs
   - Use conditional helper functions for queries

2. **ANY($1)** clause:
   - PostgreSQL: `WHERE id = ANY($1)` with Vec<Uuid>
   - SQLite: `WHERE id IN ($1, $2, ...)` with individual binds via helper functions

3. **FILTER clause**: `COUNT(*) FILTER (WHERE x)`
   - PostgreSQL: Native FILTER clause
   - SQLite: `SUM(CASE WHEN x THEN 1 ELSE 0 END)`

4. **JSON operators**: `details->>'key'`
   - PostgreSQL: Native JSON access
   - SQLite: `json_extract(details, '$.key')`

5. **Array columns in SELECT**:
   - PostgreSQL: Native UUID[] arrays
   - SQLite: JSON TEXT decoded via UuidArray/IntArray Decode impls

## Build Status

```bash
# PostgreSQL build (default)
cargo build --package appcontrol-backend --features postgres  # ✓ SUCCESS
cargo test --package appcontrol-backend --features postgres   # ✓ 87 tests pass

# SQLite build (complete)
cargo build --package appcontrol-backend --no-default-features --features sqlite  # ✓ SUCCESS
```

### Phase 4: UUID/Timestamp Handling (Complete)
- All INSERT statements provide UUIDs via `Uuid::new_v4()` in Rust
- Timestamps stored as ISO8601 strings (RFC3339 format)
- SQLite migrations use TEXT type for both UUID and TIMESTAMPTZ columns

### Phase 5: Locking and Concurrency (Complete)
- FSM transition queries use conditional compilation:
  - PostgreSQL: `FOR UPDATE OF c` clause for row-level locking
  - SQLite: WAL mode provides serializable isolation
- Helper functions: `fetch_component_for_transition()`, `update_component_state()`
- SQLite uses `datetime('now')` instead of `now()` for timestamps

### Phase 6: Partition Handling (Complete)
- `run_data_retention()` has separate implementations:
  - PostgreSQL: DROP PARTITION for check_events
  - SQLite: Simple DELETE queries
- SQLite-specific function already existed in main.rs

### Phase 7: Materialized View Replacement (Complete)
- SQLite migrations use regular tables instead of materialized views
- `component_daily_stats` as regular table with manual refresh

### Phase 8: Testing Infrastructure (Complete)
- Added `sqlite-build` job to CI workflow
- Builds and tests backend with SQLite feature
- Parallel execution with PostgreSQL tests

### Phase 9: PowerShell Deployment Script (Complete)
- Updated `deploy-standalone.ps1` with SQLite mode (`-DbMode sqlite`)
- SQLite mode is now the default (zero dependencies)
- Simplified start/stop scripts for SQLite (no PostgreSQL management)
- Environment variables: `DATABASE_TYPE=sqlite`, `SQLITE_PATH=<path>`

## Implementation Complete

All phases are now complete. The backend can be built and run with either database:

```bash
# PostgreSQL build (default, full features)
cargo build --package appcontrol-backend --features postgres

# SQLite build (portable, zero dependencies)
cargo build --package appcontrol-backend --no-default-features --features sqlite
```

### Windows Portable Deployment

```powershell
# Download and deploy with SQLite (recommended)
.\deploy-standalone.ps1 -DbMode sqlite

# Or with embedded PostgreSQL
.\deploy-standalone.ps1 -DbMode embedded
```

## Configuration

### PostgreSQL Mode (Default)
```bash
export DATABASE_TYPE=postgres
export DATABASE_URL=postgresql://user:pass@localhost:5432/appcontrol
```

### SQLite Mode
```bash
export DATABASE_TYPE=sqlite
export SQLITE_PATH=./appcontrol.db
# Or with explicit URL:
export DATABASE_URL=sqlite:./appcontrol.db
```

## Key Files

- `crates/backend/Cargo.toml` - Feature flags
- `crates/backend/src/config.rs` - DatabaseType enum
- `crates/backend/src/db.rs` - Pool abstraction + SQL helpers + UuidArray/IntArray
- `crates/backend/src/lib.rs` - AppState type change
- `crates/backend/src/main.rs` - Conditional logic
- `migrations/sqlite/V001-V041.sql` - Complete SQLite schema
