# PROGRESS.md - AppControl v4 Implementation Tracker

> **Instructions for Claude Code:** Read this file at the start of every session. Pick the next unchecked `[ ]` task. After completing work, update this file by checking off `[x]` what you did.

## Phase 0: Foundation (Week 1-2)

### P0-1: Common Types & Protocol
- [x] `crates/common/Cargo.toml` — dependencies: serde, serde_json, uuid, chrono, thiserror, tokio, bincode
- [x] `crates/common/src/types.rs` — ComponentState enum (8 states), CheckResult, CommandResult, Permission levels
- [x] `crates/common/src/fsm.rs` — FSM transition validation (valid_transition(from, to) -> bool)
- [x] `crates/common/src/protocol.rs` — Agent<->Backend messages (heartbeat, check_result, command_result, execute_command, update_config)
- [x] `crates/common/src/pki.rs` — mTLS certificate generation and validation utilities
- [x] `crates/common/src/lib.rs` — re-export all public types
- [x] Tests: ≥20 unit tests covering all FSM transitions (valid + invalid)

### P0-2: Database Migrations
- [x] `migrations/V001__organizations_users.sql` — organizations, users tables
- [x] `migrations/V002__agents_gateways.sql` — agents, gateways tables
- [x] `migrations/V003__sites_applications.sql` — sites, applications tables
- [x] `migrations/V004__components_dependencies.sql` — components (with all check/rebuild fields), dependencies, site_overrides, component_commands
- [x] `migrations/V005__event_tables.sql` — check_events (PARTITIONED), state_transitions, action_log, switchover_log, config_versions
- [x] `migrations/V006__teams_permissions.sql` — workspaces, teams, team_members, app_permissions_users, app_permissions_teams, app_share_links, user_favorites, saved_views
- [x] `migrations/V007__api_keys_notifications.sql` — api_keys, notification_preferences
- [x] `migrations/V008__materialized_views.sql` — component_daily_stats, indexes
- [x] `migrations/V009__saml_oidc.sql` — SAML/OIDC columns (oidc_sub, saml_name_id), saml_group_mappings table
- [ ] Validation: `sqlx migrate run` succeeds on clean PostgreSQL 16

### P0-3: Backend API Core
- [x] `crates/backend/Cargo.toml` — axum, tokio, sqlx, serde, tracing, tower-http, jsonwebtoken, reqwest, base64, flate2, urlencoding
- [x] `crates/backend/src/main.rs` — Axum server setup, router, middleware stack
- [x] `crates/backend/src/db.rs` — PostgreSQL pool + Redis connection
- [x] `crates/backend/src/auth/` — JWT RS256 validation, OIDC flow, SAML 2.0 SP-SSO, API key auth
- [x] `crates/backend/src/middleware/` — auth middleware, permission check middleware, request logging
- [x] `crates/backend/src/api/apps.rs` — CRUD applications (GET, POST, PUT, DELETE /apps)
- [x] `crates/backend/src/api/components.rs` — CRUD components + dependencies
- [x] `crates/backend/src/api/health.rs` — GET /health, GET /ready
- [x] Tests: API tests with test database, ≥15 tests

### P0-4: Agent Core
- [x] `crates/agent/Cargo.toml` — tokio, serde, sysinfo, nix, sled, tungstenite, tracing, reqwest
- [x] `crates/agent/src/main.rs` — CLI args, config loading, agent startup
- [x] `crates/agent/src/config.rs` — YAML config loader (gateway_url, tls, labels)
- [x] `crates/agent/src/connection.rs` — WebSocket client to gateway/backend, reconnection logic
- [x] `crates/agent/src/executor.rs` — **CRITICAL:** Process execution with double-fork + setsid detachment
- [x] `crates/agent/src/scheduler.rs` — Local check scheduler (tokio interval, configurable per component)
- [x] `crates/agent/src/buffer.rs` — Offline buffer (sled DB, 100MB FIFO, replay on reconnect)
- [x] `crates/agent/src/native_commands.rs` — Built-in: disk_space, memory, cpu, process, tcp_port, http
- [x] Tests: ≥10 tests, including process detachment test (start process, kill agent, verify process lives)

### P0-5: FSM Engine & DAG Sequencing
- [x] `crates/backend/src/core/fsm.rs` — State machine engine, validate + execute transitions, write to state_transitions
- [x] `crates/backend/src/core/dag.rs` — DAG builder from dependencies, cycle detection, topological sort (Kahn)
- [x] `crates/backend/src/core/sequencer.rs` — Start sequence (parallel per level, wait RUNNING), Stop sequence (reverse)
- [x] `crates/backend/src/core/branch.rs` — Error branch detection (find FAILED subgraph + dependents)
- [x] Tests: ≥15 tests (valid/invalid transitions, cycle detection, sequencing order, branch detection)

## Phase 1: Connectivity & Frontend (Week 3-4)

### P1-1: Gateway
- [x] `crates/gateway/src/main.rs` — WSS server (accept agents), WSS client (to backend)
- [x] `crates/gateway/src/registry.rs` — Agent registry (connected agents, heartbeat tracking)
- [x] `crates/gateway/src/router.rs` — Message routing agent<->backend
- [x] Tests: ≥5 tests

### P1-2: Backend WebSocket + Realtime
- [x] `crates/backend/src/websocket/` — WebSocket server, subscription per app, permission-filtered events
- [x] `crates/backend/src/websocket/mod.rs` — process_check_result wired: Agent CheckResult → FSM → state_transitions → WebSocket broadcast
- [x] Tests: WebSocket subscription + event delivery test

### P1-3: Frontend MVP
- [x] `frontend/` — Vite + React 18 + TypeScript + Tailwind + shadcn/ui setup
- [x] `frontend/src/api/client.ts` — HTTP client with JWT interceptor
- [x] `frontend/src/api/` — React Query hooks for apps, components, teams, permissions
- [x] `frontend/src/stores/` — Zustand stores (auth, ui, websocket)
- [x] `frontend/src/hooks/use-websocket.ts` — WebSocket connection + auto-reconnect
- [x] `frontend/src/components/layout/` — Sidebar, Breadcrumb, Header
- [x] `frontend/src/pages/DashboardPage.tsx` — Weather cards, app list, realtime feed, KPIs
- [x] `frontend/src/components/maps/ComponentNode.tsx` — Custom React Flow node (state colors, icons, actions)
- [x] `frontend/src/components/maps/AppMap.tsx` — React Flow canvas with edges, toolbar, zoom
- [x] `frontend/src/pages/MapViewPage.tsx` — Full map page with detail panel
- [x] `frontend/src/components/share/ShareModal.tsx` — Permission modal (users, teams, links)
- [x] `frontend/src/components/commands/CommandModal.tsx` — Command execution with terminal output
- [x] `frontend/src/pages/TeamsPage.tsx` — Team management
- [x] `frontend/src/pages/OnboardingPage.tsx` — Welcome wizard (7 steps)

### P1-4: RBAC + Auth
- [x] `crates/backend/src/auth/oidc.rs` — OIDC Authorization Code Flow (discovery, token exchange, userinfo, auto-create user)
- [x] `crates/backend/src/auth/saml.rs` — SAML 2.0 SP-Initiated SSO (AuthnRequest, ACS, metadata, group→team sync, admin group mapping)
- [x] `crates/backend/src/api/permissions.rs` — Full permissions API (users, teams, share links, effective)
- [x] `crates/backend/src/api/teams.rs` — Teams CRUD + members
- [x] `crates/backend/src/core/permissions.rs` — Effective permission resolution (MAX of direct + teams)
- [x] SAML group mapping admin API (CRUD /saml/group-mappings)
- [x] Tests: ≥10 permission tests (all 6 levels, team resolution, expiry, org admin override)

## Phase 2: Advanced Operations (Week 5-6)

### P2-1: DR Switchover
- [x] `crates/backend/src/core/switchover.rs` — 6-phase switchover engine (PREPARE→COMMIT)
- [x] `crates/backend/src/api/switchover.rs` — API endpoints (start, next-phase, rollback, commit)
- [x] Tests: Full switchover + rollback test

### P2-2: Diagnostic & Rebuild
- [x] `crates/backend/src/core/diagnostic.rs` — 3-level diagnosis, recommendation matrix
- [x] `crates/backend/src/core/rebuild.rs` — Rebuild orchestration (DAG order, protection check, bastion agent)
- [x] `crates/backend/src/api/diagnostic.rs` — POST /diagnose, POST /rebuild
- [x] Tests: Diagnostic + rebuild with protected components

### P2-3: DORA Reports
- [x] `crates/backend/src/api/reports.rs` — 7 report endpoints (availability, incidents, switchovers, audit, compliance, rto, export/pdf)
- [x] Tests: Report generation with test data (data-driven, seeds event tables, validates computed values)

### P2-4: MCP + Scheduler Integration
- [x] `crates/backend/src/api/orchestration.rs` — Scheduler API (/start, /stop, /status, /wait-running)
- [x] `crates/cli/` — appctl binary (start, stop, status, switchover, diagnose)
- [x] Tests: appctl start --wait test

## Phase 3: Packaging & E2E (Week 7-8)

### P3-1: Docker & Helm
- [x] `docker/Dockerfile.backend` — Multi-stage Rust build
- [x] `docker/Dockerfile.frontend` — Multi-stage Node build + nginx
- [x] `docker/Dockerfile.agent` — Minimal agent image
- [x] `docker/docker-compose.yaml` — Full dev stack
- [x] `helm/appcontrol/` — Helm chart (backend, frontend, postgres, redis, gateway)
- [x] OpenShift compatibility (non-root, SCC, Routes)

### P3-2: CI + Auto-Fix
- [x] `.github/workflows/ci.yaml` — Build, test, lint, security scan
- [x] `.github/workflows/auto-fix.yaml` — Claude Code auto-fix on failure
- [x] Protected files list, max 3 attempts, never on main

### P3-3: E2E Tests
- [x] `tests/e2e/common.rs` — TestContext with isolated DB, migrations, user seeding, SAML-enabled variants
- [x] `tests/e2e/test_full_start_stop.rs` — Full application start/stop sequence
- [x] `tests/e2e/test_branch_restart.rs` — Error branch detection + selective restart
- [x] `tests/e2e/test_switchover.rs` — DR switchover + rollback
- [x] `tests/e2e/test_diagnostic_rebuild.rs` — 3-level diagnostic + rebuild
- [x] `tests/e2e/test_custom_commands.rs` — Custom command execution + audit trail
- [x] `tests/e2e/test_permissions_sharing.rs` — Permission levels, team sharing, share links
- [x] `tests/e2e/test_audit_trail.rs` — Verify all actions logged, append-only respected
- [x] `tests/e2e/test_agent_and_scheduler.rs` — Agent management + scheduler integration
- [x] `tests/e2e/test_dag_validation.rs` — DAG cycle detection, topological sort
- [x] `tests/e2e/test_component_operations.rs` — Component CRUD + config snapshots
- [x] `tests/e2e/test_websocket_events.rs` — WebSocket subscription + events
- [x] `tests/e2e/test_reports.rs` — Data-driven report validation (availability%, incidents, RTO, DORA)
- [x] `tests/e2e/test_teams_crud.rs` — Teams CRUD + member management
- [x] `tests/e2e/test_share_links_advanced.rs` — Share links with expiry + max uses
- [x] `tests/e2e/test_switchover_advanced.rs` — Advanced switchover scenarios
- [x] `tests/e2e/test_diagnostic_advanced.rs` — Advanced diagnostic scenarios
- [x] `tests/e2e/test_config_snapshots.rs` — Config version tracking (before/after JSONB)
- [x] `tests/e2e/test_health_endpoints.rs` — Health + readiness probes
- [x] `tests/e2e/test_orchestration_advanced.rs` — Scheduler integration scenarios
- [x] `tests/e2e/test_org_isolation.rs` — Multi-org isolation
- [x] `tests/e2e/test_app_crud.rs` — Application CRUD operations
- [x] `tests/e2e/test_agent_management.rs` — Agent registration + status
- [x] `tests/e2e/test_incident_lifecycle.rs` — Incident detection, branch restart, audit trail, cross-branch isolation
- [x] `tests/e2e/test_saml_auth.rs` — SAML 2.0 E2E (metadata, login redirect, ACS, group mapping CRUD, group sync, admin group)
- [x] `tests/e2e/test_variables_groups.rs` — Variables CRUD, secret masking, component groups, links, command input params
- [x] `tests/e2e/test_yaml_import.rs` — YAML map import (old format → v4), links, command params, missing deps warning, audit trail

## Phase 4: Feature Parity with Old AppControl

### P4-1: Variables, Groups & Display Enhancements
- [x] `migrations/V010__variables_groups_params.sql` — app_variables, component_groups, component_links, command_input_params, display fields
- [x] `crates/backend/src/api/variables.rs` — CRUD variables + $(var) interpolation + secret masking
- [x] `crates/backend/src/api/groups.rs` — CRUD component groups (color, display_order)
- [x] `crates/backend/src/api/links.rs` — CRUD component links (documentation, CMDB, monitoring, log, runbook)
- [x] `crates/backend/src/api/command_params.rs` — CRUD command input params + regex validation + interpolation
- [x] `crates/backend/src/api/import.rs` — YAML map importer (old AppControl format → v4 model)
- [x] `crates/backend/src/api/components.rs` — Updated with display_name, description, icon, group_id fields
- [x] `frontend/src/api/apps.ts` — New hooks: useAppVariables, useComponentGroups, useComponentLinks, useImportYaml
- [x] `frontend/src/components/maps/ComponentNode.tsx` — Custom icon, display_name, group color border, links overlay
- [x] `frontend/src/components/maps/AppMap.tsx` — Group color mapping, pass groups to nodes
- [x] `frontend/src/pages/ImportPage.tsx` — YAML import page with file upload + paste
- [x] Tests: 15+ tests covering variables, groups, links, params, YAML import
