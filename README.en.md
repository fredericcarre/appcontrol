# AppControl

*Read this in [French](README.md) for the full narrative.*

**AppControl is an operational platform for IT resilience.** Built for production teams running mission-critical applications where availability, regulatory traceability and controlled restart are not negotiable: banks, insurance, telecoms, energy, healthcare, critical operators, integrators, MSPs.

It sits **across five families of existing tools** — supervision, CMDB, scheduler, hypervisor, container orchestrator — without replacing any. It adds the layer they were never designed to provide: the *application*, in motion, as a runnable artifact.

![map-overview](docs/screenshots/map-overview.png)

[![CI](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/fredericcarre/appcontrol/actions/workflows/ci.yaml)
[![codecov](https://codecov.io/gh/fredericcarre/appcontrol/graph/badge.svg)](https://codecov.io/gh/fredericcarre/appcontrol)
[![Release](https://img.shields.io/github/v/release/fredericcarre/appcontrol?display_name=tag&sort=semver)](https://github.com/fredericcarre/appcontrol/releases/latest)
[![License: Proprietary](https://img.shields.io/badge/license-Proprietary-red.svg)](#license)

## What it does

- **Dependency maps** — model applications as DAGs (strong / weak dependencies), visualised in React Flow
- **Sequenced operations** — start, stop, restart in DAG order, parallelism within levels
- **3-level diagnostics** — health (process alive?), integrity (data consistent?), infrastructure (OS / prereqs OK?)
- **DR switchover** — 6-phase site failover with rollback at any phase
- **Append-only audit** — DORA-compliant logs for every action, state transition, configuration change
- **Scheduler integration** — REST + `appctl` CLI for Control-M, AutoSys, $Universe, TWS
- **MCP-native** — talk to your production from Claude, ChatGPT, Cursor or any MCP-compatible client

## DORA compliance

Regulation 2022/2554, effective 17 January 2025. AppControl directly addresses **Articles 8** (mapping), **11** (continuity testing), **12** (reconstruction), **16** (incident records), **25** (cyber scenarios). Penalties: up to **2 % of annual global revenue** for the entity, up to **€1M** for executives personally.

See the [French README](README.md#dora--pas-du-confort-une-obligation) for the full *Article → mechanism* table.

## Design safeguards

A platform that *can* stop production *can* break it. AppControl answers by construction, not by procedure: granular 5-level RBAC per application, advisory mode (observe without executing), dry-run on every action, optional PR-only mode (start/stop via merged pull request), mTLS everywhere, append-only audit (no UPDATE, no DELETE, ever). Each application picks its autonomy level (observation → diagnostics → operations → drill → DR) and can step back at any time.

## Three engagement modes

Same product, three angles:

- **Application rebuild** — pilot a 12–24-month modernization program: target DAG modelling, component-by-component validation, clean restarts throughout, regulatory audit delivered at exit.
- **DORA / NIS2 compliance** — append-only signed audit trail as native output, no replacement of the existing ops stack.
- **24/7 on-call enablement** — operators at 3 AM get an immediate application map, controlled restart in the right DAG order, automatic audit — one console to open.

## Tech stack

Rust 1.88+ (agent, gateway, backend) · PostgreSQL 16 or SQLite · React 18 / TypeScript / Vite · mTLS everywhere · Docker + Helm + OpenShift compatible · on-prem, private cloud or full air-gap.

See [QUICKSTART](docs/QUICKSTART.md), [Architecture](docs/architecture.md), [Security](SECURITY_ARCHITECTURE.md), [Positioning](docs/POSITIONING.md).

## Performance

Hot-path numbers come from a `criterion` suite at `crates/benchmarks/`. Measured on a GitHub-hosted runner:

- FSM transition decision (`next_state_from_check`): **≈ 1 ns / call** (~1 Gelem/s) — the database INSERT dominates, never the FSM
- DAG topological sort on a 500-component application (10×50, ~22k edges): **2.7 ms / sort**
- Effective-permission resolution on SQLite: **≈ 360 µs / call**, flat from 10 to 10 000 users / 100 to 100 000 grants — the per-query round-trip dominates, not data volume

Reproduce locally: `cargo bench -p appcontrol-benchmarks` (HTML report in `target/criterion/`). Full reference table: [docs/CAPACITY_PLANNING.md](docs/CAPACITY_PLANNING.md#benchmarks).

<!-- RELEASE-CUT -->
<!--
Everything above this marker is shared narrative — copied verbatim
into the corp release README (xcomponent/appcontrol-release).
Everything BELOW is dev-context-specific (git clone install, dev
license) and is replaced at release time by corp/release-suffix.en.md
via .github/workflows/release.yaml.
Keep the marker on its own line; do not delete it.
-->

## Quickstart

```bash
git clone https://github.com/fredericcarre/appcontrol.git && cd appcontrol
docker compose -f docker/docker-compose.release.yaml up -d
open http://localhost:8080
```

Login: `admin@localhost`, empty password.

## License

Proprietary. All rights reserved.
