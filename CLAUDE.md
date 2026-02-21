# CLAUDE.md - AppControl v4

## Project Overview

AppControl is an enterprise platform for **operational mastery and IT system resilience**. It maps applications as dependency graphs (DAGs), monitors component health via distributed agents, orchestrates sequenced start/stop/restart operations, manages DR site failover, and provides full DORA-compliant audit trails.

**AppControl is NOT a scheduler.** It integrates with existing schedulers (Control-M, AutoSys, Dollar Universe, TWS) via REST API and CLI.

## Tech Stack (EXACT versions — do not deviate)

| Layer | Technology | Version |
|-------|-----------|---------|
| Agent | Rust + Tokio + sysinfo + nix | Rust 1.85+, Tokio 1 |
| Gateway | Rust + Axum + rustls | Axum 0.7 |
| Backend API | Rust + Axum + Tokio + sqlx | sqlx 0.7 (postgres, runtime-tokio, tls-rustls, uuid, chrono, json) |
| Database | PostgreSQL | 16 |
| Cache | Redis | 7 |
| Frontend | React + TypeScript + Vite + Tailwind + shadcn/ui | React 18, Vite 5, TS 5.3+, Tailwind 3.4 |
| Maps | React Flow | @xyflow/react 12+ |
| State | React Query + Zustand | @tanstack/react-query 5, zustand 4 |
| Auth | OIDC + SAML 2.0 + JWT RS256 | |
| Deploy | Docker + Helm + OpenShift compatible | |

## Repository Structure

```
appcontrol/
├── CLAUDE.md                          # THIS FILE — read first
├── PROGRESS.md                        # Implementation checklist — read second
├── Cargo.toml                         # Workspace root
├── crates/
│   ├── common/                        # Shared types, protocol, mTLS
│   │   ├── CLAUDE.md
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── agent/                         # Agent binary
│   │   ├── CLAUDE.md
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── gateway/                       # Gateway binary
│   │   ├── CLAUDE.md
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── backend/                       # API + WebSocket + FSM + RBAC
│   │   ├── CLAUDE.md
│   │   ├── Cargo.toml
│   │   └── src/
│   └── cli/                           # appctl CLI
│       ├── CLAUDE.md
│       ├── Cargo.toml
│       └── src/
├── migrations/                        # PostgreSQL migrations (sqlx)
│   └── CLAUDE.md
├── frontend/                          # React SPA
│   ├── CLAUDE.md
│   ├── package.json
│   └── src/
├── helm/                              # Helm charts
│   └── CLAUDE.md
├── docker/                            # Dockerfiles
├── tests/                             # E2E integration tests
│   ├── CLAUDE.md
│   └── e2e/
└── .github/workflows/                 # CI + auto-fix
```

## Critical Rules (NEVER violate)

1. **PostgreSQL 16 only.** No SQLite, not even for dev/test.
2. **Event tables are APPEND-ONLY.** `action_log`, `state_transitions`, `check_events`, `switchover_log`: NO UPDATE, NO DELETE. Ever.
3. **Log before execute.** Every user action → `action_log` INSERT **before** the action runs.
4. **Trace every transition.** Every component state change → `state_transitions` table.
5. **Process detachment.** Agent MUST double-fork + setsid. Process started by agent MUST survive agent crash.
6. **mTLS everywhere.** No plaintext between components.
7. **Check permissions first.** Every API handler checks effective permission BEFORE executing.
8. **AppControl is NOT a scheduler.** It integrates with schedulers. Never position as competitor.
9. **Delta-only sync.** Agent sends changes only, not full status on every check.
10. **Config snapshots.** Every config change → `config_versions` with before/after JSONB.

## How to Work

1. **Read `PROGRESS.md`** — find the next unchecked task
2. **Read the relevant `CLAUDE.md`** in the crate/directory you'll work on
3. **Read `COVERAGE.md`** — understand coverage targets for the module you're working on
4. **Implement with tests** — every public function needs at least 1 test
4. **Run validation:**
   - `cargo build --workspace`
   - `cargo test --workspace`
   - `cargo clippy --workspace -- -D warnings`
   - `cd frontend && npm run build && npm test` (when working on frontend)
5. **Update `PROGRESS.md`** — check off completed tasks, note any blockers
6. **After every `git push`**, monitor the CI build:
   - Use `gh run list --branch <branch> --limit 1` to find the latest run
   - Wait for it to complete with `gh run watch <run-id>`
   - If it fails, fetch logs with `gh run view <run-id> --log-failed`, fix the errors, and push again
   - Repeat until CI is green. Do NOT leave a broken build.

## Coding Conventions

### Rust (Agent, Gateway, Backend, CLI)
- `snake_case` for functions/variables, `PascalCase` for types/traits
- Error handling: `thiserror` for library errors, `anyhow` for application errors
- Async: `tokio` runtime, `async/await` everywhere
- Serialization: `serde` + `serde_json`
- Logging: `tracing` + `tracing-subscriber`
- Database: `sqlx` with compile-time checked queries where possible

### TypeScript (Frontend)
- Strict mode enabled
- Functional components only, hooks for state
- `React Query` for server state, `Zustand` for client state
- `Tailwind` for styling, `shadcn/ui` for components
- File structure: `ComponentName/index.tsx` + `ComponentName.hooks.ts` + `ComponentName.types.ts`

## Key Concepts

### FSM States
`UNKNOWN` → `RUNNING` | `STOPPED` | `FAILED` | `DEGRADED` | `STARTING` | `STOPPING` | `UNREACHABLE`

### Permission Levels (per application)
`view` < `operate` < `edit` < `manage` < `owner`
Effective = MAX(direct_user_permission, team_permissions). Org admin = implicit owner everywhere.

### Diagnostic Levels
- **Level 1 (Health):** `check_cmd` — runs every 30s, drives FSM. "Is the process alive?"
- **Level 2 (Integrity):** `integrity_check_cmd` — runs every 5min or on-demand, informational only. "Is the data consistent?"
- **Level 3 (Infrastructure):** `infra_check_cmd` — on-demand, informational only. "Is the OS/filesystem/prereqs OK?"

### Operations
1. Full application start (DAG sequencing)
2. Full application stop (reverse DAG)
3. Error branch restart (pink branch)
4. DR site switchover (6 phases)
5. Data corruption detection
6. Custom commands
7. Scheduler integration (API + CLI)
8. Dry run simulation
9. Diagnostic + Rebuild (3-level assessment → surgical reconstruction)
