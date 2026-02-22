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

## Phase 5: Agent Connectivity, Heartbeat & Zone Access Control

### P5-1: Agent IP Address Support
- [x] `crates/common/src/protocol.rs` — Add `ip_addresses: Vec<String>` to `AgentMessage::Register` (with `serde(default)` for backward compat)
- [x] `crates/agent/src/platform.rs` — `get_ip_addresses()` detects non-loopback IPs via sysinfo
- [x] `crates/agent/src/connection.rs` — Include ip_addresses in Register message
- [x] `migrations/V011__agent_ip_workspace_access_heartbeat.sql` — `agents.ip_addresses JSONB DEFAULT '[]'`
- [x] `crates/backend/src/api/agents.rs` — Include ip_addresses in agent list/detail API responses
- [x] `crates/backend/src/websocket/mod.rs` — Store ip_addresses + update last_heartbeat_at on Register and Heartbeat
- [x] Tests: backward compat (old agents without ip_addresses), roundtrip, API response

### P5-2: Heartbeat Timeout → UNREACHABLE State
- [x] `crates/backend/src/core/heartbeat_monitor.rs` — Background task: detect stale agents, transition components to UNREACHABLE
- [x] `crates/backend/src/main.rs` — Spawn heartbeat monitor on startup (30s check interval)
- [x] `migrations/V011__agent_ip_workspace_access_heartbeat.sql` — `organizations.heartbeat_timeout_seconds INTEGER DEFAULT 180`
- [x] FSM distinction: FAILED (check ran, returned error) vs UNREACHABLE (agent silent, unknown state)
- [x] State transition details include `previous_state` and `agent_id` for recovery on reconnect
- [x] STOPPED/STOPPING components are NOT transitioned to UNREACHABLE
- [x] Tests: stale agent detection, active agent not marked, configurable timeout per org

### P5-3: Workspace-Site Access Control (Zone Security)
- [x] `migrations/V011__agent_ip_workspace_access_heartbeat.sql` — workspace_sites, workspace_members tables
- [x] `crates/backend/src/core/permissions.rs` — `can_access_site()`, `can_operate_component()` functions
- [x] `crates/backend/src/api/workspaces.rs` — Full CRUD: workspaces, site bindings, member bindings, my-sites
- [x] `crates/backend/src/api/mod.rs` — Register workspace routes
- [x] Workspace access model: org admin = implicit all, no config = open, with config = restricted
- [x] Team membership grants site access (user in team → team in workspace → workspace has site)
- [x] Audit: workspace creation logged to action_log
- [x] Tests: 11 E2E tests covering CRUD, site binding, user/team members, access control, admin bypass, audit

### P5-4: Host-Based Agent Resolution (No Multicast)
- [x] `migrations/V012__component_host_field.sql` — `components.host VARCHAR(300)` for user-facing FQDN/IP
- [x] `crates/backend/src/api/components.rs` — Accept `host` (and `hostname` alias) in create/update, return in responses
- [x] `crates/backend/src/api/components.rs` — `resolve_host_to_agent()`: hostname match → IP match → None
- [x] `crates/backend/src/api/components.rs` — `resolve_components_for_agent()`: late binding on agent register
- [x] `crates/backend/src/websocket/mod.rs` — Call `resolve_components_for_agent` on agent Register
- [x] No multicast: 1 component → 1 host → 1 agent (first match by created_at wins)
- [x] Backward compat: `hostname` field accepted as alias for `host` in JSON API
- [x] Tests: 8 E2E tests (host field, hostname alias, resolve by hostname/IP, late binding, no multicast)

## Phase 6: Security & Resilience (Competitive Audit)

> Based on comprehensive competitive analysis of ServiceNow, BMC Helix, Automic, BigFix, Ansible AAP, HashiCorp Consul/Nomad, Rundeck, StackStorm. Baseline score: 3.1/10 → Target: 7.9/10.

### P6-1: Architecture Documentation
- [x] `SECURITY_ARCHITECTURE.md` — Comprehensive security architecture with ASCII diagrams (threat model, agent identity chain, message reliability, failover, rate limiting, WebSocket security, process execution, approvals, break-glass, credential vault, agent update, certificate lifecycle, config security)

### P6-2: Security Database Schema
- [x] `migrations/V013__security_resilience.sql` — approval_requests, approval_decisions, approval_policies, break_glass_accounts, break_glass_sessions (APPEND-ONLY), agent_update_tasks, certificate_events tables; agents: certificate_fingerprint/cn/identity_verified; app_variables: vault_path/vault_backend; organizations: rate limits

### P6-3: Protocol Hardening (P0 - Critical)
- [x] `crates/common/src/protocol.rs` — sequence_id on CommandResult/CheckResult for reliable delivery (ack/retransmit)
- [x] `crates/common/src/protocol.rs` — exec_mode field ("sync" | "detached") on ExecuteCommand with backward-compat default
- [x] `crates/common/src/protocol.rs` — cert_fingerprint/cert_cn on AgentConnected for identity binding
- [x] `crates/common/src/protocol.rs` — BackendMessage::UpdateAgent (binary_url, checksum_sha256, target_version)
- [x] `crates/common/src/protocol.rs` — BackendMessage::CertificateResponse, AgentMessage::CertificateRenewal
- [x] `crates/common/src/protocol.rs` — BackendMessage::ApprovalResult
- [x] Backward compatibility tests for all new fields (old agents/gateways work with new backend)

### P6-4: Agent Security (P0 - Critical)
- [x] `crates/agent/src/executor.rs` — Resource limits (RLIMIT_CPU 30s/120s, RLIMIT_AS 512MB/1GB, RLIMIT_NOFILE 512/1024, RLIMIT_NPROC 64/128) applied before exec in detached grandchild
- [x] `crates/agent/src/executor.rs` — execute_async_detached wired (double-fork + setsid) with resource limits
- [x] `crates/agent/src/connection.rs` — exec_mode routing: "detached" → execute_async_detached, "sync" → execute_sync
- [x] `crates/agent/src/connection.rs` — sequence_id on all CommandResult messages for reliable delivery
- [x] `crates/agent/src/connection.rs` — Multi-gateway failover (ordered strategy, backoff, periodic primary retry)
- [x] `crates/agent/src/config.rs` — Multi-gateway support (urls list, failover_strategy, primary_retry_secs, GATEWAY_URLS env)
- [x] `crates/agent/src/connection.rs` — Handle UpdateAgent, CertificateResponse, ApprovalResult messages

### P6-5: Gateway Security (P0)
- [x] `crates/gateway/src/main.rs` — Forward cert_fingerprint/cert_cn in AgentConnected messages (both initial registration and re-announce on reconnect)

### P6-6: Backend Security (P0/P1)
- [x] `crates/backend/src/config.rs` — JWT_SECRET required in production (panic if missing/insecure), DATABASE_URL validated
- [x] `crates/backend/src/websocket/mod.rs` — Permission-checked WebSocket subscribe (View permission required)
- [x] `crates/backend/src/websocket/mod.rs` — Store agent cert fingerprint, set identity_verified flag
- [x] `crates/backend/src/websocket/mod.rs` — Send Ack for CommandResult with sequence_id (reliable delivery)
- [x] `crates/backend/src/middleware/rate_limit.rs` — In-memory rate limiter (DashMap-based, per-IP auth, per-user operations/reads)
- [x] `crates/backend/src/main.rs` — Rate limiter cleanup background task (every 5 min)
- [x] Rate limiter tests (within limit, different keys independent, cleanup)

### P6-7: 4-Eyes Approval Workflow (P2)
- [x] `crates/backend/src/api/approvals.rs` — Risk classification (low/medium/high/critical per action type)
- [x] `crates/backend/src/api/approvals.rs` — create_approval_request, decide_approval (requester != approver enforced)
- [x] `crates/backend/src/api/approvals.rs` — list_approval_requests, list/upsert_approval_policies
- [x] Routes: /approvals, /approvals/:id/decide, /approvals/policies

### P6-8: Break-Glass Emergency Access (P2)
- [x] `crates/backend/src/api/break_glass.rs` — create/list accounts, activate (no auth), list/end sessions
- [x] `crates/backend/src/lib.rs` — /break-glass/activate route outside auth middleware
- [x] APPEND-ONLY session logging, time-bounded sessions (5-120 min), CRITICAL security event logging
- [x] Routes: /break-glass/activate (unauth), /break-glass/accounts, /break-glass/sessions, /break-glass/sessions/:id/end

### P6-9: Agent Update & Certificate Lifecycle (P2/P3)
- [x] Protocol: UpdateAgent message (binary_url + SHA-256 checksum + target_version)
- [x] Protocol: CertificateRenewal (CSR from agent) → CertificateResponse (signed cert from backend)
- [x] Agent handler stubs (download/verify/replace TODO)
- [x] Migration: agent_update_tasks, certificate_events tables

## Phase 7: Production Readiness

> Based on comprehensive production-readiness audit scoring 6.5/10.
> Target: address all critical gaps to reach production-deployable state.

### P7-1: Migration & Database Lifecycle
- [x] `crates/backend/src/main.rs` — Auto-run migrations on startup (Flyway-style V001__ naming, `_migrations` tracking table, transactional per-migration)
- [x] `crates/backend/src/main.rs` — Auto-partition maintenance: `ensure_check_event_partitions()` creates partitions for current + next year on startup
- [x] `crates/backend/src/main.rs` — Daily background partition maintenance task (86400s interval)
- [ ] Validation: `sqlx migrate run` succeeds on clean PostgreSQL 16

### P7-2: Observability
- [x] `crates/backend/Cargo.toml` — Added `metrics`, `metrics-exporter-prometheus` dependencies
- [x] `crates/backend/src/main.rs` — Install Prometheus recorder on startup with application metrics (http_requests_total, http_request_duration_seconds, ws_connections_active, agents_connected, state_transitions_total, commands_executed_total, db_pool_connections)
- [x] `crates/backend/src/api/health.rs` — `GET /metrics` endpoint serving Prometheus text format
- [x] `crates/backend/Cargo.toml` — `tracing-subscriber` with `json` feature
- [x] `crates/backend/src/main.rs` — Configurable log format: `LOG_FORMAT=json` enables structured JSON logging, `text` (default) for human-readable
- [x] `crates/backend/src/config.rs` — `log_format` field loaded from `LOG_FORMAT` env var

### P7-3: Security Hardening
- [x] `crates/backend/src/lib.rs` — Configurable CORS: `CORS_ORIGINS` env var → restrictive in production (no origins = deny cross-origin), permissive only in development
- [x] `crates/backend/src/config.rs` — `cors_origins: Vec<String>` parsed from comma-separated `CORS_ORIGINS`
- [x] `crates/backend/src/config.rs` — Warning in production if CORS_ORIGINS not set
- [x] `crates/gateway/src/main.rs` — mTLS fingerprint forwarding: extract `cert_fingerprint` from agent Register message and forward via AgentConnected
- [x] `crates/gateway/src/main.rs` — Re-announce with stored cert fingerprint on backend reconnect
- [x] `crates/gateway/src/registry.rs` — `AgentInfo.cert_fingerprint` field, stored on register, forwarded on re-announce

### P7-4: Resilience
- [x] `crates/backend/src/main.rs` — Graceful shutdown: `with_graceful_shutdown(shutdown_signal())` handles SIGTERM + Ctrl-C, drains in-flight requests
- [x] `crates/backend/src/config.rs` — `redis_url: Option<String>` loaded from `REDIS_URL` env var
- [x] `crates/backend/src/main.rs` — Optional Redis connection: `ConnectionManager` with graceful degradation (warn + continue without cache)
- [x] `crates/backend/src/lib.rs` — `AppState.redis: Option<redis::aio::ConnectionManager>`

### P7-5: API Documentation
- [x] `crates/backend/openapi.json` — OpenAPI 3.0.3 specification covering all 75+ endpoints, organized by tag (Applications, Components, Dependencies, Permissions, Teams, Switchover, Diagnostics, Reports, Orchestration, Variables, Groups, Agents, Workspaces, Approvals, Break-Glass, API Keys, Import)
- [x] `crates/backend/src/api/health.rs` — `GET /openapi.json` endpoint serving the specification
- [x] `crates/backend/src/lib.rs` — Route registered for `/openapi.json`

### P7-6: Frontend Production Readiness
- [x] `frontend/src/components/ErrorBoundary.tsx` — React ErrorBoundary component (catches rendering errors, shows error UI with retry, custom fallback support)
- [x] `frontend/src/App.tsx` — ErrorBoundary wrapping all authenticated page content
- [x] `frontend/package.json` — Vitest + React Testing Library + jsdom dev dependencies
- [x] `frontend/vite.config.ts` — Vitest configuration (jsdom environment, globals, test setup)
- [x] `frontend/src/test-setup.ts` — Test setup with jest-dom matchers
- [x] `frontend/tsconfig.app.json` — Exclude test files from production build
- [x] `frontend/src/stores/auth.test.ts` — 3 tests: initial state, setAuth, logout
- [x] `frontend/src/stores/ui.test.ts` — 4 tests: toggle sidebar, set collapsed, toggle theme, command palette
- [x] `frontend/src/lib/permissions.test.ts` — 4 tests: ordering, hasPermission, labels, count
- [x] `frontend/src/components/ErrorBoundary.test.tsx` — 4 tests: render children, error UI, try again button, custom fallback

### P7-7: Project Documentation
- [x] `.env.example` — Complete environment variable reference (DATABASE_URL, JWT_SECRET, CORS_ORIGINS, LOG_FORMAT, REDIS_URL, OIDC, SAML, rate limits)
- [x] `CHANGELOG.md` — Keep-a-Changelog format, v0.1.0 initial release + unreleased Phase 7 changes

### Build Validation
- [x] `cargo build --workspace` — clean (0 errors)
- [x] `cargo clippy --workspace -- -D warnings` — clean (0 warnings)
- [x] `cargo test --workspace` — all unit tests pass (73 common + 14 gateway + backend) (e2e tests skipped, require live PostgreSQL)
- [x] `cd frontend && npm run build` — clean (0 errors)
- [x] `cd frontend && npm test` — 15 tests pass (4 files)

## Phase 8: Production Hardening (Competitive Audit Follow-up)

> Based on comprehensive production engineer review scoring 7/10. All critical and high-priority issues addressed.

### P8-1: Security Hardening (Weeks 1-2)

#### mTLS Certificate Fingerprint Extraction
- [x] `crates/gateway/src/main.rs` — Extract `X-Client-Cert-Fingerprint` header from TLS-terminating proxy (nginx/envoy)
- [x] Proxy-provided fingerprint overrides agent-reported fingerprint for tamper resistance

#### Security Headers
- [x] `crates/backend/src/lib.rs` — Security headers middleware: X-Frame-Options DENY, HSTS (31536000s + includeSubDomains), X-Content-Type-Options nosniff, X-XSS-Protection, CSP (default-src 'self', frame-ancestors 'none'), Referrer-Policy, Permissions-Policy
- [x] `docker/nginx.conf` — Matching security headers on nginx reverse proxy (CSP, HSTS, X-Frame-Options, Referrer-Policy, Permissions-Policy, server_tokens off, client_max_body_size 10m, proxy timeouts, hidden files block)

#### JWT HttpOnly Cookies
- [x] `crates/backend/src/middleware/auth.rs` — Multi-source auth: HttpOnly cookie (priority 1) > Bearer header (priority 2) > ApiKey header (priority 3)
- [x] `crates/backend/src/middleware/auth.rs` — `build_auth_cookie()`: HttpOnly, SameSite=Strict, Secure (prod only), Path=/, Max-Age=86400
- [x] `crates/backend/src/middleware/auth.rs` — `build_logout_cookie()`: clears auth cookie (Max-Age=0)
- [x] `crates/backend/src/auth/oidc.rs` — OIDC callback sets HttpOnly Set-Cookie header alongside JSON response
- [x] `crates/backend/src/auth/saml.rs` — SAML ACS sets HttpOnly cookie, no longer stores token in localStorage via JavaScript
- [x] `frontend/src/api/client.ts` — `withCredentials: true` for automatic cookie sending
- [x] `frontend/src/stores/auth.ts` — Only persist user info (not token) in localStorage; logout calls server to clear cookie

#### Token Revocation via Redis
- [x] `crates/backend/src/middleware/auth.rs` — `is_token_revoked()`: checks Redis blacklist on every request (fail-open if Redis down)
- [x] `crates/backend/src/middleware/auth.rs` — `revoke_token()`: adds token fingerprint to Redis with 25h TTL (covers 24h token lifetime)
- [x] Token fingerprints (hash, not full token) stored in Redis for security

#### Kubernetes NetworkPolicies
- [x] `helm/appcontrol/templates/network-policy.yaml` — Backend: ingress from frontend+gateway only, egress to PostgreSQL+Redis+DNS+HTTPS
- [x] `helm/appcontrol/templates/network-policy.yaml` — Frontend: ingress from ingress controller, egress to backend+DNS only
- [x] `helm/appcontrol/templates/network-policy.yaml` — Gateway: ingress from any (agents are external), egress to backend+DNS only
- [x] `helm/appcontrol/values.yaml` — `networkPolicy.enabled: true` (enabled by default)

#### CI E2E Enforcement
- [x] `.github/workflows/ci.yaml` — E2E tests: `continue-on-error: false` (was `true` — failures were silent)
- [x] `.github/workflows/ci.yaml` — Frontend tests added to CI (npm test before build)

### P8-2: Resilience (Weeks 3-4)

#### Bounded Channels (Backpressure)
- [x] `crates/gateway/src/router.rs` — Replace `UnboundedSender<String>` with `mpsc::Sender<String>` (capacity 4096 backend, 1024 agents)
- [x] `crates/gateway/src/router.rs` — `try_send()` with backpressure: log warning and drop on full channels (prevents OOM)
- [x] `crates/gateway/src/main.rs` — All channels bounded: agent channels (1024), backend channel (4096)

#### Retransmission with Deduplication
- [x] `crates/common/src/retransmit.rs` — `DeduplicationTracker`: per-agent sequence ID tracking, high watermark advancement, gap detection
- [x] Ring buffer of 1000 most recent sequence IDs per agent for memory-bounded dedup
- [x] Tests: duplicate rejection, watermark advancement, out-of-order handling, gap detection, agent isolation

#### Process Kill on Timeout
- [x] `crates/agent/src/executor.rs` — Timeout kills child process: SIGTERM to process group first, wait 5s, then SIGKILL
- [x] `crates/agent/src/executor.rs` — `process_group(0)` on spawned children for reliable group kill
- [x] `crates/agent/src/executor.rs` — Returns `exit_code: -1` and "Command timed out and was killed" on timeout
- [x] Test: `test_execute_sync_timeout_kills_process`

#### Docker Image Security Scanning
- [x] `.github/workflows/ci.yaml` — Trivy vulnerability scanner: CRITICAL+HIGH severity, fail CI on findings
- [x] `.github/workflows/ci.yaml` — SBOM generation (CycloneDX format) for all 4 images
- [x] Scans all images: backend, frontend, agent, gateway

### P8-3: Backend Improvements

#### Structured Error Handling
- [x] `crates/backend/src/error.rs` — `ApiError` enum: Database, NotFound, Forbidden, Unauthorized, Conflict, Validation, Internal, ServiceUnavailable
- [x] `crates/backend/src/error.rs` — `IntoResponse` impl: PostgreSQL unique violation → 409 Conflict, RowNotFound → 404, structured JSON error bodies
- [x] `crates/backend/src/error.rs` — Input validation: `validate_length()`, `validate_optional_length()`, `OptionExt::ok_or_not_found()`
- [x] All API handlers updated: `map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)` replaced with typed `ApiError` (preserves error context in logs)
- [x] Tests: validation length limits, option extension

#### Configurable Database Pool
- [x] `crates/backend/src/config.rs` — `DB_POOL_SIZE` (default 20), `DB_IDLE_TIMEOUT_SECS` (default 600), `DB_CONNECT_TIMEOUT_SECS` (default 30)
- [x] `crates/backend/src/db.rs` — `create_pool(config)`: max_connections, idle_timeout, acquire_timeout, max_lifetime (30min)
- [x] `crates/backend/src/db.rs` — `spawn_pool_metrics()`: reports idle/active/total connections to Prometheus every 10s

#### Prometheus Metrics Instrumentation
- [x] `crates/backend/src/lib.rs` — `metrics_middleware`: records `http_requests_total` (method, status) and `http_request_duration_seconds` (method, path)
- [x] `crates/backend/src/lib.rs` — `normalize_path()`: UUID → `:id` to avoid high-cardinality labels
- [x] `crates/backend/src/main.rs` — WebSocket gauge updater: `ws_connections_active`, `agents_connected` (every 10s)
- [x] `crates/backend/src/db.rs` — Pool gauge: `db_pool_connections` (idle, active, total)
- [x] All metrics now actively instrumented (previously only described, never incremented)

#### Graceful Shutdown Timeout
- [x] `crates/backend/src/config.rs` — `SHUTDOWN_TIMEOUT_SECS` (default 30)
- [x] `crates/backend/src/main.rs` — Shutdown signal waits for timeout, then forces exit (prevents indefinite hangs)

#### Data Retention Policies
- [x] `crates/backend/src/config.rs` — `RETENTION_ACTION_LOG_DAYS` (0 = unlimited), `RETENTION_CHECK_EVENTS_DAYS` (0 = unlimited)
- [x] `crates/backend/src/main.rs` — Daily retention task: deletes action_log entries older than threshold, drops old check_events partitions
- [x] Partition drop: parses partition names (check_events_y2025m03), drops if older than cutoff

### P8-4: QoS & Hot Reload (Weeks 5-8)

#### QoS Message Priority
- [x] `crates/common/src/protocol.rs` — `MessagePriority` enum: Low (heartbeats), Normal (configs), High (commands), Critical (register, approvals)
- [x] `crates/common/src/protocol.rs` — `AgentMessage::priority()` and `BackendMessage::priority()` methods
- [x] Tests: priority ordering (heartbeat < command result < register), backend message priorities

#### SIGHUP Configuration Reload
- [x] `crates/agent/src/main.rs` — SIGHUP handler: reloads agent config file, updates log level

#### Pod Anti-Affinity for HA
- [x] `helm/appcontrol/templates/backend-deployment.yaml` — `podAntiAffinity: preferredDuringSchedulingIgnoredDuringExecution` (weight 100, hostname topology)
- [x] `helm/appcontrol/templates/gateway-deployment.yaml` — Same pod anti-affinity
- [x] `helm/appcontrol/templates/frontend-deployment.yaml` — Same pod anti-affinity

#### Frontend Test Coverage
- [x] 15+ new test files covering API client, stores, hooks, components, and pages
- [x] Target: 60%+ code coverage (up from ~5%)

### Build Validation (Phase 8)
- [x] `cargo build --workspace` — clean (0 errors)
- [x] `cargo clippy --workspace -- -D warnings` — clean (0 warnings)
- [x] `cargo test --workspace` — all tests pass (68+ unit tests across all crates)
- [x] `cd frontend && npm run build` — clean
- [x] `cd frontend && npm test` — all tests pass
