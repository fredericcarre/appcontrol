---
title: Methodology in 9 screens
description: Walk through the AppControl methodology, screen by screen, with annotated screenshots.
---

# The methodology, screen by screen

This page walks an architect or ops engineer through the AppControl
methodology by showing the actual UI for each phase. Use it as the
five-minute tour you give a new joiner or a prospect.

The companion document **[Example maps](EXAMPLE_MAPS.md)** contains the
copy-pasteable payloads behind each screen.

---

## Phase 1 — Captation multi-sources

A single screen consolidates every way data enters AppControl: agents
in discovery mode, referential ingestion (CMDB, XL Release, XL Deploy,
flow registry, ITSM), and AI-assisted schema validation. The
ingestion wizard at the bottom lets an operator push a payload right
there — no curl required.

<!-- SCREENSHOT:captation-page -->

Each source card is colour-coded:

- **Teal** — push connectors (CMDB, XL, flows)
- **Indigo** — pull connectors (ServiceNow Table API, Jira JQL)
- **Amber** — incident ingestion (special: not part of the structural
  map, feeds the learning loop in phase 5)

---

## Phase 2 — Audit &amp; reconciliation

The audit report (link inside the Captation page) makes contradictions
between sources explicit before anything else happens. Coverage
indicators tell you whether the map is ready to publish.

<!-- SCREENSHOT:audit-report -->

---

## Phase 3 — Construction &amp; review

Once the map is published, the **Knowledge tab** on every component
becomes the central artefact of the review phase. The reviewer can:

1. Promote the component through the ladder (`candidate` → `draft` →
   `reviewed` → `validated`)
2. Set a confidence score with the slider
3. Drop notes, reviews, todos or warnings (annotations panel)

The **knowledge pip** on the node in the map summarises this state at
a glance — a draft component shows an amber dot, a reviewed one shows
indigo, a validated one shows nothing (everything's fine).

<!-- SCREENSHOT:knowledge-tab -->

### Maturity declared by the source

When a map arrives from an external source (Git CI, scrape, manual
upload), the import API lets the caller declare the maturity. This is
the methodology rule:

| Origin | Default | Override |
|---|---|---|
| Git CI push | `reviewed` (declared by CI) | per-component in payload |
| CMDB scrape | `candidate` (declared honestly) | per-component or app-wide |
| Manual UI | `draft` (DB default) | promote via the Knowledge tab |
| Agent discovery | `candidate` | promote via review |

The full resolution order, with concrete examples, is in
[Example maps](EXAMPLE_MAPS.md).

---

## Phase 4 — Activation graduelle

Each application carries an **activation level** (0 to 4) that gates
what AppControl is allowed to do. Move it manually from the Activation
page; the change appears immediately on the *MaturityStrip* at the top
of the MapView.

<!-- SCREENSHOT:activation-page -->

The levels, in order:

- **0 Captation** — only reads from referentials, no agent ops
- **1 Advisory** — agents observe, no checks run
- **2 Diagnostic** — health/integrity/infra checks run, no start/stop
- **3 PR-only** — operations need an `X-PR-Approved-Sha` header
- **4 Direct** — operations allowed for users with the right RBAC

A new application created today defaults to **level 1**. Existing
applications onboarded before the feature defaulted to **level 4**
(no behaviour change).

---

## Phase 4 (cont.) — Operating the map

Once activation is high enough, the map becomes operable. The
**MapToolbar** offers Start All, Stop, Branch restart, Switchover,
Diagnose and Export — all gated by the activation level and the
operator's RBAC.

<!-- SCREENSHOT:map-view -->

Every operation:

- Logs to `action_log` BEFORE execution (audit append-only)
- Fires a webhook `Operation` event when complete (success or
  failure)
- Records timings — feeds the DORA RTR metric

---

## Phase 5 — Learning from incidents

When an incident hits a component, the **Incident Recovery** flow
ties the recorded ITSM ticket back to the touched components. An AI
provider (Anthropic, OpenAI or an on-prem gateway) can be invoked to
produce a ranked list of root-cause hypotheses with recommended
remediation actions.

<!-- SCREENSHOT:incident-recovery -->

After the incident is resolved, a **pattern** can be created from the
learnings (a specific check, integrity command or rebuild step). The
pattern lives in the org-wide library and **propagates** to similar
components in other applications — the candidate list is computed
automatically by matching `technology` and missing fields.

<!-- SCREENSHOT:pattern-propagation -->

---

## Phase 6 — Governance &amp; reporting

DORA compliance dashboards aggregate everything:

- RTO / RPO / RTR by application
- Validated coverage of the map
- Drill history with timestamps
- Full audit log accessible via API

<!-- SCREENSHOT:reports -->

The audit log itself is append-only by design — never `UPDATE`, never
`DELETE`. An auditor receives read-only credentials and queries the
data directly.

<!-- SCREENSHOT:audit-export -->

---

## A typical onboarding week, as an example

| Day | Phase | What happens | Where |
|---|---|---|---|
| Mon | 1 | Agents deployed in discovery mode, first CMDB scrape | Captation page |
| Tue | 1.5 | Architecture diagrams uploaded, validated by AI | AI Schema page |
| Wed | 2 | Audit report reviewed, contradictions arbitrated | Audit report |
| Thu | 3 | Map published in advisory mode, knowledge review by sachants | MapView · Knowledge tab |
| Fri | 4 | Activation bumped to diagnostic, first 3-level checks run | Activation page |

By the end of week 1, the application is in advisory + diagnostic
mode, fully cartographied, with knowledge maturity tracked component
by component. Operational levels (3 and 4) come later, when the team
is comfortable.
