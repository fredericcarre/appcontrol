# PROGRESS.md - AppControl v4 Implementation Tracker

> **Instructions for Claude Code:** Read this file at the start of every session. Pick the next unchecked `[ ]` task. After completing work, update this file by checking off `[x]` what you did.

## Phase 0: Foundation (Week 1-2)

### P0-1: Common Types & Protocol
- [ ] `crates/common/Cargo.toml` — dependencies: serde, serde_json, uuid, chrono, thiserror, tokio, bincode
- [ ] `crates/common/src/types.rs` — ComponentState enum (8 states), CheckResult, CommandResult, Permission levels
- [ ] `crates/common/src/fsm.rs` — FSM transition validation (valid_transition(from, to) -> bool)
- [ ] `crates/common/src/protocol.rs` — Agent<->Backend messages (heartbeat, check_result, command_result, execute_command, update_config)
- [ ] `crates/common/src/pki.rs` — mTLS certificate generation and validation utilities
- [ ] `crates/common/src/lib.rs` — re-export all public types
- [ ] Tests: ≥20 unit tests covering all FSM transitions (valid + invalid)

### P0-2: Database Migrations
- [ ] `migrations/V001__organizations_users.sql` — organizations, users tables
- [ ] `migrations/V002__agents_gateways.sql` — agents, gateways tables
- [ ] `migrations/V003__sites_applications.sql` — sites, applications tables
- [ ] `migrations/V004__components_dependencies.sql` — components (with all check/rebuild fields), dependencies, site_overrides, component_commands
- [ ] `migrations/V005__event_tables.sql` — check_events (PARTITIONED), state_transitions, action_log, switchover_log, config_versions
- [ ] `migrations/V006__teams_permissions.sql` — workspaces, teams, team_members, app_permissions_users, app_permissions_teams, app_share_links, user_favorites, saved_views
- [ ] `migrations/V007__api_keys_notifications.sql` — api_keys, notification_preferences
- [ ] `migrations/V008__materialized_views.sql` — component_daily_stats, indexes
- [ ] Validation: `sqlx migrate run` succeeds on clean PostgreSQL 16

### P0-3: Backend API Core
- [ ] `crates/backend/Cargo.toml` — axum, tokio, sqlx, serde, tracing, tower-http, jsonwebtoken
- [ ] `crates/backend/src/main.rs` — Axum server setup, router, middleware stack
- [ ] `crates/backend/src/db.rs` — PostgreSQL pool + Redis connection
- [ ] `crates/backend/src/auth/` — JWT RS256 validation, OIDC/SAML callback stubs, API key auth
- [ ] `crates/backend/src/middleware/` — auth middleware, permission check middleware, request logging
- [ ] `crates/backend/src/api/apps.rs` — CRUD applications (GET, POST, PUT, DELETE /apps)
- [ ] `crates/backend/src/api/components.rs` — CRUD components + dependencies
- [ ] `crates/backend/src/api/health.rs` — GET /health, GET /ready
- [ ] Tests: API tests with test database, ≥15 tests

### P0-4: Agent Core
- [ ] `crates/agent/Cargo.toml` — tokio, serde, sysinfo, nix, sled, tungstenite, tracing, reqwest
- [ ] `crates/agent/src/main.rs` — CLI args, config loading, agent startup
- [ ] `crates/agent/src/config.rs` — YAML config loader (gateway_url, tls, labels)
- [ ] `crates/agent/src/connection.rs` — WebSocket client to gateway/backend, reconnection logic
- [ ] `crates/agent/src/executor.rs` — **CRITICAL:** Process execution with double-fork + setsid detachment
- [ ] `crates/agent/src/scheduler.rs` — Local check scheduler (tokio interval, configurable per component)
- [ ] `crates/agent/src/buffer.rs` — Offline buffer (sled DB, 100MB FIFO, replay on reconnect)
- [ ] `crates/agent/src/native_commands.rs` — Built-in: disk_space, memory, cpu, process, tcp_port, http
- [ ] Tests: ≥10 tests, including process detachment test (start process, kill agent, verify process lives)

### P0-5: FSM Engine & DAG Sequencing
- [ ] `crates/backend/src/core/fsm.rs` — State machine engine, validate + execute transitions, write to state_transitions
- [ ] `crates/backend/src/core/dag.rs` — DAG builder from dependencies, cycle detection, topological sort (Kahn)
- [ ] `crates/backend/src/core/sequencer.rs` — Start sequence (parallel per level, wait RUNNING), Stop sequence (reverse)
- [ ] `crates/backend/src/core/branch.rs` — Error branch detection (find FAILED subgraph + dependents)
- [ ] Tests: ≥15 tests (valid/invalid transitions, cycle detection, sequencing order, branch detection)

## Phase 1: Connectivity & Frontend (Week 3-4)

### P1-1: Gateway
- [ ] `crates/gateway/src/main.rs` — WSS server (accept agents), WSS client (to backend)
- [ ] `crates/gateway/src/registry.rs` — Agent registry (connected agents, heartbeat tracking)
- [ ] `crates/gateway/src/router.rs` — Message routing agent<->backend
- [ ] Tests: ≥5 tests

### P1-2: Backend WebSocket + Realtime
- [ ] `crates/backend/src/websocket/` — WebSocket server, subscription per app, permission-filtered events
- [ ] `crates/backend/src/core/check_processor.rs` — Process incoming check results, update state via FSM, push to WebSocket
- [ ] Tests: WebSocket subscription + event delivery test

### P1-3: Frontend MVP
- [ ] `frontend/` — Vite + React 18 + TypeScript + Tailwind + shadcn/ui setup
- [ ] `frontend/src/api/client.ts` — HTTP client with JWT interceptor
- [ ] `frontend/src/api/` — React Query hooks for apps, components, teams, permissions
- [ ] `frontend/src/stores/` — Zustand stores (auth, ui, websocket)
- [ ] `frontend/src/hooks/use-websocket.ts` — WebSocket connection + auto-reconnect
- [ ] `frontend/src/components/layout/` — Sidebar, Breadcrumb, Header
- [ ] `frontend/src/pages/DashboardPage.tsx` — Weather cards, app list, realtime feed, KPIs
- [ ] `frontend/src/components/maps/ComponentNode.tsx` — Custom React Flow node (state colors, icons, actions)
- [ ] `frontend/src/components/maps/AppMap.tsx` — React Flow canvas with edges, toolbar, zoom
- [ ] `frontend/src/pages/MapViewPage.tsx` — Full map page with detail panel
- [ ] `frontend/src/components/share/ShareModal.tsx` — Permission modal (users, teams, links)
- [ ] `frontend/src/components/commands/CommandModal.tsx` — Command execution with terminal output
- [ ] `frontend/src/pages/TeamsPage.tsx` — Team management
- [ ] `frontend/src/pages/OnboardingPage.tsx` — Welcome wizard (7 steps)

### P1-4: RBAC + Auth
- [ ] `crates/backend/src/auth/oidc.rs` — OIDC flow
- [ ] `crates/backend/src/auth/saml.rs` — SAML flow
- [ ] `crates/backend/src/api/permissions.rs` — Full permissions API (users, teams, share links, effective)
- [ ] `crates/backend/src/api/teams.rs` — Teams CRUD + members
- [ ] `crates/backend/src/core/permissions.rs` — Effective permission resolution (MAX of direct + teams)
- [ ] Tests: ≥10 permission tests (all 6 levels, team resolution, expiry, org admin override)

## Phase 2: Advanced Operations (Week 5-6)

### P2-1: DR Switchover
- [ ] `crates/backend/src/core/switchover.rs` — 6-phase switchover engine (PREPARE→COMMIT)
- [ ] `crates/backend/src/api/switchover.rs` — API endpoints (start, next-phase, rollback, commit)
- [ ] Tests: Full switchover + rollback test

### P2-2: Diagnostic & Rebuild
- [ ] `crates/backend/src/core/diagnostic.rs` — 3-level diagnosis, recommendation matrix
- [ ] `crates/backend/src/core/rebuild.rs` — Rebuild orchestration (DAG order, protection check, bastion agent)
- [ ] `crates/backend/src/api/diagnostic.rs` — POST /diagnose, POST /rebuild
- [ ] Tests: Diagnostic + rebuild with protected components

### P2-3: DORA Reports
- [ ] `crates/backend/src/api/reports.rs` — 7 report endpoints (availability, incidents, switchovers, audit, compliance, rto, pdf)
- [ ] Tests: Report generation with test data

### P2-4: MCP + Scheduler Integration
- [ ] `crates/backend/src/mcp/` — MCP server (7 tools)
- [ ] `crates/backend/src/api/orchestration.rs` — Scheduler API (/start, /stop, /status, /wait-running)
- [ ] `crates/cli/` — appctl binary (start, stop, status, switchover, diagnose)
- [ ] Tests: appctl start --wait test

## Phase 3: Packaging & E2E (Week 7-8)

### P3-1: Docker & Helm
- [ ] `docker/Dockerfile.backend` — Multi-stage Rust build
- [ ] `docker/Dockerfile.frontend` — Multi-stage Node build + nginx
- [ ] `docker/Dockerfile.agent` — Minimal agent image
- [ ] `docker/docker-compose.yaml` — Full dev stack
- [ ] `helm/appcontrol/` — Helm chart (backend, frontend, postgres, redis, gateway)
- [ ] OpenShift compatibility (non-root, SCC, Routes)

### P3-2: CI + Auto-Fix
- [ ] `.github/workflows/ci.yaml` — Build, test, lint, security scan
- [ ] `.github/workflows/auto-fix.yaml` — Claude Code auto-fix on failure
- [ ] Protected files list, max 3 attempts, never on main

### P3-3: E2E Tests
- [ ] `tests/e2e/test_full_start_stop.rs` — Full application start/stop sequence
- [ ] `tests/e2e/test_branch_restart.rs` — Error branch detection + selective restart
- [ ] `tests/e2e/test_switchover.rs` — DR switchover + rollback
- [ ] `tests/e2e/test_diagnostic_rebuild.rs` — 3-level diagnostic + rebuild
- [ ] `tests/e2e/test_custom_commands.rs` — Custom command execution + audit trail
- [ ] `tests/e2e/test_permissions_sharing.rs` — Permission levels, team sharing, share links
- [ ] `tests/e2e/test_audit_trail.rs` — Verify all actions logged, append-only respected
- [ ] `tests/e2e/test_agent_offline.rs` — Agent disconnection + buffer + replay
- [ ] `tests/e2e/test_scheduler_integration.rs` — appctl start --wait, API key auth
