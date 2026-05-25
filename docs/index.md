---
title: AppControl — Operational mastery and IT system resilience
hide:
  - navigation
---

# AppControl

**An operational platform for IT resilience.** Built for production teams running mission-critical applications where availability, regulatory traceability and controlled restart are not negotiable: banks, insurance, telecoms, energy, healthcare, critical operators, integrators, MSPs.

AppControl sits **across five families of existing tools** — supervision, CMDB, scheduler, hypervisor, container orchestrator — without replacing any. It adds the layer they were never designed to provide: the *application*, in motion, as a runnable artifact.

![map-overview](screenshots/map-overview.png)

---

## Three moments, three clicks

### Sunday 3:17 AM — the core-banking batch crashed

Your senior sysadmin is on holiday. The runbook is two years out of date.

You open AppControl. The application map is already on screen. The broken branch is red.
One click on **Restart error branch**. Components restart in the correct order, in parallel where possible.
Four minutes later, everything is green. The audit trail is chained, signed, and ready to export.

![incident-recovery](screenshots/incident-recovery.gif)

### Tuesday 2 PM — Paris → Lyon DR failover drill

Six phases, rollback available at every step. You watch each component switch site in real time. The compliance report is ready before the meeting ends.

![dr-switchover](screenshots/dr-switchover.gif)

### Thursday 5 PM — auditor walks in

Every action, every state change, every config diff is in `action_log` and `state_transitions`. Append-only, signed, chained. Export to PDF or CSV in one click.

![audit-export](screenshots/audit-export.gif)

---

## The five questions AppControl answers

Your ops tools say a lot. None were designed to answer the questions you actually ask when something is going wrong:

- The **real-time state** of your processes, here and now
- The **impact** of a failure on the service rendered to the user
- The **time** the restart will take
- The **order** in which components have to come back
- The **interactions** between services during the incident

AppControl answers these five questions. It integrates with your existing stack, requires no replacement, and provides the **executable overview** that was missing.

---

## What it does

- **Dependency maps** — model applications as DAGs (strong / weak dependencies), visualised in React Flow
- **Sequenced operations** — start, stop, restart in DAG order, parallelism within levels
- **3-level diagnostics** — health (process alive?), integrity (data consistent?), infrastructure (OS / prereqs OK?)
- **DR switchover** — 6-phase site failover with rollback at any phase
- **Append-only audit** — DORA-compliant logs for every action, state transition, configuration change
- **Scheduler integration** — REST + `appctl` CLI for Control-M, AutoSys, $Universe, TWS
- **MCP-native** — talk to your production from Claude, ChatGPT, Cursor or any MCP-compatible client

---

## DORA compliance

Regulation 2022/2554, effective 17 January 2025. AppControl directly addresses **Articles 8** (mapping), **11** (continuity testing), **12** (reconstruction), **16** (incident records), **25** (cyber scenarios). Penalties: up to **2 % of annual global revenue** for the entity, up to **€1M** for executives personally.

---

## Design safeguards

A platform that *can* stop production *can* break it. AppControl answers by construction, not by procedure:

- Granular 5-level RBAC per application: `view` < `operate` < `edit` < `manage` < `owner`
- Advisory mode (observe without executing)
- Dry-run on every action
- Optional PR-only mode (start/stop via merged pull request)
- mTLS everywhere
- Append-only audit (no UPDATE, no DELETE, ever)

Each application picks its autonomy level (observation → diagnostics → operations → drill → DR) and can step back at any time.

---

## Where to go next

<div class="grid cards" markdown>

-   :material-rocket-launch:{ .lg .middle } **Get started in 5 minutes**

    ---

    Three deployment paths: standalone (no Docker), Docker Compose, or local dev with hot-reload.

    [:octicons-arrow-right-24: Quickstart](QUICKSTART.md)

-   :material-book-open-variant:{ .lg .middle } **Use the product**

    ---

    Every page, every operation, every panel — with screenshots.

    [:octicons-arrow-right-24: User Guide](USER_GUIDE.md)

-   :material-server-network:{ .lg .middle } **Deploy to production**

    ---

    Helm, OpenShift, Azure gateway, Windows agents, air-gapped installs.

    [:octicons-arrow-right-24: Production deployment](PRODUCTION_DEPLOYMENT.md)

-   :material-shield-lock:{ .lg .middle } **Understand the architecture**

    ---

    Agent → Gateway → Backend → UI, with mTLS end-to-end and append-only audit.

    [:octicons-arrow-right-24: Architecture](architecture.md)

-   :material-cog:{ .lg .middle } **Integrate with your stack**

    ---

    Control-M, AutoSys, $Universe, TWS, Prometheus, OIDC/SAML, MCP.

    [:octicons-arrow-right-24: Integration cookbook](INTEGRATION_COOKBOOK.md)

-   :material-lock-check:{ .lg .middle } **Review the security model**

    ---

    Threat model, mTLS chain, secrets handling, append-only invariants.

    [:octicons-arrow-right-24: Security architecture](SECURITY_ARCHITECTURE.md)

</div>

---

## Tech stack

Rust 1.88+ (agent, gateway, backend) · PostgreSQL 16 or SQLite · React 18 / TypeScript / Vite · mTLS everywhere · Docker + Helm + OpenShift compatible · on-prem, private cloud or full air-gap.

---

## License

Proprietary. All rights reserved.
