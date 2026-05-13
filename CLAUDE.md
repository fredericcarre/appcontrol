# CLAUDE.md - AppControl v4

## Project Overview

AppControl is an enterprise platform for **operational mastery and IT system resilience**. It maps applications as dependency graphs (DAGs), monitors component health via distributed agents, orchestrates sequenced start/stop/restart operations, manages DR site failover, and provides full DORA-compliant audit trails.

**AppControl is NOT a scheduler.** It integrates with existing schedulers (Control-M, AutoSys, Dollar Universe, TWS) via REST API and CLI.

## Tech Stack (EXACT versions — do not deviate)

| Layer | Technology | Version |
|-------|-----------|---------|
| Agent | Rust + Tokio + sysinfo + nix | Rust 1.88+, Tokio 1 |
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

1. **PostgreSQL 16 + SQLite dual support.** Both backends MUST be feature-equivalent. Every SQL query MUST have a `#[cfg(feature = "postgres")]` and `#[cfg(all(feature = "sqlite", not(feature = "postgres")))]` variant when using PostgreSQL-specific syntax (FILTER, ::cast, ILIKE, ANY, UNNEST, DISTINCT ON, JSONB operators, gen_random_uuid, interval arithmetic). Use `DbUuid` for UUID binds/decodes on SQLite (TEXT encoding). Use `DbJson` for JSONB columns on SQLite.
2. **Event tables are APPEND-ONLY.** `action_log`, `state_transitions`, `check_events`, `switchover_log`: NO UPDATE, NO DELETE. Ever.
3. **Log before execute.** Every user action → `action_log` INSERT **before** the action runs.
4. **Trace every transition.** Every component state change → `state_transitions` table.
5. **Process detachment.** Agent MUST double-fork + setsid. Process started by agent MUST survive agent crash.
6. **mTLS everywhere.** No plaintext between components.
7. **Check permissions first.** Every API handler checks effective permission BEFORE executing.
8. **AppControl is NOT a scheduler.** It integrates with schedulers. Never position as competitor.
9. **Delta-only sync.** Agent sends changes only, not full status on every check.
10. **Config snapshots.** Every config change → `config_versions` with before/after JSONB.
11. **No hardcoded credentials or seed data.** No emails, passwords, organization names, user accounts, or default values baked into source code. All configurable values MUST come from environment variables (SEED_*, JWT_SECRET, DATABASE_URL, etc.). The docker-compose files are the single source of truth for configuration — reading them should tell you everything needed to run the system.
12. **E2E tests MUST deploy ALL components.** E2E tests must launch the real backend, gateway, AND agent binaries. Tests that only make HTTP calls to a backend without gateway+agent are integration tests, NOT E2E. Both PostgreSQL and SQLite backends must have real E2E tests that verify the full chain: backend → gateway → agent → process start/stop/health-check.

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
6. **CI monitoring with GitHub CLI** — you MUST connect to GitHub to monitor CI:
   - Set the token: `export GH_TOKEN=<token>` (user provides it at session start)
   - The git remote uses a local proxy, so always pass `--repo fredericcarre/appcontrol` to `gh` commands
   - **After every `git push`**, monitor the CI build:
     - `gh run list --repo fredericcarre/appcontrol --branch <branch> --limit 1` to find the latest run
     - `gh run view <run-id> --repo fredericcarre/appcontrol` to see job statuses
     - `gh api repos/fredericcarre/appcontrol/check-runs/<job-id>/annotations` for failure details
     - If it fails, run tests locally (`cargo test --workspace`), fix the errors, and push again
     - Repeat until CI is green. Do NOT leave a broken build.

## Documentation Maintenance

**Always regenerate documentation** when modifying user-facing features:

1. **`docs/QUICKSTART.md`** — Update if changing auth flow, Docker setup, or getting-started steps
2. **`docs/USER_GUIDE.md`** — Update if adding/modifying UI pages, features, or API endpoints
3. **`docs/screenshots/`** — UI screenshots are auto-generated by CI (`docs-screenshots.yaml`). They capture every page via Playwright and embed them in the User Guide
4. **Screenshot markers** — Use `<!-- SCREENSHOT:page-name -->` in `USER_GUIDE.md` to mark where screenshots should be inserted. CI replaces these with actual image references

### How screenshot auto-generation works

1. CI workflow `.github/workflows/docs-screenshots.yaml` triggers on push to `main` when `frontend/src/**` or `docs/**` change
2. It builds and starts the full Docker stack
3. Playwright (`frontend/e2e-screenshots/capture.spec.ts`) navigates each page and captures screenshots to `docs/screenshots/`
4. CI replaces `<!-- SCREENSHOT:name -->` markers in `USER_GUIDE.md` with `![name](screenshots/name.png)`
5. CI commits the updated screenshots and docs

**When adding a new page:** Add a new test in `frontend/e2e-screenshots/capture.spec.ts` and a `<!-- SCREENSHOT:page-name -->` marker in `USER_GUIDE.md`.

### Documentation site (served at `docs.appcontrol.io` via the corp repo)

The same Markdown files are published as a static MkDocs Material site. The site is **built in this repo** (`fredericcarre/appcontrol`, source of truth for the docs) and **served from the corp repo** (`xcomponent/appcontrol-release`, gh-pages branch) so customers never see the dev mirror's URL.

```
fredericcarre/appcontrol (this repo)
  └─ .github/workflows/docs-pages.yaml builds with mkdocs
                ↓
                push to →  xcomponent/appcontrol-release@gh-pages
                                ↓
                                GitHub Pages serves it at docs.appcontrol.io
```

1. **Config:** `mkdocs.yml` at the repo root defines navigation, theme and plugins. `docs/index.md` is the landing page. `docs/requirements.txt` pins the Python build deps.
2. **Workflow:** `.github/workflows/docs-pages.yaml` builds the site on every push to `main` touching `docs/`, `mkdocs.yml`, `scripts/docs/`, or the root README/security/changelog files. After build it pushes the contents of `site/` to the corp repo's `gh-pages` branch using `secrets.CORP_GITHUB_TOKEN` (the same PAT the release workflow uses to mirror binaries and examples).
3. **Root files** (`SECURITY_ARCHITECTURE.md`, `CHANGELOG.md`, `RELEASE.md`) live at the repo root; the workflow copies them into `docs/` at build time so MkDocs can ingest them. Do NOT commit duplicates inside `docs/`.
4. **Custom domain (`docs.appcontrol.io`):** controlled by `docs/CNAME` — copied to the corp gh-pages tree at every deploy, so Pages on the corp repo reads it. Configure the DNS at the registrar: `docs CNAME xcomponent.github.io.` (mind the trailing dot — point it at the corp org, not the dev mirror). HTTPS is automatic once the CNAME propagates.
5. **Local preview:** `make docs-serve` regenerates the auto-generated references and starts `mkdocs serve` at <http://127.0.0.1:8000>.
6. **Strict build for verification:** `mkdocs build --strict` flags broken internal links and orphan pages.
7. **Adding a new narrative doc page:** create `docs/MY_PAGE.md`, then add it to the `nav` section of `mkdocs.yml`. Pages omitted from `nav` build silently if listed in `not_in_nav`.

**One-time setup steps** (none of this is automated; perform on the corp repo, not here):

- On `xcomponent/appcontrol-release`: Settings → Pages → Source = "Deploy from a branch" → branch `gh-pages`, folder `/ (root)`.
- On `fredericcarre/appcontrol`: Settings → Secrets → Actions → add `CORP_GITHUB_TOKEN` if it isn't already there (a PAT with `contents:write` on `xcomponent/appcontrol-release`; the release workflow already requires it).
- At the DNS registrar: `docs.appcontrol.io CNAME xcomponent.github.io.` (24h max propagation; HTTPS issued automatically by Pages once verified).

### Auto-generated reference docs (`docs/reference/`)

To keep the reference documentation in sync with the code, nine generators in `scripts/docs/` parse the source of truth and emit fresh markdown pages under `docs/reference/` on every build. The directory is **not committed** — every generator runs in CI before `mkdocs build`, and locally via `make docs-reference`.

| Generator | Source of truth | Output |
|---|---|---|
| `gen_errors.py` | `crates/backend/src/error.rs` + `crates/cli/src/main.rs` | `reference/errors.md` |
| `gen_fsm.py` | `crates/common/src/fsm.rs` + `types.rs` | `reference/fsm.md` |
| `gen_metrics.py` | scans `metrics::{counter,gauge,histogram}!` across `crates/**/*.rs` | `reference/metrics.md` |
| `gen_configuration.py` | `crates/{backend,agent}/src/config.rs` | `reference/configuration.md` |
| `gen_cli.py` | `crates/cli/src/main.rs` (clap derives) | `reference/cli.md` |
| `gen_database_schema.py` | `migrations/V*.sql` | `reference/database.md` |
| `gen_api.py` | `appcontrol-backend --export-openapi` (utoipa-derived) | `reference/api.md` |
| `gen_enums.py` | `crates/common/src/types.rs` | `reference/enums.md` |
| `gen_mcp.py` | `crates/mcp/src/tools.rs` | `reference/mcp.md` |

**When you add a new enum, env var, error variant, FSM transition, migration, OpenAPI path, metric, or MCP tool — you do NOT touch any markdown.** The next CI build regenerates the reference page. If the parser cannot match the new structure, the build fails noisily and the parser must be updated in `scripts/docs/gen_*.py`.

Some generators carry a hand-maintained `ANNOTATIONS` / `METRIC_META` / `TABLE_PURPOSE` table for context that cannot be inferred from the code (descriptions, units, business meaning). Add entries there when you introduce a new field.

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
