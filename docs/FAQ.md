---
title: FAQ — frequently asked questions
description: Short answers to the questions that come up the most when an SRE, a DBA, an architect or a compliance lead first lands on AppControl.
---

# Frequently asked questions

A quick index of the questions most newcomers ask within the first five
minutes. Each answer is short on purpose — when more detail is useful,
follow the link.

If your question is not answered here, the
[Troubleshooting guide](TROUBLESHOOTING.md) covers concrete failure
modes, and the [Glossary](GLOSSARY.md) defines every term used across
the docs.

---

## What AppControl is

### What problem does AppControl solve?

It owns the **operational state** of a multi-component application:
the dependency graph, the live health of each component, the sequenced
start/stop/restart, the DR switchover, and the audit trail of every
operator action. See [POSITIONING.md](POSITIONING.md) for the long
version.

### Is AppControl a scheduler?

**No.** It plugs into an existing scheduler (Control-M, AutoSys,
Dollar Universe, TWS, Airflow, Jenkins, GitLab CI) via REST API and
CLI. The scheduler decides *when*; AppControl decides *how* and in
what *order*. The [Integration cookbook](INTEGRATION_COOKBOOK.md)
shows the wiring for each one.

### Is AppControl a monitoring tool?

**No.** It does not replace Prometheus / Datadog / Dynatrace.
AppControl runs *operational* checks (every 30s, three diagnostic
levels) to drive its FSM and trigger restarts; production monitoring
keeps its dashboards and alert rules. AppControl exposes its own
metrics at `/metrics` so your existing Prometheus can scrape it — see
[Observability](OBSERVABILITY.md).

### Is AppControl a CMDB?

**No** — but it captures the same data and reconciles it with reality.
Push your CMDB extract into AppControl with declared maturity
`candidate`, review the diff, promote to `validated`. See the
[Hands-on guide](HANDS_ON_GUIDE.md) and
[Example maps](EXAMPLE_MAPS.md).

---

## Deployment & operations

### How do I install it in five minutes?

Three paths in [Quickstart](QUICKSTART.md): standalone (no Docker),
Docker Compose with pre-built images, local dev with hot reload.

### Does it work air-gapped?

Yes. The binaries are self-contained, the database is PostgreSQL 16
*or* SQLite, the agent binary is pushed to remote hosts via the
gateway (see [Air-gap agent update](QUICKSTART.md#air-gap-agent-update)),
and the AI features run against an on-prem OpenAI-compatible endpoint
via `OPENAI_BASE_URL` (Mistral, Ollama, vLLM, GPT4All all work).

### What does it run on?

- **Agents**: Linux (glibc 2.31+), Windows Server 2019+, macOS for dev.
  See [Agent installation](AGENT_INSTALLATION.md) and
  [Windows deployment](WINDOWS_DEPLOYMENT.md).
- **Backend/Gateway**: any container runtime. Helm chart for
  Kubernetes / OpenShift in [`helm/`](https://github.com/fredericcarre/appcontrol/tree/main/helm).
- **Browser**: Chrome / Edge / Firefox latest. Safari works but has
  known print-PDF quirks documented in the Strategy deck.

### How big can it scale?

The published capacity benchmarks live in
[Capacity planning](CAPACITY_PLANNING.md). Order of magnitude: one
backend pod handles **~50 000 components** at the default check
interval (30s), one gateway holds **5 000 simultaneous agent
WebSockets**. See [Limits & quotas](LIMITS.md) for the hard limits.

### Does it support multi-tenancy?

Yes — one platform, N **organizations**, each with its own users,
sites, agents, applications, CA and audit trail. The
[Hardening checklist](HARDENING.md) covers tenant-isolation
configuration.

---

## Security & compliance

### Is it DORA-compliant?

The relevant articles (8 mapping, 11 testing, 12 RTO/RPO, 16 change,
25 third-party) are addressed in
[COMPLIANCE_DORA_NIS2.md](COMPLIANCE_DORA_NIS2.md). The
[Strategy deck](#) (draft) details how each promise materialises in
the code. NIS2 mapping is in the same document.

### Where are the credentials stored?

- **User JWTs**: signed (HMAC in dev, RS256 in prod), expire in 24h.
- **API keys**: SHA-256-hashed in the database; plaintext returned
  *once* at creation.
- **Enrollment tokens**: SHA-256-hashed, scoped (`agent` or
  `gateway`), expiring.
- **Git remote tokens**: stored as **env-var names** (`GITHUB_TOKEN`,
  `GITLAB_TOKEN`, …), never the value.
- **mTLS keys**: each org has its own CA, certs are rotated via the
  in-product PKI rotation workflow.

### What audit trail do I get for free?

Five append-only tables: `action_log` (who/what/when), 
`state_transitions` (every FSM move), `switchover_log` (DR phases),
`config_versions` (before/after JSONB), `check_events`. They are
never UPDATED or DELETED. Schemas are documented in
[reference/database.md](reference/database.md) (auto-generated).

---

## Methodology & knowledge

### What is *activation level*?

A 5-step ladder controlling how much AppControl is allowed to do on a
given application: `0 captation` (read-only modelling) → `1 advisory`
(suggests, no act) → `2 diagnostic` (executes reads, not writes) →
`3 PR-only` (operations gated by an external approval) → `4 direct
ops`. Mid-rollout teams stop at level 2 for weeks. See
[methodology.html](methodology.html) §3.

### What is *knowledge maturity*?

Per-component (and per-dependency) status declaring how trusted the
information is: `candidate` → `draft` → `reviewed` → `validated` →
`deprecated`, with a `confidence_score` from 0 to 1. Maturity is
**declared by the source**, never imposed by AppControl. A raw CMDB
scrape lands as `candidate`; an architect-reviewed map lands as
`validated`. The [Methodology in 9 screens](METHODOLOGY_WALKTHROUGH.md)
shows the full life-cycle.

### What is a *pattern*?

A reusable recipe (start/stop/check commands + tags + dependencies)
distilled from a real incident. Once defined, AppControl finds every
matching component across every application and lets you propagate
the recipe in one click. See [Example maps](EXAMPLE_MAPS.md) and
the `examples/pattern-spring-boot-jdbc.json` shipped in the repo.

### Can I round-trip with Git?

Yes — GitHub, GitLab and Gitea are supported (Contents / Repository
Files APIs). Configure a remote (`POST /api/v1/git/remotes` as
admin), bind your app to it (`PUT /api/v1/apps/:id/git`), push with
`POST /apps/:id/git/push`. The pushed JSON embeds knowledge maturity
on every row.

---

## Where do I find…

| Question | Page |
|---|---|
| The complete REST API surface | [reference/api.md](reference/api.md) (auto-generated from OpenAPI) |
| Every environment variable | [reference/configuration.md](reference/configuration.md) |
| Every metric the backend emits | [reference/metrics.md](reference/metrics.md) |
| Every CLI subcommand | [reference/cli.md](reference/cli.md) |
| Every FSM transition | [reference/fsm.md](reference/fsm.md) |
| The MCP tools for Claude / GPT | [reference/mcp.md](reference/mcp.md) |
| Concrete failure modes & fixes | [Troubleshooting](TROUBLESHOOTING.md) |
| Step-by-step recovery scenarios | [Runbooks](RUNBOOKS.md) |
| What the backend logs look like | [Observability §4](OBSERVABILITY.md#4-logs-structured-json-to-stdout) |
| What's in the code right now | [Implementation status](#) (kept off the live site while v0.9) |

> The `reference/` pages are **regenerated on every build** from the
> source of truth (Rust code, SQL migrations, OpenAPI export). Do not
> hand-edit them — the generators live in
> [`scripts/docs/`](https://github.com/fredericcarre/appcontrol/tree/main/scripts/docs).

---

## Still stuck?

- Open an issue on the dev repo:
  <https://github.com/fredericcarre/appcontrol/issues>
- File a bug report following the template in
  [Troubleshooting §How to file a bug](TROUBLESHOOTING.md#how-to-file-a-bug-report)
- For commercial support, see the [Pricing](#) page (draft).
