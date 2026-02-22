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
  - **Redis integration**: Optional Redis cache (`REDIS_URL` env var), graceful degradation when unavailable
  - **mTLS fingerprint forwarding**: Gateway extracts and forwards agent certificate fingerprints to backend for identity binding
  - **Auto-partition maintenance**: Background task creates `check_events` partitions for current + next year; daily maintenance loop
  - **React ErrorBoundary**: Catches rendering errors in frontend subtrees with retry button
  - **Frontend test infrastructure**: Vitest + React Testing Library with unit tests for stores, hooks, and components
  - **`.env.example`**: Complete reference for all environment variables
  - **`CHANGELOG.md`**: This file

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
