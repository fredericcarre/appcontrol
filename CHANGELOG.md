# Changelog

All notable changes to AppControl will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase 7: Production readiness improvements
  - **Migration auto-run**: Backend automatically runs `sqlx` migrations on startup
  - **Graceful shutdown**: SIGTERM/SIGINT handler with in-flight request draining
  - **Configurable CORS**: `CORS_ORIGINS` env var replaces `CorsLayer::permissive()`; restrictive by default in production
  - **Structured JSON logging**: `LOG_FORMAT=json` enables structured JSON logs for log aggregation (ELK, Loki, Datadog)
  - **Prometheus metrics**: `/metrics` endpoint with `http_requests_total`, `http_request_duration_seconds`, `ws_connections_active`, `agents_connected`, `state_transitions_total`, `commands_executed_total`, `db_pool_connections`
  - **Optional Redis cache**: `REDIS_URL` env var with graceful degradation when unavailable (removed in Phase 10, replaced with PostgreSQL)
  - **mTLS fingerprint forwarding**: Gateway extracts and forwards agent certificate fingerprints to backend for identity binding
  - **Auto-partition maintenance**: Background task creates `check_events` partitions for current + next year; daily maintenance loop
  - **React ErrorBoundary**: Catches rendering errors in frontend subtrees with retry button
  - **Frontend test infrastructure**: Vitest + React Testing Library with unit tests for stores, hooks, and components
  - **`.env.example`**: Complete reference for all environment variables
  - **`CHANGELOG.md`**: This file
- Phase 8: Production hardening
  - **mTLS proxy fingerprint extraction**: Gateway extracts `X-Client-Cert-Fingerprint` header from TLS-terminating proxies (nginx/envoy), overriding agent-reported fingerprint for tamper resistance
  - **Security headers middleware**: X-Frame-Options DENY, Content-Security-Policy (default-src 'self', frame-ancestors 'none'), HSTS (31536000s + includeSubDomains), X-Content-Type-Options nosniff, X-XSS-Protection, Referrer-Policy, Permissions-Policy
  - **JWT HttpOnly cookie support**: Multi-source auth (HttpOnly cookie > Bearer header > API key), SameSite=Strict, Secure in production; OIDC and SAML callbacks set cookies directly
  - **Token revocation via PostgreSQL**: Token fingerprint blacklist checked on every request (originally Redis-backed, migrated to PostgreSQL in Phase 10)
  - **Kubernetes NetworkPolicies**: Backend ingress from frontend+gateway only, frontend ingress from ingress controller, gateway ingress from any (external agents); egress restricted per component
  - **Bounded channels with backpressure**: Gateway uses `mpsc::Sender` (1024 agent, 4096 backend) with `try_send()` — drops messages on full channels to prevent OOM
  - **Retransmission with deduplication**: `DeduplicationTracker` with per-agent sequence ID tracking, high watermark advancement, gap detection, ring buffer of 1000 IDs per agent
  - **Process kill on timeout**: SIGTERM to process group, 5s grace period, then SIGKILL; returns exit_code -1 on timeout
  - **Trivy vulnerability scanning in CI**: CRITICAL+HIGH severity scan on all 4 Docker images, SBOM generation (CycloneDX format)
  - **Structured error handling**: `ApiError` enum (Database, NotFound, Forbidden, Unauthorized, Conflict, Validation, Internal, ServiceUnavailable) with PostgreSQL unique violation → 409, structured JSON error bodies, input validation helpers
  - **Configurable database pool**: `DB_POOL_SIZE` (default 20), `DB_IDLE_TIMEOUT_SECS` (default 600), `DB_CONNECT_TIMEOUT_SECS` (default 30), max_lifetime 30min, pool metrics reported to Prometheus every 10s
  - **Prometheus metrics instrumentation**: `metrics_middleware` records `http_requests_total` (method, status) and `http_request_duration_seconds` (method, path) with UUID path normalization; WebSocket and pool gauges updated every 10s
  - **Graceful shutdown with configurable timeout**: `SHUTDOWN_TIMEOUT_SECS` (default 30) prevents indefinite hangs on SIGTERM
  - **Data retention policies**: `RETENTION_ACTION_LOG_DAYS` and `RETENTION_CHECK_EVENTS_DAYS` (0 = unlimited), daily background task drops old check_events partitions
  - **QoS message priority**: `MessagePriority` enum (Low for heartbeats, Normal for configs, High for commands, Critical for register/approvals) on all WebSocket messages
  - **SIGHUP config reload**: Agent reloads config file and updates log level on SIGHUP signal
  - **Pod anti-affinity rules**: `preferredDuringSchedulingIgnoredDuringExecution` (weight 100, hostname topology) on backend, gateway, and frontend deployments
  - **60%+ frontend test coverage**: 15+ new test files covering API client, stores, hooks, components, and pages (up from ~5%)
- Phase 9: Sharing, API keys & documentation
  - **Workspace validation for permissions**: `validate_workspace_access()` checks target user/team has site access before granting permissions or consuming share links
  - **User search/discovery with autocomplete**: `GET /users/search?q=&limit=` endpoint (ILIKE on email + display_name, org-scoped) with `UserPicker` typeahead component
  - **Share link consumption flow**: `POST /share-links/consume` (validates token, expiry, max uses, workspace, grants permission), `GET /share/:token` preview (unauthenticated), accept/decline UI page
  - **Combined permissions API**: `GET /apps/:app_id/permissions` (users + teams joined with details), `DELETE /apps/:app_id/permissions/:perm_id` (tries user then team)
  - **API key management UI**: Full CRUD page — create key with copy-once warning, list keys (prefix, scopes, status, dates), revoke; linked from Settings
  - **ShareModal improvements**: UserPicker autocomplete replaces free-text input, share link revocation button, copy-to-clipboard feedback
  - **Comprehensive documentation**: `docs/QUICKSTART.md` (Docker Compose, local dev, auth setup), `docs/USER_GUIDE.md` (all features, operations, sharing, API keys, administration with screenshot placeholders), `CLAUDE.md` documentation maintenance instructions
  - **CI screenshot auto-generation**: Playwright captures all pages (1440x900, light theme), CI workflow builds stack, captures screenshots, embeds in docs, auto-commits
- Phase 10: Production review fixes
  - **Rebuild engine waits for command completion**: Polls `command_executions` table, waits for each rebuild command to complete or timeout before proceeding; suspends on failure instead of blindly restarting
  - **Switchover SYNC waits for integrity check results**: Dispatches integrity checks AND polls for results; fails the SYNC phase if any check returns non-zero or times out
  - **FSM database transactions**: `transition_component()` wraps state read + validation + INSERT + UPDATE in a transaction with `SELECT ... FOR UPDATE` to prevent race conditions
  - **cargo-audit enforcement in CI**: Removed `continue-on-error: true` — vulnerability findings now fail the build
  - **Action log archival instead of deletion**: Retention task archives old entries to `action_log_archive` table instead of deleting, respecting append-only compliance (Critical Rule #2)
  - **Webhook circuit breaker**: After 5 consecutive failures, webhook is skipped for 5min cooldown; success resets circuit; lock-free global state via LazyLock + DashMap
  - **PostgreSQL-backed rate limiting**: `HA_MODE=true` uses PostgreSQL INCR+EXPIRE pattern (safe across replicas), falls back to in-memory DashMap otherwise (replaces Redis-backed implementation)
  - **Diagnostic query optimization**: Single query with `ROW_NUMBER() OVER (PARTITION BY component_id, check_type)` replaces O(3N) individual queries
  - **Dead code cleanup**: Removed unused variables in switchover config swap
  - **Auto-init PKI at startup**: `auto_init_pki()` generates CA for organizations without one at startup — zero-config mTLS
  - **Full removal of Redis dependency**: Token revocation moved to `revoked_tokens` PostgreSQL table, rate limiting uses PostgreSQL in HA mode, `redis` crate removed from dependencies, Redis service removed from CI

## [0.1.0] - 2026-02-22

### Added
- Initial release of AppControl v4
- **Common crate**: FSM (8 states), protocol types, mTLS PKI utilities
- **Agent**: Process execution with double-fork + setsid, offline buffer (sled), multi-gateway failover, native commands (disk/memory/cpu/process/tcp/http), resource limits (RLIMIT_CPU/AS/NOFILE/NPROC)
- **Gateway**: Agent registry, message routing, backend auto-reconnect, agent rate limiting
- **Backend API**: 75+ REST endpoints under `/api/v1/`
  - Applications CRUD with DAG-based start/stop/restart
  - Components CRUD with FSM state management
  - Teams, permissions (5 levels), share links
  - DR switchover (6-phase engine)
  - 3-level diagnostics + rebuild orchestration
  - DORA-compliant reports (availability, incidents, switchovers, audit, compliance, RTO)
  - Scheduler integration (orchestration API + CLI)
  - Variables, groups, links, command parameters
  - YAML map import (old AppControl format)
  - 4-eyes approval workflow
  - Break-glass emergency access
  - SAML 2.0 + OIDC + API key authentication
  - Workspace-site access control
  - Host-based agent resolution
- **CLI**: `appctl` binary (start, stop, status, switchover, diagnose)
- **Frontend**: React 18 + TypeScript + Tailwind + shadcn/ui + React Flow
  - Dashboard with weather cards and KPIs
  - Interactive DAG map with component nodes
  - Team management
  - Agent monitoring
  - DORA reports
  - YAML import
  - Onboarding wizard
- **Database**: 14 PostgreSQL migrations, partitioned event tables, append-only audit trail
- **Docker**: Multi-stage builds (backend, frontend, gateway, agent), 3 compose variants (dev, build, release)
- **Helm**: Kubernetes chart with OpenShift compatibility
- **CI/CD**: Build + test + lint + security scan, multi-platform release pipeline
- **Security**: JWT RS256, rate limiting (3-tier), config version snapshots, heartbeat monitoring
- **Tests**: 134 Rust unit tests, 183 E2E test functions
