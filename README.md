# AppControl v4

[![CI](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml)
[![Security Scan](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml/badge.svg?branch=main&event=push)](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml)
[![codecov](https://codecov.io/gh/fredericcarre/appcontrol/graph/badge.svg)](https://codecov.io/gh/fredericcarre/appcontrol)
[![Release](https://img.shields.io/github/v/release/fredericcarre/appcontrol?display_name=tag&sort=semver)](https://github.com/fredericcarre/appcontrol/releases/latest)
[![License: Proprietary](https://img.shields.io/badge/license-Proprietary-red.svg)](#license)

**Operational mastery and IT system resilience.** AppControl maps your applications as dependency graphs (DAGs), monitors component health via distributed agents, orchestrates sequenced start/stop/restart operations, manages DR site failover, and provides full DORA-compliant audit trails.

> AppControl is **not** a scheduler. It integrates with existing schedulers (Control-M, AutoSys, Dollar Universe, TWS) via REST API and CLI.

---

## Key Features

| Feature | Description |
|---------|-------------|
| **Dependency Maps** | Model applications as DAGs with strong/weak dependencies, visualized in React Flow |
| **Sequenced Operations** | Start, stop, restart in correct DAG order with parallel execution within levels |
| **3-Level Diagnostics** | Health (process alive?), Integrity (data consistent?), Infrastructure (OS/prereqs OK?) |
| **Error Branch Restart** | Detect failed subgraph, restart only affected components |
| **DR Switchover** | 6-phase site failover (Paris → Lyon) with rollback at any phase |
| **RBAC** | 5 permission levels (view < operate < edit < manage < owner), teams, share links |
| **Audit Trail** | DORA-compliant append-only logs for every action, state transition, and config change |
| **Scheduler Integration** | REST API + `appctl` CLI for Control-M, AutoSys, TWS, Dollar Universe |
| **Distributed Agents** | Rust agents with process detachment (survives agent crash), offline buffering, mTLS |
| **Realtime UI** | React 18 SPA with WebSocket live updates, weather-style dashboards |

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         Frontend (React 18)                         │
│             TypeScript · Vite · Tailwind · shadcn/ui                │
│                     React Flow · React Query                        │
└──────────────────────┬───────────────────────┬───────────────────────┘
                  REST │                   WS  │
┌──────────────────────▼───────────────────────▼───────────────────────┐
│                       Backend (Rust + Axum)                          │
│       FSM Engine · DAG Sequencer · RBAC · Switchover · Reports      │
├──────────────────┬─────────────────────────────────────────────────────┤
│   PostgreSQL 16  │                              JWT RS256 Auth         │
└──────────────────┘                              └─────────────────────┘
                                  │
┌─────────────────────────────────▼────────────────────────────────────┐
│                      Gateway (Rust + Axum)                           │
│                 WebSocket relay · mTLS · Routing                     │
└───────────────────┬──────────────────────┬───────────────────────────┘
                    │                      │
         ┌──────────▼──────────┐ ┌─────────▼──────────┐
         │   Agent (Rust)      │ │   Agent (Rust)      │
         │ Health checks       │ │ Health checks       │
         │ Process detachment  │ │ Process detachment  │
         │ Offline buffer      │ │ Offline buffer      │
         │ Native commands     │ │ Native commands     │
         └─────────────────────┘ └─────────────────────┘
```

## Quick Start

Get running in under 5 minutes with pre-built images — no local build required.

```bash
git clone https://github.com/fredericcarre/appcontrol.git && cd appcontrol

# Start the full stack from the latest release
docker compose -f docker/docker-compose.release.yaml up -d

# Verify backend is healthy
curl http://localhost:3000/health   # → {"status":"ok"}

# Open the UI
open http://localhost:8080
```

**Login:** The form is pre-filled with `admin@localhost`. Leave the password empty and click **Sign in**.

Pin a specific version:

```bash
APPCONTROL_VERSION=0.2.0 docker compose -f docker/docker-compose.release.yaml up -d
```

See [QUICKSTART.md](docs/QUICKSTART.md) for the full guide (CLI install, agent setup, first map creation).

## Example Maps

Three ready-to-import application maps in [`examples/`](examples/):

| Example | Components | Highlights |
|---------|:----------:|------------|
| [Three-Tier Web App](examples/three-tier-webapp.json) | 7 | Strong/weak deps, DB replication, batch processing |
| [Microservices E-Commerce](examples/microservices-ecommerce.json) | 12 | API gateway, message broker, service-per-DB pattern |
| [Core Banking System](examples/banking-core-system.json) | 9 | DR switchover (Paris→Lyon), Control-M integration, DORA compliance |

## CLI

```bash
# Download from the latest release
gh release download --repo fredericcarre/appcontrol --pattern 'appctl-linux-amd64' --dir /usr/local/bin
chmod +x /usr/local/bin/appctl

# Use it
export APPCONTROL_URL=http://localhost:3000

appctl start my-app --wait --timeout 120
appctl status my-app --format table
appctl diagnose my-app --level 2
appctl switchover my-app --target-site lyon --mode FULL --wait
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Agent | Rust 1.88+ · Tokio · sysinfo · nix |
| Gateway | Rust · Axum 0.7 · rustls |
| Backend API | Rust · Axum · sqlx 0.7 · PostgreSQL 16 |
| Frontend | React 18 · TypeScript 5.3 · Vite 5 · Tailwind 3.4 · shadcn/ui |
| Maps | React Flow (@xyflow/react 12+) |
| State | React Query 5 · Zustand 4 |
| Auth | OIDC · SAML 2.0 · JWT RS256 |
| Deploy | Docker · Helm · OpenShift compatible |

## Repository Layout

```
appcontrol/
├── crates/
│   ├── common/       # Shared types, FSM, protocol, mTLS
│   ├── agent/        # Distributed agent binary
│   ├── gateway/      # WebSocket relay
│   ├── backend/      # API + WebSocket + FSM + RBAC
│   └── cli/          # appctl CLI
├── migrations/       # PostgreSQL migrations (sqlx)
├── frontend/         # React SPA
├── examples/         # Example application maps
├── helm/             # Helm charts (OpenShift compatible)
├── docker/           # Dockerfiles + compose files
├── tests/            # E2E integration tests
└── .github/          # CI + auto-fix workflows
```

## Test Coverage

| Module | Target | Focus |
|--------|:------:|-------|
| `common/` | 90% | FSM transitions, protocol serialization |
| `backend/core/` | 80% | FSM, DAG, permissions, switchover, diagnostics |
| `backend/api/` | 70% | Every endpoint: happy path + error path |
| `agent/` | 75% | Executor, scheduler, offline buffer |
| `frontend/` | 60% | Hooks, stores, permission logic |
| **E2E** | 9 scenarios | Full stack with real DB and WebSocket |

```bash
# Rust tests + coverage
cargo llvm-cov --workspace --html --output-dir coverage/

# Frontend tests
cd frontend && npm test -- --coverage
```

See [COVERAGE.md](COVERAGE.md) for the full coverage strategy and per-module targets.

## Development

```bash
# Start dev infrastructure (PostgreSQL only)
docker compose -f docker/docker-compose.dev.yaml up -d

# Or run the full setup script
./docker/dev-setup.sh

# Build everything
cargo build --workspace

# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings
cd frontend && npm run lint && npm run build
```

See [QUICKSTART.md](docs/QUICKSTART.md) for the "from source" section and [RELEASE.md](RELEASE.md) for the release procedure.

## Contributing

1. Read [PROGRESS.md](PROGRESS.md) — find the next unchecked task
2. Read the `CLAUDE.md` in the crate you'll work on
3. Implement with tests
4. Validate: `cargo build && cargo test && cargo clippy -- -D warnings`
5. Update [PROGRESS.md](PROGRESS.md)

## License

Proprietary. All rights reserved.
